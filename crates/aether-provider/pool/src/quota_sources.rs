use serde_json::{Map, Value};

use crate::quota::{
    provider_pool_json_bool, provider_pool_json_f64, provider_pool_timestamp_unix_secs,
};

/// 对来源化快照重新计算账号级耗尽；旧快照保持原字段，兼容其他 OAuth 适配器。
pub fn provider_quota_snapshot_exhausted(snapshot: &Value, now_unix_secs: u64) -> bool {
    let Some(snapshot) = snapshot.as_object() else {
        return false;
    };
    source_account_exhausted(snapshot, now_unix_secs)
        .unwrap_or_else(|| provider_pool_json_bool(snapshot.get("exhausted")) == Some(true))
}

fn sources(snapshot: &Map<String, Value>) -> Option<&[Value]> {
    let value = snapshot.get("sources")?;
    match value.as_array() {
        Some(values) if values.is_empty() => None,
        Some(values) => Some(values),
        // 非法来源结构不能退回旧字段并意外触发阻断。
        None => Some(&[]),
    }
}

fn source_not_applicable(source: &Map<String, Value>) -> bool {
    matches!(
        source.get("query_status").and_then(Value::as_str),
        Some("unsupported" | "not_applicable")
    )
}

fn source_is_fresh(source: &Map<String, Value>) -> bool {
    source.get("query_status").and_then(Value::as_str) == Some("ok")
        && source.get("freshness").and_then(Value::as_str) == Some("fresh")
}

/// 不同套餐可以独立供给；只有每个已确认适用的套餐均耗尽才形成账号约束。
/// 余额、额外用量或失败来源的抵扣关系未知时，不能用某个套餐耗尽替代整个 Key 的事实。
pub(crate) fn source_account_exhausted(
    snapshot: &Map<String, Value>,
    now_unix_secs: u64,
) -> Option<bool> {
    let sources = sources(snapshot)?;
    if now_unix_secs == 0
        || snapshot.get("provider_type").and_then(Value::as_str) == Some("minimax")
    {
        return Some(false);
    }
    let windows = snapshot.get("windows").and_then(Value::as_array);
    let mut saw_plan = false;
    for source in sources {
        let Some(source) = source.as_object() else {
            return Some(false);
        };
        if source_not_applicable(source) {
            continue;
        }
        if !source_is_fresh(source)
            || !matches!(
                source.get("product").and_then(Value::as_str),
                Some("coding_plan" | "token_plan")
            )
        {
            return Some(false);
        }
        let Some(id) = source
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Some(false);
        };
        saw_plan = true;
        let exhausted = windows
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .any(|window| {
                window.get("source_id").and_then(Value::as_str) == Some(id)
                    && matches!(
                        window.get("scope").and_then(Value::as_str),
                        Some("account" | "key")
                    )
                    && provider_pool_json_bool(window.get("is_included")) != Some(false)
                    && provider_pool_json_bool(window.get("unlimited")) != Some(true)
                    // 绝对计数与比例均须有明确单位，未知原始值不能形成调度事实。
                    && matches!(
                        window.get("unit").and_then(Value::as_str),
                        Some("percent" | "credits" | "count" | "tokens" | "quota")
                    )
                    && window_exhausted(window)
                    && provider_pool_timestamp_unix_secs(window.get("reset_at"))
                        .is_some_and(|reset_at| reset_at > now_unix_secs)
            });
        if !exhausted {
            return Some(false);
        }
    }
    Some(saw_plan)
}

fn window_exhausted(window: &Map<String, Value>) -> bool {
    provider_pool_json_bool(window.get("is_exhausted"))
        .or_else(|| provider_pool_json_f64(window.get("used_ratio")).map(|ratio| ratio >= 1.0))
        .or_else(|| provider_pool_json_f64(window.get("remaining_ratio")).map(|ratio| ratio <= 0.0))
        .unwrap_or(false)
}

/// 只有明确的纯金额来源才适用既有阈值；套餐与钱包并存时不推断上游抵扣策略。
pub(crate) fn source_balance_below_minimum(
    snapshot: &Map<String, Value>,
    minimum: f64,
) -> Option<bool> {
    let sources = sources(snapshot)?;
    if snapshot.get("provider_type").and_then(Value::as_str) == Some("minimax") {
        return Some(false);
    }
    let balances = snapshot.get("balances").and_then(Value::as_array);
    let mut saw_balance = false;
    for source in sources {
        let Some(source) = source.as_object() else {
            return Some(false);
        };
        if source_not_applicable(source) {
            continue;
        }
        // 免费模型请求数不是资金来源：不参与金额阈值，也不能把金额推断降级为未知，
        // 否则 Key 消费限额的低余额事实会被并存的免费来源意外取消。
        if source.get("product").and_then(Value::as_str) == Some("model_requests") {
            continue;
        }
        if !source_is_fresh(source)
            || !matches!(
                source.get("product").and_then(Value::as_str),
                Some("account_balance" | "key_spending_limit")
            )
        {
            return Some(false);
        }
        if source.get("product").and_then(Value::as_str) == Some("key_spending_limit")
            && provider_pool_json_bool(snapshot.get("key_limit_unlimited")) == Some(true)
        {
            // 无消费上限不能被响应中附带的剩余额度零值反向解释为限额不足。
            return Some(false);
        }
        let Some(id) = source
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Some(false);
        };
        let mut entries = balances
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .filter(|balance| balance.get("source_id").and_then(Value::as_str) == Some(id))
            .peekable();
        if entries.peek().is_none() {
            return Some(false);
        }
        // 当前接入中仅 CNY/USD 有明确货币依据，其他三字母字符串不能自动当成币种。
        if !entries.all(|balance| {
            matches!(
                balance.get("unit").and_then(Value::as_str),
                Some("CNY" | "USD")
            ) && decimal_at_most(balance.get("available"), minimum)
        }) {
            return Some(false);
        }
        saw_balance = true;
    }
    Some(saw_balance)
}

fn decimal_at_most(value: Option<&Value>, minimum: f64) -> bool {
    let Some(number) = provider_pool_json_f64(value) else {
        return false;
    };
    if number != minimum {
        return number < minimum;
    }
    let Some(text) = value.and_then(Value::as_str) else {
        return true;
    };
    // 解析器已输出普通十进制字符串；临界值不能因 f64 舍入把 1.000…001 当作 1。
    let text = text.trim().trim_start_matches('+');
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|digit| digit.is_ascii_digit())
        || !fraction.bytes().all(|digit| digit.is_ascii_digit())
    {
        return false;
    }
    let minimum = minimum.to_string();
    let (limit_whole, limit_fraction) = minimum.split_once('.').unwrap_or((&minimum, ""));
    let whole = whole.trim_start_matches('0');
    let limit_whole = limit_whole.trim_start_matches('0');
    (whole.len(), whole, fraction.trim_end_matches('0'))
        <= (
            limit_whole.len(),
            limit_whole,
            limit_fraction.trim_end_matches('0'),
        )
}
