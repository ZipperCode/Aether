use serde::Deserialize;
use serde_json::{Map, Value};

use super::official_balance::{
    decimal_string, official_quota_source, scale_decimal, subtract_decimal_clamped,
};
use crate::quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaQueryStatus, ProviderQuotaSource, ProviderQuotaValue,
    ProviderQuotaWindow,
};

#[path = "official_subscription_decimal.rs"]
mod decimal;
#[path = "official_subscription_time.rs"]
mod time;

use decimal::DecimalInput;
use time::rfc3339_unix_secs;

#[derive(Debug)]
pub(super) struct ParsedOfficialSubscription {
    pub sources: Vec<ProviderQuotaSource>,
    pub balances: Vec<ProviderQuotaBalance>,
    pub windows: Vec<ProviderQuotaWindow>,
    pub extensions: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingWindow {
    limit: Option<DecimalInput>,
    used: Option<DecimalInput>,
    remaining: Option<DecimalInput>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KimiCodingWindowSpec {
    duration: Option<u64>,
    #[serde(rename = "timeUnit")]
    time_unit: Option<String>,
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
    if !value.is_object() || value.get("error").is_some_and(|error| !error.is_null()) {
        return Err("invalid coding quota response");
    }
    let mut windows = Vec::new();
    // 官方将 usage 定义为周额度；计量数字并非真实 Token，只输出比例。
    if let Some(window) = value
        .get("usage")
        .and_then(|detail| serde_json::from_value(detail.clone()).ok())
        .and_then(|detail| kimi_window("cycle", "每周配额", detail, Some(7 * 24 * 60)))
    {
        windows.push(window);
    }
    for (index, item) in value
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let spec: Option<KimiCodingWindowSpec> = item
            .get("window")
            .and_then(|window| serde_json::from_value(window.clone()).ok());
        let duration = spec.as_ref().and_then(kimi_window_minutes);
        let label = reported_text(item.get("name"))
            .unwrap_or_else(|| kimi_window_label(index, spec.as_ref()));
        if let Some(window) = item
            .get("detail")
            .and_then(|detail| serde_json::from_value(detail.clone()).ok())
            .and_then(|detail| kimi_window(&format!("window_{index}"), &label, detail, duration))
        {
            windows.push(window);
        }
    }
    // 新版 usages 返回精确比例，替换同周期的整数百分比，并补充实际返回的月度窗口。
    for (code, label, minutes, scope) in [
        ("limit_5h", "5小时配额", Some(300), "account"),
        ("limit_7d", "每周配额", Some(7 * 24 * 60), "account"),
        ("limit_month_total", "月度总额度", None, "unknown"),
        ("limit_month_code", "月度 Code 额度", None, "unknown"),
    ] {
        let Some(detail) = value.get("usages").and_then(|usages| usages.get(code)) else {
            continue;
        };
        let Some(used_ratio) = detail
            .get("used_ratio")
            .and_then(decimal_string)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|ratio| ratio.is_finite() && *ratio >= 0.0)
        else {
            continue;
        };
        let reset_text = reported_text(detail.get("reset_time"));
        let reset_at = reset_text.as_deref().and_then(rfc3339_unix_secs);
        if let Some(window) = windows.iter_mut().find(|window| {
            minutes.is_some() && window.window_minutes == minutes && window.scope == "account"
        }) {
            window.used_ratio = Some(used_ratio);
            window.remaining_ratio = Some((1.0 - used_ratio).max(0.0));
            window.is_exhausted = used_ratio >= 1.0;
            if reset_at.is_some() {
                window.reset_at = reset_at;
                window.reset_at_text = reset_text;
            }
        } else {
            windows.push(ProviderQuotaWindow {
                source_id: Some("subscription".into()),
                code: code.into(),
                label: label.into(),
                // 月度总量与 Code 的抵扣关系尚未确认，仅展示，不扩散为整个 Key 的阻断。
                scope: scope.into(),
                unit: "percent".into(),
                used_ratio: Some(used_ratio),
                remaining_ratio: Some((1.0 - used_ratio).max(0.0)),
                window_minutes: minutes,
                reset_at,
                reset_at_text: reset_text,
                is_exhausted: used_ratio >= 1.0,
                ..Default::default()
            });
        }
    }
    let mut subscription =
        official_quota_source("subscription", "Kimi Coding 套餐", "coding_plan", "account");
    if windows.is_empty() {
        subscription.query_status = ProviderQuotaQueryStatus::Error;
        subscription.freshness = "unknown".into();
        subscription.refresh_state.error = Some("no valid coding quota windows".into());
    }
    let mut extensions = Map::new();
    if let Some(level) = reported_text(value.pointer("/user/membership/level")) {
        subscription.plan_tier = Some(level.clone());
        extensions.insert("membership_level".into(), Value::String(level));
    }
    subscription.plan_id = reported_text(value.get("planId"));
    subscription.plan_name = reported_text(value.get("planName"));
    if let Some(subscription_type) = reported_text(value.get("subType")) {
        extensions.insert("subscription_type".into(), Value::String(subscription_type));
    }
    if let Some(limit) = value.pointer("/parallel/limit").and_then(decimal_string) {
        extensions.insert("parallel_limit".into(), Value::String(limit));
    }
    let mut extra = official_quota_source("extra_usage", "额外用量", "extra_usage", "account");
    let mut balances = Vec::new();
    match value
        .get("boosterWallet")
        .filter(|wallet| !wallet.is_null())
    {
        None => extra.query_status = ProviderQuotaQueryStatus::NotApplicable,
        Some(wallet) => match kimi_extra_usage(wallet) {
            Ok((balance, window, currency_source)) => {
                balances.push(balance);
                windows.extend(window);
                extra.currency_source = Some(currency_source.into());
            }
            Err(error) => {
                extra.query_status = ProviderQuotaQueryStatus::Error;
                extra.freshness = "unknown".into();
                extra.refresh_state.error = Some(error.into());
            }
        },
    }
    if subscription.query_status != ProviderQuotaQueryStatus::Ok
        && extra.query_status != ProviderQuotaQueryStatus::Ok
    {
        return Err("no valid coding quota sources");
    }
    Ok(ParsedOfficialSubscription {
        sources: vec![subscription, extra],
        balances,
        windows,
        extensions,
    })
}

