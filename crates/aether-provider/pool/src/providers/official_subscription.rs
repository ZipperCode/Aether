use serde::Deserialize;
use serde_json::{Map, Number, Value};

use crate::quota_snapshot::{ProviderQuotaValue, ProviderQuotaWindow};

#[path = "official_subscription_decimal.rs"]
mod decimal;
#[path = "official_subscription_time.rs"]
mod time;

use decimal::DecimalInput;
use time::rfc3339_unix_secs;

#[derive(Debug)]
pub(super) struct ParsedOfficialSubscription {
    pub windows: Vec<ProviderQuotaWindow>,
    pub extensions: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingResponse {
    usage: KimiCodingWindow,
    #[serde(default)]
    limits: Vec<KimiCodingLimit>,
    user: Option<KimiCodingUser>,
    parallel: Option<KimiCodingParallel>,
    #[serde(rename = "subType")]
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingWindow {
    limit: DecimalInput,
    used: Option<DecimalInput>,
    remaining: Option<DecimalInput>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingLimit {
    window: Option<KimiCodingWindowSpec>,
    detail: Option<KimiCodingWindow>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingWindowSpec {
    duration: Option<u64>,
    #[serde(rename = "timeUnit")]
    time_unit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingUser {
    membership: Option<KimiCodingMembership>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingMembership {
    level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingParallel {
    limit: Option<DecimalInput>,
}

#[derive(Debug, Deserialize)]
struct ZhipuQuotaResponse {
    success: Option<bool>,
    data: ZhipuQuotaData,
}

#[derive(Debug, Deserialize)]
struct ZhipuQuotaData {
    level: Option<String>,
    limits: Vec<ZhipuQuotaLimit>,
}

#[derive(Debug, Deserialize)]
struct ZhipuQuotaLimit {
    #[serde(rename = "type")]
    limit_type: Option<String>,
    #[serde(rename = "currentValue")]
    current_value: Option<DecimalInput>,
    usage: Option<DecimalInput>,
    percentage: Option<DecimalInput>,
    #[serde(rename = "resetAt")]
    reset_at: Option<Value>,
    unit: Option<Value>,
    number: Option<Value>,
    #[serde(rename = "nextResetTime")]
    next_reset_time: Option<Value>,
}

pub(super) fn parse_kimi_coding_subscription(
    value: &Value,
) -> Result<ParsedOfficialSubscription, &'static str> {
    let response: KimiCodingResponse =
        serde_json::from_value(value.clone()).map_err(|_| "invalid coding quota response")?;
    let mut windows = Vec::with_capacity(response.limits.len().saturating_add(1));
    if let Some(window) = kimi_window("cycle", "周期配额", response.usage, None) {
        windows.push(window);
    }
    for (index, item) in response.limits.into_iter().enumerate() {
        let duration = item.window.as_ref().and_then(|window| window.duration);
        let label = kimi_window_label(index, item.window.as_ref());
        if let Some(window) = item
            .detail
            .and_then(|detail| kimi_window(&format!("window_{index}"), &label, detail, duration))
        {
            windows.push(window);
        }
    }
    if windows.is_empty() {
        return Err("no valid coding quota windows");
    }

    let mut extensions = Map::new();
    if let Some(level) = response
        .user
        .and_then(|user| user.membership)
        .and_then(|membership| membership.level)
    {
        extensions.insert("membership_level".into(), Value::String(level));
    }
    if let Some(subscription_type) = response.subscription_type {
        extensions.insert("subscription_type".into(), Value::String(subscription_type));
    }
    if let Some(limit) = response
        .parallel
        .and_then(|parallel| parallel.limit)
        .and_then(|limit| limit.decimal_text())
    {
        extensions.insert("parallel_limit".into(), Value::String(limit));
    }
    Ok(ParsedOfficialSubscription {
        windows,
        extensions,
    })
}

pub(super) fn parse_zhipu_subscription(
    value: &Value,
) -> Result<ParsedOfficialSubscription, &'static str> {
    let response: ZhipuQuotaResponse =
        serde_json::from_value(value.clone()).map_err(|_| "invalid quota response")?;
    let mut extensions = Map::new();
    if let Some(level) = response
        .data
        .level
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        extensions.insert(
            "plan_type".into(),
            Value::String(level.to_ascii_lowercase()),
        );
        extensions.insert("pool_tier".into(), Value::String(level));
    }
    if response.success == Some(false) {
        return Err("upstream quota response was unsuccessful");
    }
    let windows = response
        .data
        .limits
        .into_iter()
        .filter_map(zhipu_window)
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return Err("no valid quota limits");
    }
    Ok(ParsedOfficialSubscription {
        windows,
        extensions,
    })
}

fn kimi_window(
    code: &str,
    label: &str,
    detail: KimiCodingWindow,
    window_minutes: Option<u64>,
) -> Option<ProviderQuotaWindow> {
    let limit_number = detail.limit.finite_number()?;
    let limit_value = detail.limit.quota_value()?;
    let remaining_number = detail
        .remaining
        .as_ref()
        .and_then(DecimalInput::finite_number);
    let remaining_ratio = remaining_number
        .filter(|_| limit_number > 0.0)
        .map(|remaining| remaining / limit_number);
    let reset_at = detail.reset_time.as_deref().and_then(rfc3339_unix_secs);
    Some(ProviderQuotaWindow {
        code: code.to_owned(),
        label: label.to_owned(),
        scope: "account".into(),
        unit: "count".into(),
        used_value: detail.used.as_ref().and_then(DecimalInput::quota_value),
        remaining_value: detail
            .remaining
            .as_ref()
            .and_then(DecimalInput::quota_value),
        limit_value: Some(limit_value),
        used_ratio: None,
        remaining_ratio,
        window_minutes,
        reset_at,
        reset_at_text: detail.reset_time,
        is_exhausted: remaining_number.is_some_and(|remaining| remaining <= 0.0),
    })
}

fn zhipu_window(limit: ZhipuQuotaLimit) -> Option<ProviderQuotaWindow> {
    let limit_type = limit.limit_type?.trim().to_owned();
    if limit_type.is_empty() {
        return None;
    }
    let total = limit.usage.as_ref().and_then(DecimalInput::finite_number);
    let used = limit
        .current_value
        .as_ref()
        .and_then(DecimalInput::finite_number);
    let used_ratio = limit
        .percentage
        .as_ref()
        .and_then(DecimalInput::finite_number)
        .filter(|percentage| *percentage >= 0.0)
        .map(|percentage| percentage / 100.0);
    let unit = zhipu_u64(limit.unit.as_ref());
    let number = zhipu_u64(limit.number.as_ref());
    let (code, label, window_minutes) = match (limit_type.as_str(), unit, number) {
        ("TOKENS_LIMIT", Some(3), Some(5)) => (
            "tokens_5h".to_string(),
            "5小时 Token 配额".to_string(),
            Some(300),
        ),
        ("TOKENS_LIMIT", Some(6), Some(1)) => (
            "tokens_weekly".to_string(),
            "每周 Token 配额".to_string(),
            Some(7 * 24 * 60),
        ),
        ("TOKENS_LIMIT", _, _) => ("tokens_limit".to_string(), "Token 配额".to_string(), None),
        _ => (limit_type.to_ascii_lowercase(), limit_type.clone(), None),
    };
    let reset_at = zhipu_reset_at(limit.next_reset_time.as_ref().or(limit.reset_at.as_ref()));
    let reset_at_text = limit
        .reset_at
        .as_ref()
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Some(ProviderQuotaWindow {
        code,
        label,
        scope: "account".into(),
        unit: "tokens".into(),
        used_value: limit
            .current_value
            .as_ref()
            .and_then(DecimalInput::quota_value),
        remaining_value: match (total, used) {
            (Some(total), Some(used)) => {
                Number::from_f64((total - used).max(0.0)).map(ProviderQuotaValue::Number)
            }
            _ => None,
        },
        limit_value: limit.usage.as_ref().and_then(DecimalInput::quota_value),
        used_ratio,
        remaining_ratio: used_ratio.map(|ratio| (1.0 - ratio).clamp(0.0, 1.0)),
        window_minutes,
        reset_at,
        reset_at_text,
        is_exhausted: used_ratio.is_some_and(|ratio| ratio >= 1.0),
    })
}

/// 将智谱响应中可能为数字或字符串的整数安全归一化为 u64。
fn zhipu_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
            .or_else(|| value.as_str().and_then(|number| number.trim().parse().ok()))
    })
}

/// 兼容 RFC3339、Unix 秒以及智谱 `nextResetTime` 的 Unix 毫秒格式。
fn zhipu_reset_at(value: Option<&Value>) -> Option<u64> {
    if let Some(text) = value.and_then(Value::as_str).map(str::trim) {
        if let Some(timestamp) = rfc3339_unix_secs(text) {
            return Some(timestamp);
        }
    }
    let timestamp = zhipu_u64(value)?;
    Some(if timestamp >= 10_000_000_000 {
        timestamp / 1_000
    } else {
        timestamp
    })
}

fn kimi_window_label(index: usize, window: Option<&KimiCodingWindowSpec>) -> String {
    match window {
        Some(KimiCodingWindowSpec {
            duration: Some(300),
            time_unit: Some(unit),
        }) if unit == "TIME_UNIT_MINUTE" => "5小时配额".into(),
        Some(KimiCodingWindowSpec {
            duration: Some(duration),
            time_unit: Some(unit),
        }) if unit == "TIME_UNIT_MINUTE" && duration % 60 == 0 => {
            format!("{}小时配额", duration / 60)
        }
        _ => format!("窗口 {}", index + 1),
    }
}
