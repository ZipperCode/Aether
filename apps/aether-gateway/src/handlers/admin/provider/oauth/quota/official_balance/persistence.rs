use super::domain::{AttemptResult, PersistedSnapshot, QuotaKind, StableErrorClass};
use super::routing::retry_after_eligibility;
use crate::{handlers::admin::request::AdminAppState, GatewayError};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_provider_pool::{
    official_balance_backoff_with_jitter_secs, ProviderQuotaRefreshState,
    ProviderQuotaSnapshotContract, ProviderQuotaSnapshotKind,
    PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION, ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD,
};
use serde_json::{json, Value};

pub(super) struct SnapshotUpdate<'a> {
    pub(super) key: &'a StoredProviderCatalogKey,
    pub(super) provider_type: &'a str,
    pub(super) attempt: &'a AttemptResult,
    pub(super) now_unix_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QuotaCacheInvalidationScope {
    CatalogOnly,
    CandidateRouting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubscriptionRoutingState {
    Unknown,
    Active,
    Exhausted,
}

impl SubscriptionRoutingState {
    const fn from_exhausted(exhausted: bool) -> Self {
        if exhausted {
            Self::Exhausted
        } else {
            Self::Active
        }
    }
}

pub(super) async fn persist_attempt(
    state: &AdminAppState<'_>,
    update: SnapshotUpdate<'_>,
) -> Result<PersistedSnapshot, GatewayError> {
    let key_id = update.key.id.clone();
    let now_unix_secs = update.now_unix_secs;
    let invalidation_scope = quota_cache_invalidation_scope(&update);
    let persisted = build_persisted_snapshot(update)
        .map_err(|class| GatewayError::Internal(class.persisted_error()))?;
    let updated = state
        .mutate_provider_catalog_key_quota_snapshot(
            &key_id,
            &persisted.snapshot,
            Some(now_unix_secs),
        )
        .await?;
    if !updated {
        return Err(GatewayError::Internal(
            StableErrorClass::PersistenceFailed.persisted_error(),
        ));
    }
    if invalidation_scope == QuotaCacheInvalidationScope::CandidateRouting {
        state.app().invalidate_provider_quota_candidate_caches();
    }
    Ok(persisted)
}

pub(super) fn quota_cache_invalidation_scope(
    update: &SnapshotUpdate<'_>,
) -> QuotaCacheInvalidationScope {
    let Some(quota_kind) = update.attempt.quota_kind() else {
        return QuotaCacheInvalidationScope::CatalogOnly;
    };
    match quota_kind {
        QuotaKind::Balance => {
            let previous = subscription_routing_state(typed_snapshot(update.key).as_ref());
            let next = subscription_routing_state_for_attempt(update.attempt, previous);
            if next == SubscriptionRoutingState::Exhausted && previous != next {
                QuotaCacheInvalidationScope::CandidateRouting
            } else {
                QuotaCacheInvalidationScope::CatalogOnly
            }
        }
        QuotaKind::Subscription => {
            let previous = subscription_routing_state(typed_snapshot(update.key).as_ref());
            let next = subscription_routing_state_for_attempt(update.attempt, previous);
            if previous == next {
                QuotaCacheInvalidationScope::CatalogOnly
            } else {
                QuotaCacheInvalidationScope::CandidateRouting
            }
        }
    }
}

fn subscription_routing_state(
    snapshot: Option<&ProviderQuotaSnapshotContract>,
) -> SubscriptionRoutingState {
    if snapshot.is_some_and(|snapshot| {
        snapshot.provider_type.eq_ignore_ascii_case("zhipu")
            && snapshot.exhausted
            && snapshot
                .extensions
                .get(ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD)
                .and_then(Value::as_bool)
                == Some(true)
    }) {
        return SubscriptionRoutingState::Exhausted;
    }
    match snapshot.map(|snapshot| (snapshot.kind, snapshot.exhausted)) {
        Some((ProviderQuotaSnapshotKind::Subscription, exhausted)) => {
            SubscriptionRoutingState::from_exhausted(exhausted)
        }
        Some((ProviderQuotaSnapshotKind::Balance, _)) | None => SubscriptionRoutingState::Unknown,
    }
}

fn subscription_routing_state_for_attempt(
    attempt: &AttemptResult,
    previous: SubscriptionRoutingState,
) -> SubscriptionRoutingState {
    match attempt {
        AttemptResult::Success { snapshot, .. } => subscription_routing_state(Some(snapshot)),
        AttemptResult::HttpFailure { .. }
        | AttemptResult::ParseFailure { .. }
        | AttemptResult::BusinessFailure { .. }
        | AttemptResult::TransportFailure { .. } => match previous {
            SubscriptionRoutingState::Unknown => SubscriptionRoutingState::Active,
            SubscriptionRoutingState::Active => SubscriptionRoutingState::Active,
            SubscriptionRoutingState::Exhausted => SubscriptionRoutingState::Exhausted,
        },
    }
}

pub(super) fn build_persisted_snapshot(
    update: SnapshotUpdate<'_>,
) -> Result<PersistedSnapshot, StableErrorClass> {
    let refresh_state = refresh_state_for_attempt(update.key, update.attempt, update.now_unix_secs);
    let (mut snapshot, code, freshness) = match update.attempt {
        AttemptResult::Success { snapshot, .. } => (snapshot.clone(), "ok", "fresh"),
        AttemptResult::HttpFailure { class, .. }
        | AttemptResult::ParseFailure { class, .. }
        | AttemptResult::BusinessFailure { class, .. }
        | AttemptResult::TransportFailure { class, .. } => {
            let quota_kind = update
                .attempt
                .quota_kind()
                .ok_or(StableErrorClass::RequestInvalid)?;
            let retained = typed_snapshot(update.key)
                .filter(|snapshot| snapshot.kind == quota_kind.snapshot_kind())
                .unwrap_or_else(|| {
                    quota_kind.empty_snapshot(update.provider_type, update.now_unix_secs)
                });
            (retained, class.code(), "stale")
        }
    };
    snapshot.schema_version = PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION;
    snapshot.provider_type = update.provider_type.to_owned();
    snapshot.refresh_state = refresh_state.clone();
    snapshot.extensions.insert("code".into(), json!(code));
    snapshot
        .extensions
        .insert("freshness".into(), json!(freshness));
    snapshot
        .extensions
        .entry("observed_at")
        .or_insert_with(|| json!(update.now_unix_secs));
    snapshot
        .extensions
        .insert("updated_at".into(), json!(update.now_unix_secs));
    let snapshot =
        serde_json::to_value(snapshot).map_err(|_| StableErrorClass::PersistenceFailed)?;
    Ok(PersistedSnapshot {
        snapshot,
        refresh_state,
    })
}

pub(super) fn latest_snapshot(key: &StoredProviderCatalogKey) -> Option<Value> {
    key.status_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("quota"))
        .cloned()
        .filter(Value::is_object)
}

