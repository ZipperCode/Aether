use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use crate::capability::{ProviderPoolCapabilities, ProviderQuotaServingPolicy};
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use crate::quota_snapshot::{ProviderQuotaBalance, ProviderQuotaSnapshotContract};

use super::official_balance::{
    decimal_string, endpoint_has_official_origin, official_quota_source,
};

pub const OPENROUTER_CREDITS_URL: &str = "https://openrouter.ai/api/v1/key";
const OPENROUTER_HOST: &str = "openrouter.ai";
pub fn is_official_openrouter_endpoint(endpoint: &StoredProviderCatalogEndpoint) -> bool {
    endpoint_has_official_origin(endpoint, OPENROUTER_HOST)
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
}
