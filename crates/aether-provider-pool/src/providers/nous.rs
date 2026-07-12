use crate::capability::ProviderPoolCapabilities;
use crate::provider::{provider_pool_matching_endpoint, ProviderPoolAdapter};
use crate::quota_refresh::ProviderPoolQuotaRequestSpec;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use std::collections::BTreeMap;

pub const NOUS_PORTAL_BASE_URL: &str = "https://portal.nousresearch.com";
pub const NOUS_ACCOUNT_PATH: &str = "/api/oauth/account";
pub const NOUS_BILLING_PATH: &str = "/api/billing/state";

#[derive(Debug, Clone, Default)]
pub struct NousProviderPoolAdapter;
impl ProviderPoolAdapter for NousProviderPoolAdapter {
    fn provider_type(&self) -> &'static str {
        "nous"
    }
    fn capabilities(&self) -> ProviderPoolCapabilities {
        ProviderPoolCapabilities {
            plan_tier: true,
            quota_reset: true,
            quota_refresh: true,
        }
    }
    fn quota_refresh_endpoint(
        &self,
        endpoints: &[StoredProviderCatalogEndpoint],
        include_inactive: bool,
    ) -> Option<StoredProviderCatalogEndpoint> {
        provider_pool_matching_endpoint(endpoints, include_inactive, |_| true)
    }
    fn quota_refresh_missing_endpoint_message(&self) -> String {
        "找不到有效的 Nous 端点".to_string()
    }
}
fn request(
    key_id: &str,
    authorization: (String, String),
    path: &str,
    kind: &str,
) -> ProviderPoolQuotaRequestSpec {
    let headers = BTreeMap::from([
        ("accept".to_string(), "application/json".to_string()),
        authorization,
    ]);
    ProviderPoolQuotaRequestSpec {
        request_id: format!("nous-{kind}-{key_id}"),
        provider_name: "nous".into(),
        quota_kind: kind.into(),
        method: "GET".into(),
        url: format!("{NOUS_PORTAL_BASE_URL}{path}"),
        headers,
        content_type: None,
        json_body: None,
        client_api_format: "openai:chat".into(),
        provider_api_format: "openai:chat".into(),
        model_name: None,
        accept_invalid_certs: false,
    }
}
pub fn build_nous_account_quota_request(
    key_id: &str,
    authorization: (String, String),
) -> ProviderPoolQuotaRequestSpec {
    request(key_id, authorization, NOUS_ACCOUNT_PATH, "account")
}
pub fn build_nous_billing_quota_request(
    key_id: &str,
    authorization: (String, String),
) -> ProviderPoolQuotaRequestSpec {
    request(key_id, authorization, NOUS_BILLING_PATH, "billing")
}
