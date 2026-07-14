use super::super::shared::resolve_provider_quota_execution_timeouts;
use super::domain::{ExecutionRoute, FlightScope, RouteSource, StableErrorClass};
use aether_contracts::{ExecutionTimeouts, ProxySnapshot};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogEndpoint;
use aether_provider_pool::{
    clamp_official_balance_execution_timeouts, is_official_api_key_quota_endpoint,
    is_official_deepseek_endpoint, is_official_openrouter_endpoint,
    OFFICIAL_BALANCE_MAX_BACKOFF_SECS, OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, future::Future, time::UNIX_EPOCH};

pub(super) fn validate_selected_endpoint(
    provider_id: &str,
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> Result<(), StableErrorClass> {
    if endpoint.provider_id != provider_id {
        return Err(StableErrorClass::EndpointForeign);
    }
    if !endpoint.is_active {
        return Err(StableErrorClass::EndpointInactive);
    }
    let official = match provider_type.trim().to_ascii_lowercase().as_str() {
        "deepseek" => is_official_deepseek_endpoint(endpoint),
        "openrouter" => is_official_openrouter_endpoint(endpoint),
        provider @ ("moonshot" | "kimi_coding" | "siliconflow" | "zhipu" | "zai") => {
            is_official_api_key_quota_endpoint(provider, endpoint)
        }
        _ => false,
    };
    if official {
        Ok(())
    } else {
        Err(StableErrorClass::EndpointUnofficial)
    }
}

pub(super) async fn resolve_execution_route<F, Fut>(
    proxy_override: Option<ProxySnapshot>,
    configured: F,
) -> ExecutionRoute
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = (Option<ProxySnapshot>, Option<&'static str>)>,
{
    match proxy_override {
        Some(proxy) => ExecutionRoute {
            proxy: Some(proxy),
            source: RouteSource::ExplicitOverride,
        },
        None => {
            let (proxy, source) = configured().await;
            ExecutionRoute {
                proxy,
                source: RouteSource::configured(source),
            }
        }
    }
}

pub(super) fn singleflight_identity(scope: FlightScope<'_>, route: &ExecutionRoute) -> String {
    let mut hasher = Sha256::new();
    let hash_field = |hasher: &mut Sha256, tag: &[u8], value: &[u8]| {
        hasher.update((tag.len() as u64).to_be_bytes());
        hasher.update(tag);
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    };
    hash_field(&mut hasher, b"provider_id", scope.provider_id.as_bytes());
    hash_field(&mut hasher, b"key_id", scope.key_id.as_bytes());
    hash_field(&mut hasher, b"endpoint_id", scope.endpoint_id.as_bytes());
    hash_field(
        &mut hasher,
        b"route_source",
        route.source.identity().as_bytes(),
    );
    match serde_json::to_vec(&route.proxy) {
        Ok(snapshot) => hash_field(&mut hasher, b"route_snapshot_json", &snapshot),
        Err(_) => {
            let snapshot = format!("{:?}", route.proxy);
            hash_field(
                &mut hasher,
                b"route_snapshot_serialization_failure",
                snapshot.as_bytes(),
            );
        }
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn retry_after_eligibility(
    headers: &BTreeMap<String, String>,
    current: u64,
) -> Option<u64> {
    let raw = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))?
        .1
        .trim();
    let minimum = current.saturating_add(OFFICIAL_BALANCE_MIN_BACKOFF_SECS);
    let maximum = current.saturating_add(OFFICIAL_BALANCE_MAX_BACKOFF_SECS);
    if let Ok(delta) = raw.parse::<u64>() {
        return Some(current.saturating_add(delta.clamp(
            OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
            OFFICIAL_BALANCE_MAX_BACKOFF_SECS,
        )));
    }
    let absolute = httpdate::parse_http_date(raw)
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(absolute.clamp(minimum, maximum))
}

pub(super) fn official_balance_execution_timeouts(
    configured: Option<ExecutionTimeouts>,
    proxy: Option<&ProxySnapshot>,
) -> ExecutionTimeouts {
    let proxy_active = proxy.is_some_and(|snapshot| snapshot.enabled != Some(false));
    let resolved =
        resolve_provider_quota_execution_timeouts(configured, proxy.filter(|_| proxy_active));
    clamp_official_balance_execution_timeouts(resolved, proxy_active)
}
