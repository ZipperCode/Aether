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
    reset_at: Option<String>,
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
) -> Result<Vec<ProviderQuotaWindow>, &'static str> {
    let response: ZhipuQuotaResponse =
        serde_json::from_value(value.clone()).map_err(|_| "invalid quota response")?;
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
    Ok(windows)
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
    Some(ProviderQuotaWindow {
        code: limit_type.to_ascii_lowercase(),
        label: if limit_type == "TOKENS_LIMIT" {
            "5小时 Token 配额".into()
        } else {
            limit_type
        },
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
        remaining_ratio: None,
        window_minutes: None,
        reset_at: limit.reset_at.as_deref().and_then(rfc3339_unix_secs),
        reset_at_text: limit.reset_at,
        is_exhausted: used_ratio.is_some_and(|ratio| ratio >= 1.0),
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
