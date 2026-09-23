use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use crate::capability::{ProviderPoolCapabilities, ProviderQuotaServingPolicy};
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use crate::quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaSnapshotContract, ProviderQuotaValue, ProviderQuotaWindow,
};

use super::official_balance::{
    decimal_string, endpoint_has_official_origin, official_quota_source,
};

pub const OPENROUTER_CREDITS_URL: &str = "https://openrouter.ai/api/v1/key";
const OPENROUTER_HOST: &str = "openrouter.ai";
pub fn is_official_openrouter_endpoint(endpoint: &StoredProviderCatalogEndpoint) -> bool {
    endpoint_has_official_origin(endpoint, OPENROUTER_HOST)
}

/// 额度请求出站 URL 的 origin 校验复用官方端点同一域名，不另建白名单。
pub fn openrouter_quota_url_host_is_allowed(host: &str) -> bool {
    host.eq_ignore_ascii_case(OPENROUTER_HOST)
}

#[derive(Debug, Clone, Default)]
pub struct OpenRouterProviderPoolAdapter;

impl ProviderPoolAdapter for OpenRouterProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        "openrouter"
    }
    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            quota_refresh: true,
            ..Default::default()
        }
    }
    fn quota_serving_policy(&self) -> Option<ProviderQuotaServingPolicy> {
        Some(ProviderQuotaServingPolicy::ObservationOnly)
    }
    fn quota_refresh_endpoint(
        &self,
        endpoints: &[StoredProviderCatalogEndpoint],
        include_inactive: bool,
    ) -> Option<StoredProviderCatalogEndpoint> {
        provider_pool_matching_endpoint(endpoints, include_inactive, |endpoint| {
            endpoint_has_official_origin(endpoint, OPENROUTER_HOST)
        })
    }
}

/// 普通 Key 仅查询自身消费限额；账户余额接口要求管理密钥。
pub fn build_openrouter_credits_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    if !is_official_openrouter_endpoint(endpoint) {
        return Err("quota unsupported for non-official OpenRouter endpoint");
    }
    let raw_secret = read_secret();
    let secret = normalize_bearer_secret(&raw_secret);
    Ok(ProviderPoolQuotaRequestSpec {
        request_id: format!("openrouter-key-quota-{key_id}"),
        provider_name: "openrouter".into(),
        quota_kind: "balance".into(),
        method: "GET".into(),
        url: OPENROUTER_CREDITS_URL.into(),
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("authorization".into(), format!("Bearer {secret}")),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
    })
}

/// 免费请求数是非负整数计数：仅接受全数字十进制整数字符串，
/// 小数、负数与科学计数法展开后的小数一律视为未知，不经浮点换算。
fn free_request_count(value: &Value) -> Option<String> {
    let text = decimal_string(value)?;
    text.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then_some(text)
}

/// 精确判断归一化后的计数是否为零，避免 f64 下溢把微小正数误判为耗尽。
fn free_request_count_is_zero(text: &str) -> bool {
    text.bytes().all(|byte| byte == b'0')
}

fn normalize_bearer_secret(secret: &str) -> &str {
    let trimmed = secret.trim();
    trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed)
        .trim()
}