pub(super) fn parse_zhipu_subscription(
    value: &Value,
) -> Result<ParsedOfficialSubscription, &'static str> {
    let response: ZhipuQuotaResponse =
        serde_json::from_value(value.clone()).map_err(|_| "invalid quota response")?;
    if response.success == Some(false)
        || value.get("error").is_some_and(|error| !error.is_null())
        || value
            .get("code")
            .filter(|code| !code.is_null())
            .is_some_and(|code| !matches!(decimal_string(code).as_deref(), Some("0" | "200")))
    {
        return Err("upstream quota response was unsuccessful");
    }
    let mut source = official_quota_source("subscription", "Coding Plan", "coding_plan", "account");
    let mut extensions = Map::new();
    if let Some(level) = response
        .data
        .level
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        source.plan_tier = Some(level.clone());
        extensions.insert(
            "plan_type".into(),
            Value::String(level.to_ascii_lowercase()),
        );
        extensions.insert("pool_tier".into(), Value::String(level));
    }
    source.plan_id = reported_text(value.pointer("/data/planId"));
    source.plan_name = reported_text(value.pointer("/data/planName"));
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
        sources: vec![source],
        balances: Vec::new(),
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
    let limit_number = detail
        .limit
        .as_ref()?
        .finite_number()
        .filter(|limit| *limit > 0.0)?;
    let remaining_ratio = match detail.remaining.as_ref() {
        Some(remaining) => remaining
            .finite_number()
            .filter(|remaining| *remaining >= 0.0)
            .map(|remaining| remaining / limit_number),
        None => detail
            .used
            .as_ref()
            .and_then(DecimalInput::finite_number)
            .filter(|used| *used >= 0.0)
            .map(|used| (1.0 - used / limit_number).max(0.0)),
    }
    .filter(|ratio| ratio.is_finite());
    let reset_at = detail.reset_time.as_deref().and_then(rfc3339_unix_secs);
    Some(ProviderQuotaWindow {
        source_id: Some("subscription".into()),
        code: code.to_owned(),
        label: label.to_owned(),
        scope: "account".into(),
        unit: "percent".into(),
        used_ratio: remaining_ratio.map(|ratio| (1.0 - ratio).max(0.0)),
        remaining_ratio,
        window_minutes,
        reset_at,
        reset_at_text: detail.reset_time,
        is_exhausted: remaining_ratio.is_some_and(|ratio| ratio <= 0.0),
        ..Default::default()
    })
}

/// 钱包固定点数除以 10^8，月度金额 priceInCents 除以 100；两套单位不可复用。
fn kimi_extra_usage(
    wallet: &Value,
) -> Result<
    (
        ProviderQuotaBalance,
        Option<ProviderQuotaWindow>,
        &'static str,
    ),
    &'static str,
