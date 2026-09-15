use serde_json::Value;

use super::official_balance::{decimal_string, official_quota_source, subtract_decimal_clamped};
use crate::quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaSnapshotContract, ProviderQuotaValue, ProviderQuotaWindow,
};

/// 遵循官方 CLI 的余额与 Token Plan 响应格式；套餐额度仅观察，不作 Key 级阻断。
pub(super) fn parse_minimax_quota(
    value: &Value,
    now_unix_secs: u64,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return Err("upstream quota response was unsuccessful");
    }
    if let Some(base) = value.get("base_resp") {
        if base.get("status_code").and_then(decimal_string).as_deref() != Some("0") {
            return Err("upstream quota response was unsuccessful");
        }
    }
    if value.get("model_remains").is_none() {
        return parse_balance(value);
    }
    let models = value
        .get("model_remains")
        .and_then(Value::as_array)
        .ok_or("missing Token Plan quota data")?;
    let mut windows = Vec::new();
    for (index, model) in models.iter().enumerate() {
        let Some(name) = model
            .get("model_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        // 两种窗口同时 status=3 且总量为零表示套餐未包含该池，不能显示无限额。
        let excluded = number(model, "current_interval_total_count") == Some(0.0)
            && number(model, "current_weekly_total_count") == Some(0.0)
            && number(model, "current_interval_status") == Some(3.0)
            && number(model, "current_weekly_status") == Some(3.0);
        for weekly in [false, true] {
            if let Some(window) = quota_window(model, name, index, weekly, excluded, now_unix_secs)
            {
                windows.push(window);
            }
        }
    }
    if windows.is_empty() {
        return Err("no valid Token Plan quota windows");
    }
    let mut snapshot =
        ProviderQuotaSnapshotContract::subscription("minimax", windows, now_unix_secs);
    // 订阅额度可由已购积分兜底，任一模型池耗尽不代表当前 Key 已不可用。
    snapshot.exhausted = false;
    let mut source = official_quota_source("subscription", "Token Plan", "token_plan", "account");
    // 只透传服务器明确报告的身份；额度大小及模型列表不用于反推套餐档位。
    source.plan_id = reported_plan_text(value, "plan_id", "planId");
    source.plan_name = reported_plan_text(value, "plan_name", "planName");
    source.plan_tier = reported_plan_text(value, "plan_tier", "planTier");
    snapshot.sources.push(source);
    Ok(snapshot)
}

fn reported_plan_text(value: &Value, snake: &str, camel: &str) -> Option<String> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|value| match value {
            Value::String(text) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
}

fn parse_balance(value: &Value) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    let available = value
        .get("available_amount")
        .and_then(decimal_string)
        .ok_or("missing available_amount")?;
    let unit = value
        .get("currency")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_owned();
    let mut source = official_quota_source("balance", "账户余额", "account_balance", "account");
    // 官方响应未承诺币种，不能凭区域把原值当作 CNY/USD，也不作倍率换算。
    source.currency_source = Some(
        if unit.is_empty() {
            "unknown"
        } else {
            "upstream"
        }
        .into(),
    );
    let mut snapshot = ProviderQuotaSnapshotContract::balance(
        "minimax",
        vec![ProviderQuotaBalance {
            source_id: Some("balance".into()),
            unit,
            available: Some(available),
            granted: value.get("voucher_balance").and_then(decimal_string),
            topped_up: value.get("cash_balance").and_then(decimal_string),
            ..Default::default()
        }],
    );
    snapshot.sources.push(source);
    Ok(snapshot)
}