pub(super) fn latest_refresh_state(key: &StoredProviderCatalogKey) -> ProviderQuotaRefreshState {
    typed_snapshot(key)
        .map(|snapshot| snapshot.refresh_state)
        .unwrap_or_default()
}

pub(super) fn failure_refresh_state(
    key: &StoredProviderCatalogKey,
    class: StableErrorClass,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    build_failure_refresh_state(key, class, None, None, now_unix_secs)
}

fn refresh_state_for_attempt(
    key: &StoredProviderCatalogKey,
    attempt: &AttemptResult,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    match attempt {
        AttemptResult::Success { .. } => ProviderQuotaRefreshState {
            last_attempt_at: Some(now_unix_secs),
            last_success_at: Some(now_unix_secs),
            error: None,
            next_eligible_at: None,
            failure_count: Some(0),
        },
        AttemptResult::HttpFailure { headers, class, .. } => build_failure_refresh_state(
            key,
            *class,
            None,
            retry_after_eligibility(headers, now_unix_secs),
            now_unix_secs,
        ),
        AttemptResult::BusinessFailure { class, detail, .. } => {
            build_failure_refresh_state(key, *class, Some(detail.as_str()), None, now_unix_secs)
        }
        AttemptResult::ParseFailure { class, .. }
        | AttemptResult::TransportFailure { class, .. } => {
            build_failure_refresh_state(key, *class, None, None, now_unix_secs)
        }
    }
}

fn build_failure_refresh_state(
    key: &StoredProviderCatalogKey,
    class: StableErrorClass,
    detail: Option<&str>,
    retry_at: Option<u64>,
    now_unix_secs: u64,
) -> ProviderQuotaRefreshState {
    let previous = latest_refresh_state(key);
    let failure_count = previous.failure_count.unwrap_or(0).saturating_add(1);
    let seed = key
        .id
        .bytes()
        .fold(u64::from(failure_count), |value, byte| {
            value
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(byte))
        });
    ProviderQuotaRefreshState {
        last_attempt_at: Some(now_unix_secs),
        last_success_at: previous.last_success_at,
        error: Some(match detail {
            Some(detail) => format!("{}: {detail}", class.code()),
            None => class.persisted_error(),
        }),
        next_eligible_at: Some(retry_at.unwrap_or_else(|| {
            now_unix_secs.saturating_add(official_balance_backoff_with_jitter_secs(
                failure_count,
                seed,
            ))
        })),
        failure_count: Some(failure_count),
    }
}

fn typed_snapshot(key: &StoredProviderCatalogKey) -> Option<ProviderQuotaSnapshotContract> {
    latest_snapshot(key)
        .and_then(|value| serde_json::from_value::<ProviderQuotaSnapshotContract>(value).ok())
}