> {
    let raw = wallet
        .get("balance")
        .filter(|balance| balance.get("type").and_then(Value::as_str) == Some("BOOSTER"))
        .ok_or("unsupported extra usage wallet")?;
    let available = raw
        .get("amountLeft")
        .and_then(|value| scale_decimal(value, 8));
    let total = raw.get("amount").and_then(|value| scale_decimal(value, 8));
    if available.is_none() && total.is_none() {
        return Err("no valid extra usage wallet amounts");
    }
    let limit_currency = reported_text(wallet.pointer("/monthlyChargeLimit/currency"));
    let used_currency = reported_text(wallet.pointer("/monthlyUsed/currency"));
    if limit_currency
        .as_ref()
        .zip(used_currency.as_ref())
        .is_some_and(|(limit, used)| !limit.eq_ignore_ascii_case(used))
    {
        return Err("extra usage amounts have different currencies");
    }
    let reported_currency = limit_currency.or(used_currency);
    // 官方客户端在钱包未返回币种时固定使用 USD，保留这一来源标记供界面解释。
    let currency_source = if reported_currency.is_some() {
        "upstream"
    } else {
        "official_client_default"
    };
    let unit = reported_currency.unwrap_or_else(|| "USD".into());
    let limit = wallet
        .pointer("/monthlyChargeLimit/priceInCents")
        .and_then(|value| scale_decimal(value, 2));
    let used = wallet
        .pointer("/monthlyUsed/priceInCents")
        .and_then(|value| scale_decimal(value, 2));
    let unlimited = match wallet
        .get("monthlyChargeLimitEnabled")
        .and_then(Value::as_bool)
    {
        Some(false) => Some(true),
        Some(true) => limit.as_deref().map(|limit| limit == "0"),
        None => None,
    };
    let window = if limit.is_some() || used.is_some() || unlimited.is_some() {
        let remaining = if unlimited == Some(false) {
            limit
                .as_deref()
                .zip(used.as_deref())
                .and_then(|(limit, used)| subtract_decimal_clamped(limit, used))
        } else {
            None
        };
        Some(ProviderQuotaWindow {
            source_id: Some("extra_usage".into()),
            code: "monthly_spending_limit".into(),
            label: "额外用量月度消费上限".into(),
            scope: "account".into(),
            unit: unit.clone(),
            limit_value: limit.map(ProviderQuotaValue::Decimal),
            used_value: used.map(ProviderQuotaValue::Decimal),
            remaining_value: remaining.map(ProviderQuotaValue::Decimal),
            unlimited,
            ..Default::default()
        })
    } else {
        None
    };
    Ok((
        ProviderQuotaBalance {
            source_id: Some("extra_usage".into()),
            unit,
            available,
            total,
            ..Default::default()
        },
        window,
        currency_source,
    ))
}