fn quota_window(
    model: &Value,
    name: &str,
    index: usize,
    weekly: bool,
    excluded: bool,
    now_unix_secs: u64,
) -> Option<ProviderQuotaWindow> {
    let period = if weekly { "weekly" } else { "interval" };
    let prefix = format!("current_{period}");
    let status = number(model, &format!("{prefix}_status"));
    let unlimited = !excluded && status == Some(3.0);
    let total = model
        .get(format!("{prefix}_total_count"))
        .and_then(decimal_string);
    let reported = model
        .get(format!("{prefix}_usage_count"))
        .and_then(decimal_string);
    let percent =
        number(model, &format!("{prefix}_remaining_percent")).filter(|percent| *percent >= 0.0);
    let counts = reported
        .as_deref()
        .zip(total.as_deref())
        .and_then(|(reported, total)| resolve_counts(reported, total, percent));
    let base_ratio = percent.map(|percent| percent / 100.0).or_else(|| {
        counts.as_ref().and_then(|(_, remaining, total)| {
            finite(remaining)
                .zip(finite(total))
                .map(|(remaining, total)| remaining / total)
        })
    });
    let boost = if weekly {
        number(model, "weekly_boost_permille").map_or(1.0, |boost| boost.max(0.0) / 1000.0)
    } else {
        1.0
    };
    let remaining_ratio = if excluded || unlimited {
        None
    } else {
        base_ratio
            .map(|ratio| ratio * boost)
            .filter(|ratio| ratio.is_finite())
    };
    if !excluded
        && !unlimited
        && counts.is_none()
        && remaining_ratio.is_none()
        && status != Some(2.0)
    {
        return None;
    }
    let (used_value, remaining_value, limit_value) = if excluded || unlimited {
        (None, None, None)
    } else if let Some((used, remaining, total)) = counts {
        (
            Some(ProviderQuotaValue::Decimal(used)),
            Some(ProviderQuotaValue::Decimal(remaining)),
            Some(ProviderQuotaValue::Decimal(total)),
        )
    } else {
        (None, None, None)
    };
    let start = millis(
        model,
        if weekly {
            "weekly_start_time"
        } else {
            "start_time"
        },
    );
    let end = millis(
        model,
        if weekly {
            "weekly_end_time"
        } else {
            "end_time"
        },
    );
    let reset_at = end.map(|end| end / 1_000).or_else(|| {
        millis(
            model,
            if weekly {
                "weekly_remains_time"
            } else {
                "remains_time"
            },
        )
        .and_then(|remaining| now_unix_secs.checked_add(remaining / 1_000))
    });
    Some(ProviderQuotaWindow {
        source_id: Some("subscription".into()),
        code: format!("model_{index}_{period}"),
        label: format!("{name} · {}", if weekly { "周额度" } else { "周期额度" }),
        scope: "model".into(),
        unit: "quota".into(),
        used_value,
        remaining_value,
        limit_value,
        // 加成可使文字超过 100%；只由展示层约束进度条几何宽度。
        remaining_ratio,
        window_minutes: start
            .zip(end)
            .and_then(|(start, end)| end.checked_sub(start))
            .map(|length| length / 60_000)
            .filter(|length| *length > 0),
        reset_at,
        unlimited: Some(unlimited),
        is_included: if excluded {
            Some(false)
        } else if status.is_some() || total.is_some() {
            Some(true)
        } else {
            None
        },
        is_exhausted: !excluded
            && !unlimited
            && (status == Some(2.0) || remaining_ratio.is_some_and(|ratio| ratio <= 0.0)),
        ..Default::default()
    })
}

/// usage_count 在旧版表示剩余、新版可能表示已用；以误差 1% 内的剩余比例选择解释。
fn resolve_counts(
    reported: &str,
    total: &str,
    percent: Option<f64>,
) -> Option<(String, String, String)> {
    let reported_number = finite(reported)?;
    let total_number = finite(total)?;
    if total_number <= 0.0 || reported_number < 0.0 || reported_number > total_number {
        return None;
    }
    let mut remaining = reported.to_owned();
    if let Some(percent) = percent {
        let as_remaining = reported_number / total_number * 100.0;
        let as_used = (total_number - reported_number) / total_number * 100.0;
        let remaining_distance = (as_remaining - percent).abs();
        let used_distance = (as_used - percent).abs();
        if remaining_distance.min(used_distance) > 1.0 {
            return None;
        }
        if used_distance < remaining_distance {
            remaining = subtract_decimal_clamped(total, reported)?;
        }
    }
    let used = subtract_decimal_clamped(total, &remaining)?;
    Some((used, remaining, total.to_owned()))
}

fn finite(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(decimal_string)
        .as_deref()
        .and_then(finite)
}

fn millis(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(decimal_string)?.parse().ok()
}