pub fn parse_openrouter_credits(
    value: &Value,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return Err("upstream key limit response was unsuccessful");
    }
    let data = value
        .get("data")
        .filter(|data| data.is_object())
        .ok_or("missing key limit data")?;
    let total = data.get("limit").and_then(decimal_string);
    let used = data.get("usage").and_then(decimal_string);
    // usage 是累计消费，limit 可能按周期重置，两者不能相减推断剩余额度。
    let remaining = data.get("limit_remaining").and_then(decimal_string);
    let unlimited = total.is_none() && data.get("limit").is_some_and(Value::is_null);
    let balances = if remaining.is_some() || total.is_some() || used.is_some() {
        vec![ProviderQuotaBalance {
            source_id: Some("key_limit".into()),
            unit: "USD".into(),
            available: remaining,
            total,
            granted: None,
            topped_up: None,
            used,
        }]
    } else {
        Vec::new()
    };
    if balances.is_empty() && !unlimited {
        return Err("no valid key spending limit fields");
    }
    let mut snapshot = ProviderQuotaSnapshotContract::balance("openrouter", balances);
    let mut source =
        official_quota_source("key_limit", "Key 消费限额", "key_spending_limit", "key");
    source.region = Some("global".into());
    source.currency_source = Some("official_documentation".into());
    snapshot.sources.push(source);
    // 免费模型每日请求数是独立计数来源；仅透传上游字段，不参与金额事实，也不推断重置时间。
    if let Some(free) = data
        .get("free_model_daily_requests")
        .filter(|free| free.is_object())
    {
        let limit = free.get("limit").and_then(free_request_count);
        let used = free.get("used").and_then(free_request_count);
        let remaining = free.get("remaining").and_then(free_request_count);
        if limit.is_some() || used.is_some() || remaining.is_some() {
            snapshot.windows.push(ProviderQuotaWindow {
                source_id: Some("free_requests".into()),
                code: "free_model_daily_requests".into(),
                label: "免费模型每日请求".into(),
                scope: "model".into(),
                unit: "requests".into(),
                used_value: used.map(ProviderQuotaValue::Decimal),
                remaining_value: remaining.clone().map(ProviderQuotaValue::Decimal),
                limit_value: limit.map(ProviderQuotaValue::Decimal),
                // 仅归一化整数零值视为当日耗尽；缺失、小数、负数或非有限输入保持未知。
                is_exhausted: remaining.as_deref().is_some_and(free_request_count_is_zero),
                ..Default::default()
            });
            let mut free_source = official_quota_source(
                "free_requests",
                "免费模型每日请求",
                "model_requests",
                "model",
            );
            free_source.region = Some("global".into());
            snapshot.sources.push(free_source);
        }
    }
    if unlimited {
        snapshot
            .extensions
            .insert("key_limit_unlimited".into(), Value::Bool(true));
    }
    for field in [
        "label",
        "is_free_tier",
        "is_management_key",
        "is_provisioning_key",
        "limit_reset",
        "include_byok_in_limit",
        "expires_at",
    ] {
        if let Some(value) = data.get(field).filter(|value| !value.is_null()) {
            snapshot.extensions.insert(field.into(), value.clone());
        }
    }
    for field in [
        "usage_daily",
        "usage_weekly",
        "usage_monthly",
        "byok_usage",
        "byok_usage_daily",
        "byok_usage_weekly",
        "byok_usage_monthly",
    ] {
        if let Some(amount) = data.get(field).and_then(decimal_string) {
            snapshot
                .extensions
                .insert(field.into(), Value::String(amount));
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_pasted_bearer_prefix() {
        assert_eq!(
            normalize_bearer_secret(" Bearer sk-or-v1-test "),
            "sk-or-v1-test"
        );
        assert_eq!(
            normalize_bearer_secret("bearer sk-or-v1-test"),
            "sk-or-v1-test"
        );
        assert_eq!(normalize_bearer_secret("sk-or-v1-test"), "sk-or-v1-test");
    }
    use serde_json::json;
    use std::cell::Cell;

    fn endpoint(base_url: &str) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "e".into(),
            "p".into(),
            "openai:chat".into(),
            None,
            None,
            true,
        )
        .unwrap()
        .with_transport_fields(
            base_url.into(),
            Some(json!({"authorization":"attacker"})),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn request_is_fixed_isolated_and_redirect_closed() {
        let spec = build_openrouter_credits_request(
            "k",
            &endpoint("https://openrouter.ai/api/v1"),
            || "key".into(),
        )
        .unwrap();
        assert_eq!(spec.url, OPENROUTER_CREDITS_URL);
        assert_eq!(spec.headers.len(), 2);
        assert_eq!(spec.headers["authorization"], "Bearer key");
    }

    #[test]
    fn rejects_nonofficial_endpoint_before_secret_access() {
        let read = Cell::new(false);
        assert!(build_openrouter_credits_request(
            "k",
            &endpoint("https://openrouter.ai.evil.test"),
            || {
                read.set(true);
                "secret".into()
            }
        )
        .is_err());
        assert!(!read.get());
    }

    #[test]
    fn parses_key_limit_usage_and_remaining() {
        let parsed = parse_openrouter_credits(
            &json!({"data":{"limit":10.5,"usage":3,"limit_remaining":7.5}}),
        )
        .unwrap();
        assert_eq!(parsed.balances[0].total.as_deref(), Some("10.5"));
        assert_eq!(parsed.balances[0].used.as_deref(), Some("3"));
        assert_eq!(parsed.balances[0].available.as_deref(), Some("7.5"));
    }

    #[test]
    fn preserves_decimal_precision_beyond_f64() {
        let parsed = parse_openrouter_credits(&json!({"data":{
            "limit":"9007199254740993.0001",
            "usage":"9007199254740992.9999"
        }}))
        .unwrap();
        assert_eq!(
            parsed.balances[0].total.as_deref(),
            Some("9007199254740993.0001")
        );
        assert_eq!(parsed.balances[0].available, None);
    }

    #[test]
    fn accepts_key_without_a_spending_limit() {
        let parsed = parse_openrouter_credits(&json!({"data":{
            "limit": null,
            "limit_remaining": null,
            "usage": 45.232836758
        }}))
        .unwrap();
        assert_eq!(parsed.extensions["key_limit_unlimited"], json!(true));
        assert_eq!(parsed.balances[0].available, None);
        assert_eq!(parsed.balances[0].used.as_deref(), Some("45.232836758"));
    }

    #[test]
    fn preserves_official_zero_remaining_for_exhausted_limited_key() {
        let parsed = parse_openrouter_credits(&json!({"data":{
            "limit": 20,
            "limit_remaining": 0,
            "usage": 20.185116646,
            "is_free_tier": false
        }}))
        .unwrap();
        assert_eq!(parsed.extensions.get("unlimited"), None);
        assert_eq!(parsed.balances[0].total.as_deref(), Some("20"));
        assert_eq!(parsed.balances[0].available.as_deref(), Some("0"));
        assert_eq!(parsed.balances[0].used.as_deref(), Some("20.185116646"));
    }

    #[test]
    fn tolerates_partial_or_empty_quota_fields_without_rendering_fake_values() {
        let partial =
            parse_openrouter_credits(&json!({"data":{"limit_remaining":5,"usage":"not-a-number"}}))
                .unwrap();
        assert_eq!(partial.balances[0].available.as_deref(), Some("5"));
        assert_eq!(partial.balances[0].total, None);
        assert_eq!(partial.balances[0].used, None);

        assert!(parse_openrouter_credits(&json!({"data":{"label":"key"}})).is_err());
    }

    #[test]
    fn parses_free_model_daily_requests_as_independent_source() {
        let parsed = parse_openrouter_credits(&json!({"data":{
            "limit": 10,
            "usage": 3,
            "limit_remaining": 7.5,
            "free_model_daily_requests": {"limit": 50, "used": 12, "remaining": 38}
        }}))
        .unwrap();
        assert_eq!(parsed.sources.len(), 2);
        assert_eq!(parsed.sources[1].id, "free_requests");
        assert_eq!(parsed.sources[1].product, "model_requests");
        assert_eq!(parsed.sources[1].scope, "model");
        assert_eq!(parsed.sources[1].region.as_deref(), Some("global"));
        let window = &parsed.windows[0];
        assert_eq!(window.source_id.as_deref(), Some("free_requests"));
        assert_eq!(window.code, "free_model_daily_requests");
        assert_eq!(window.scope, "model");
        assert_eq!(window.unit, "requests");
        let ProviderQuotaValue::Decimal(limit) = window.limit_value.as_ref().unwrap() else {
            panic!("free window limit should stay a decimal string");
        };
        assert_eq!(limit, "50");
        assert!(window.used_value.is_some());
        assert_eq!(window.reset_at, None);
        assert!(!window.is_exhausted);
        // Key 消费限额金额逻辑不受免费来源影响。
        assert_eq!(parsed.balances.len(), 1);
        assert_eq!(parsed.balances[0].source_id.as_deref(), Some("key_limit"));
        assert_eq!(parsed.balances[0].available.as_deref(), Some("7.5"));
    }

    #[test]
    fn free_model_daily_requests_zero_remaining_marks_exhausted_window() {
        for remaining in [json!(0), json!("0"), json!("00")] {
            let parsed = parse_openrouter_credits(&json!({"data":{
                "limit_remaining": 5,
                "free_model_daily_requests": {"limit": 50, "used": 50, "remaining": remaining}
            }}))
            .unwrap();
            assert!(parsed.windows[0].is_exhausted, "remaining {remaining}");
        }

        // 巨大整数计数不经浮点换算，非零不得误判耗尽。
        let huge = parse_openrouter_credits(&json!({"data":{
            "limit_remaining": 5,
            "free_model_daily_requests": {"limit": "9007199254740993", "remaining": "9007199254740993"}
        }}))
        .unwrap();
        assert!(!huge.windows[0].is_exhausted);
    }

    #[test]
    fn missing_or_invalid_free_request_fields_stay_unknown() {
        let absent = parse_openrouter_credits(&json!({"data":{"limit_remaining": 5}})).unwrap();
        assert!(absent.windows.is_empty());
        let invalid = parse_openrouter_credits(&json!({"data":{
            "limit_remaining": 5,
            "free_model_daily_requests": {"limit": "many", "remaining": null}
        }}))
        .unwrap();
        assert!(invalid.windows.is_empty());
        assert_eq!(invalid.sources.len(), 1);

        // 负数、小数与科学计数法小数都不是合法请求计数，全部保持未知。
        let non_integer = parse_openrouter_credits(&json!({"data":{
            "limit_remaining": 5,
            "free_model_daily_requests": {"limit": -1, "used": 1.5, "remaining": "1e-400"}
        }}))
        .unwrap();
        assert!(non_integer.windows.is_empty());
        assert_eq!(non_integer.sources.len(), 1);

        // 仅部分字段非法时，合法字段保留原值，非法字段不参与零值耗尽判断。
        let partial = parse_openrouter_credits(&json!({"data":{
            "limit_remaining": 5,
            "free_model_daily_requests": {"limit": 50, "remaining": "0.5"}
        }}))
        .unwrap();
        assert_eq!(partial.windows.len(), 1);
        assert!(partial.windows[0].remaining_value.is_none());
        assert!(!partial.windows[0].is_exhausted);
    }
}