fn reported_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn zhipu_window(limit: ZhipuQuotaLimit) -> Option<ProviderQuotaWindow> {
    let limit_type = limit.limit_type?.trim().to_owned();
    if limit_type.is_empty() {
        return None;
    }
    let used_ratio = limit
        .percentage
        .as_ref()
        .and_then(DecimalInput::finite_number)
        .filter(|percentage| *percentage >= 0.0)
        .map(|percentage| percentage / 100.0);
    let unit = zhipu_u64(limit.unit.as_ref());
    let number = zhipu_u64(limit.number.as_ref());
    let (code, label, quota_unit, window_minutes) = match (limit_type.as_str(), unit, number) {
        ("TOKENS_LIMIT" | "CREDIT_LIMIT", Some(3), Some(hours)) if hours > 0 && hours != 5 => (
            format!(
                "{}_{}h",
                if limit_type == "CREDIT_LIMIT" {
                    "credits"
                } else {
                    "tokens"
                },
                hours
            ),
            format!(
                "{hours}小时{}",
                if limit_type == "CREDIT_LIMIT" {
                    "积分"
                } else {
                    "配额"
                }
            ),
            if limit_type == "CREDIT_LIMIT" {
                "credits"
            } else {
                "percent"
            }
            .into(),
            hours.checked_mul(60),
        ),
        ("TOKENS_LIMIT" | "CREDIT_LIMIT", Some(6), Some(weeks)) if weeks > 1 => (
            format!(
                "{}_{}w",
                if limit_type == "CREDIT_LIMIT" {
                    "credits"
                } else {
                    "tokens"
                },
                weeks
            ),
            format!(
                "{weeks}周{}",
                if limit_type == "CREDIT_LIMIT" {
                    "积分"
                } else {
                    "配额"
                }
            ),
            if limit_type == "CREDIT_LIMIT" {
                "credits"
            } else {
                "percent"
            }
            .into(),
            weeks.checked_mul(7 * 24 * 60),
        ),
        ("TOKENS_LIMIT", Some(3), Some(5)) => (
            "tokens_5h".to_string(),
            "5小时配额".to_string(),
            "percent".to_string(),
            Some(300),
        ),
        ("TOKENS_LIMIT", Some(6), Some(1)) => (
            "tokens_weekly".to_string(),
            "每周配额".to_string(),
            "percent".to_string(),
            Some(7 * 24 * 60),
        ),
        ("TOKENS_LIMIT", _, _) => (
            "tokens_limit".to_string(),
            "套餐配额".to_string(),
            "percent".to_string(),
            None,
        ),
        ("CREDIT_LIMIT", Some(3), Some(5)) => (
            "credits_5h".to_string(),
            "5小时积分".to_string(),
            "credits".to_string(),
            Some(300),
        ),
        ("CREDIT_LIMIT", Some(6), Some(1)) => (
            "credits_weekly".to_string(),
            "每周积分".to_string(),
            "credits".to_string(),
            Some(7 * 24 * 60),
        ),
        ("CREDIT_LIMIT", _, _) => (
            "credits_limit".to_string(),
            "积分额度".to_string(),
            "credits".to_string(),
            None,
        ),
        ("TIME_LIMIT", _, _) => (
            "time_limit".to_string(),
            "MCP 月度额度".to_string(),
            "count".to_string(),
            None,
        ),
        _ => (
            limit_type.to_ascii_lowercase(),
            limit_type.clone(),
            "percent".to_string(),
            None,
        ),
    };
    let reset_at = zhipu_reset_at(limit.next_reset_time.as_ref().or(limit.reset_at.as_ref()));
    let reset_at_text = limit
        .reset_at
        .as_ref()
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let absolute_values = matches!(limit_type.as_str(), "CREDIT_LIMIT" | "TIME_LIMIT");
    let total = absolute_values
        .then(|| limit.usage.as_ref().and_then(DecimalInput::decimal_text))
        .flatten();
    let used = absolute_values
        .then(|| {
            limit
                .current_value
                .as_ref()
                .and_then(DecimalInput::decimal_text)
        })
        .flatten();
    Some(ProviderQuotaWindow {
        source_id: Some("subscription".into()),
        code,
        label,
        scope: match limit_type.as_str() {
            "TIME_LIMIT" => "tool",
            "TOKENS_LIMIT" | "CREDIT_LIMIT" => "account",
            _ => "unknown",
        }
        .into(),
        unit: quota_unit,
        remaining_value: total
            .as_deref()
            .zip(used.as_deref())
            .and_then(|(total, used)| subtract_decimal_clamped(total, used))
            .map(ProviderQuotaValue::Decimal),
        used_value: used.map(ProviderQuotaValue::Decimal),
        limit_value: total.map(ProviderQuotaValue::Decimal),
        used_ratio,
        remaining_ratio: used_ratio.map(|ratio| (1.0 - ratio).clamp(0.0, 1.0)),
        window_minutes,
        reset_at,
        reset_at_text,
        is_exhausted: used_ratio.is_some_and(|ratio| ratio >= 1.0),
        ..Default::default()
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
    match window.and_then(kimi_window_minutes) {
        Some(minutes) if minutes % (7 * 24 * 60) == 0 => {
            format!("{}周配额", minutes / (7 * 24 * 60))
        }
        Some(minutes) if minutes % (24 * 60) == 0 => format!("{}天配额", minutes / (24 * 60)),
        Some(minutes) if minutes % 60 == 0 => format!("{}小时配额", minutes / 60),
        Some(minutes) => format!("{minutes}分钟配额"),
        _ => format!("窗口 {}", index + 1),
    }
}

fn kimi_window_minutes(window: &KimiCodingWindowSpec) -> Option<u64> {
    let duration = window.duration.filter(|duration| *duration > 0)?;
    let factor = match window.time_unit.as_deref()? {
        "TIME_UNIT_MINUTE" => 1,
        "TIME_UNIT_HOUR" => 60,
        "TIME_UNIT_DAY" => 24 * 60,
        "TIME_UNIT_WEEK" => 7 * 24 * 60,
        _ => return None,
    };
    duration.checked_mul(factor)
}
