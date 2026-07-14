use std::collections::BTreeMap;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use serde_json::Value;

use crate::capability::{ProviderPoolCapabilities, ProviderQuotaServingPolicy};
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use crate::quota_snapshot::{ProviderQuotaBalance, ProviderQuotaSnapshotContract};

use super::official_balance::{decimal_string, endpoint_has_official_origin};

pub const DEEPSEEK_BALANCE_URL: &str = "https://api.deepseek.com/user/balance";
const DEEPSEEK_HOST: &str = "api.deepseek.com";
pub fn is_official_deepseek_endpoint(endpoint: &StoredProviderCatalogEndpoint) -> bool {
    endpoint_has_official_origin(endpoint, DEEPSEEK_HOST)
}

#[derive(Debug, Clone, Default)]
pub struct DeepSeekProviderPoolAdapter;

impl ProviderPoolAdapter for DeepSeekProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        "deepseek"
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
            endpoint_has_official_origin(endpoint, DEEPSEEK_HOST)
        })
    }
}

pub fn build_deepseek_balance_request<F>(
    key_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    read_secret: F,
) -> Result<ProviderPoolQuotaRequestSpec, &'static str>
where
    F: FnOnce() -> String,
{
    if !is_official_deepseek_endpoint(endpoint) {
        return Err("quota unsupported for non-official DeepSeek endpoint");
    }
    let secret = read_secret();
    Ok(ProviderPoolQuotaRequestSpec {
        request_id: format!("deepseek-balance-{key_id}"),
        provider_name: "deepseek".into(),
        quota_kind: "balance".into(),
        method: "GET".into(),
        url: DEEPSEEK_BALANCE_URL.into(),
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("authorization".into(), format!("Bearer {secret}")),
        ]),
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
        accept_invalid_certs: false,
    })
}

pub fn parse_deepseek_balance(
    value: &Value,
) -> Result<ProviderQuotaSnapshotContract, &'static str> {
    let records = value
        .get("balance_infos")
        .and_then(Value::as_array)
        .ok_or("missing balance_infos")?;
    let balances = records
        .iter()
        .filter_map(|record| {
            let unit = record.get("currency")?.as_str()?.trim();
            if unit.is_empty() {
                return None;
            }
            Some(ProviderQuotaBalance {
                unit: unit.to_string(),
                available: record.get("total_balance").and_then(decimal_string),
                total: record.get("total_balance").and_then(decimal_string),
                granted: record.get("granted_balance").and_then(decimal_string),
                topped_up: record.get("topped_up_balance").and_then(decimal_string),
                used: None,
            })
        })
        .collect::<Vec<_>>();
    if balances.is_empty() {
        return Err("no valid balance records");
    }
    let mut snapshot = ProviderQuotaSnapshotContract::balance("deepseek", balances);
    if let Some(available) = value.get("is_available").and_then(Value::as_bool) {
        snapshot
            .extensions
            .insert("is_available".into(), Value::Bool(available));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
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
            Some(json!([{"action":"set","key":"x-secret","value":"leak"}])),
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
        let spec =
            build_deepseek_balance_request("k", &endpoint("https://api.deepseek.com/v1"), || {
                "key".into()
            })
            .unwrap();
        assert_eq!(spec.url, DEEPSEEK_BALANCE_URL);
        assert_eq!(spec.headers.len(), 2);
        assert!(!spec.headers.contains_key("x-secret"));
    }

    #[test]
    fn rejects_hostile_origins_before_reading_secret() {
        for url in [
            "http://api.deepseek.com",
            "https://api.deepseek.com.evil.test",
            "https://user@api.deepseek.com",
            "https://api.deepseek.com:444",
            "https://127.0.0.1",
        ] {
            let read = Cell::new(false);
            assert!(build_deepseek_balance_request("k", &endpoint(url), || {
                read.set(true);
                "secret".into()
            })
            .is_err());
            assert!(!read.get(), "secret read for {url}");
        }
    }

    #[test]
    fn parses_all_currency_records() {
        let parsed = parse_deepseek_balance(&json!({"is_available":false,"balance_infos":[
            {"currency":"CNY","total_balance":"3.25","granted_balance":"1","topped_up_balance":"2.25"},
            {"currency":"USD","total_balance":"4","granted_balance":"4","topped_up_balance":"0"}
        ]})).unwrap();
        assert_eq!(parsed.balances.len(), 2);
        assert_eq!(parsed.balances[0].available.as_deref(), Some("3.25"));
        assert_eq!(parsed.extensions["is_available"], false);
    }
}
