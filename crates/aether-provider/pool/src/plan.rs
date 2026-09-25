use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
pub use aether_provider_transport::normalize_provider_plan_tier;
use aether_provider_transport::{provider_plan_tier_from_fields, provider_plan_tier_from_metadata};
use serde_json::{Map, Value};

pub fn derive_plan_tier(
    provider_type: &str,
    key: &StoredProviderCatalogKey,
    auth_config: Option<&Map<String, Value>>,
) -> Option<String> {
    let has_auth_config = auth_config.is_some()
        || key
            .encrypted_auth_config
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    if !provider_pool_auth_managed(key, provider_type, has_auth_config) {
        return None;
    }

    if let Some(quota_snapshot) = key
        .status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("quota"))
        .and_then(Value::as_object)
    {
        if let Some(normalized) = provider_plan_tier_from_fields(quota_snapshot, provider_type) {
            return Some(normalized);
        }
    }

    // 保持额度快照优先，其余来源与模型发现和传输能力判断共用纯投影。
    provider_plan_tier_from_metadata(provider_type, key.upstream_metadata.as_ref(), auth_config)
}

pub fn derive_oauth_plan_type(
    provider_type: &str,
    key: &StoredProviderCatalogKey,
    auth_config: Option<&Map<String, Value>>,
) -> Option<String> {
    derive_plan_tier(provider_type, key, auth_config)
}

fn provider_pool_auth_managed(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    has_auth_config: bool,
) -> bool {
    key.auth_type.trim().eq_ignore_ascii_case("oauth")
        || (provider_type.trim().eq_ignore_ascii_case("kiro")
            && key.auth_type.trim().eq_ignore_ascii_case("bearer")
            && has_auth_config)
}
