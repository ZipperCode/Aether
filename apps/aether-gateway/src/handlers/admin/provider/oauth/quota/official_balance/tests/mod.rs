use super::{
    domain::{
        AttemptResult, ExecutionRoute, FlightScope, ItemStatus, OfficialQuotaItem, QuotaKind,
        RouteSource, StableErrorClass,
    },
    execution::{
        apply_zhipu_plan_scope, apply_zhipu_token_plan_fallback_policy,
        execution_result_to_attempt, should_fallback_to_zhipu_balance,
        should_retry_zhipu_team_quota,
    },
    persisted_backoff_applies,
    persistence::{
        build_persisted_snapshot, quota_cache_invalidation_scope, QuotaCacheInvalidationScope,
        SnapshotUpdate,
    },
    response::{backoff_item, management_response, persisted_item},
    routing::{
        official_balance_execution_timeouts, resolve_execution_route, retry_after_eligibility,
        singleflight_identity, validate_selected_endpoint,
    },
    QuotaRefreshSource,
};
use aether_contracts::{ExecutionResult, ProxySnapshot, ResponseBody};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
};
use aether_provider_pool::{
    ProviderQuotaRefreshState, ProviderQuotaSnapshotContract, ProviderQuotaSnapshotKind,
    ProviderQuotaValue, ProviderQuotaWindow,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

mod manual_http;
mod response_persistence;
mod routing_security;

struct EndpointFixture<'a> {
    id: &'a str,
    provider_id: &'a str,
    base_url: &'a str,
    active: bool,
}

fn endpoint(fixture: EndpointFixture<'_>) -> StoredProviderCatalogEndpoint {
    StoredProviderCatalogEndpoint::new(
        fixture.id.into(),
        fixture.provider_id.into(),
        "openai:chat".into(),
        None,
        None,
        fixture.active,
    )
    .expect("endpoint fixture")
    .with_transport_fields(
        fixture.base_url.into(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("endpoint transport fixture")
}

fn key(id: &str, name: &str, quota: Option<Value>) -> StoredProviderCatalogKey {
    let mut key = StoredProviderCatalogKey::new(
        id.into(),
        "provider-1".into(),
        name.into(),
        "api_key".into(),
        None,
        true,
    )
    .expect("key fixture");
    key.status_snapshot = quota.map(|quota| json!({"quota": quota}));
    key
}

fn refresh_state(error: Option<&str>) -> ProviderQuotaRefreshState {
    ProviderQuotaRefreshState {
        last_attempt_at: Some(100),
        last_success_at: Some(90),
        error: error.map(ToOwned::to_owned),
        next_eligible_at: error.map(|_| 160),
        failure_count: Some(u32::from(error.is_some())),
    }
}

fn item(key_id: &str, status: ItemStatus, snapshot: Option<Value>) -> OfficialQuotaItem {
    OfficialQuotaItem {
        key_id: key_id.into(),
        key_name: format!("name-{key_id}"),
        status,
        status_code: None,
        error_class: None,
        message: None,
        quota_snapshot: snapshot,
        refresh_state: refresh_state(None),
    }
}
