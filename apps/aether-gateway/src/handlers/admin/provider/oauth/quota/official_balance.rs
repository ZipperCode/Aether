mod domain;
mod execution;
mod persistence;
mod response;
mod routing;

use self::domain::{FlightScope, OfficialQuotaItem, StableErrorClass};
use self::execution::{execute_prepared, prepare_attempt, PrepareInput, PreparedAttempt};
use self::persistence::{latest_refresh_state, persist_attempt, SnapshotUpdate};
use self::response::{backoff_item, management_response, persisted_item, rejected_item};
use self::routing::{singleflight_identity, validate_selected_endpoint};
use super::dispatch::QuotaRefreshSource;
use crate::{handlers::admin::request::AdminAppState, GatewayError};
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{AsyncSingleflight, OfficialProviderBackgroundLimiter};
use serde_json::Value;
use std::{
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

static IN_FLIGHT: LazyLock<AsyncSingleflight<String, OfficialQuotaItem>> =
    LazyLock::new(Default::default);

static BACKGROUND_LIMITER: LazyLock<OfficialProviderBackgroundLimiter> =
    LazyLock::new(|| OfficialProviderBackgroundLimiter::new(3));

pub(crate) async fn refresh_official_balance_provider_quota_locally(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    keys: Vec<StoredProviderCatalogKey>,
    proxy_override: Option<ProxySnapshot>,
    source: QuotaRefreshSource,
) -> Result<Option<Value>, GatewayError> {
    let _background_permit = if source.is_background() {
        Some(
            BACKGROUND_LIMITER
                .acquire(&provider.id)
                .await
                .map_err(|error| GatewayError::Internal(error.to_string()))?,
        )
    } else {
        None
    };
    let provider_type = provider.provider_type.trim().to_ascii_lowercase();
    if let Err(class) = validate_selected_endpoint(&provider.id, &provider_type, endpoint) {
        let observed_at = now_unix_secs();
        let items = keys
            .iter()
            .map(|key| rejected_item(key, class, observed_at))
            .collect();
        return Ok(Some(management_response(items)));
    }

    let mut items = Vec::with_capacity(keys.len());
    for key in keys {
        let observed_at = now_unix_secs();
        if persisted_backoff_applies(&key, source, observed_at) {
            items.push(backoff_item(&key));
            continue;
        }
        let prepared = prepare_attempt(PrepareInput {
            state,
            provider,
            endpoint,
            key: &key,
            proxy_override: proxy_override.clone(),
        })
        .await;
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(failure) => {
                items.push(rejected_item(&key, failure.class, observed_at));
                continue;
            }
        };
        let flight = singleflight_identity(
            FlightScope {
                provider_id: &provider.id,
                key_id: &key.id,
                endpoint_id: &endpoint.id,
            },
            &prepared.route,
        );
        let item = IN_FLIGHT
            .run(flight, || {
                refresh_and_persist_one(state, &key, &provider_type, prepared, observed_at)
            })
            .await;
        items.push(item);
    }
    Ok(Some(management_response(items)))
}

async fn refresh_and_persist_one(
    state: &AdminAppState<'_>,
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    prepared: PreparedAttempt,
    observed_at: u64,
) -> OfficialQuotaItem {
    let attempt = execute_prepared(state, prepared).await;
    match persist_attempt(
        state,
        SnapshotUpdate {
            key,
            provider_type,
            attempt: &attempt,
            now_unix_secs: observed_at,
        },
    )
    .await
    {
        Ok(persisted) => persisted_item(key, &attempt, persisted),
        Err(_) => rejected_item(key, StableErrorClass::PersistenceFailed, observed_at),
    }
}

fn persisted_backoff_applies(
    key: &StoredProviderCatalogKey,
    source: QuotaRefreshSource,
    now_unix_secs: u64,
) -> bool {
    !source.bypasses_persisted_backoff()
        && latest_refresh_state(key)
            .next_eligible_at
            .is_some_and(|eligible_at| eligible_at > now_unix_secs)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "official_balance/tests/mod.rs"]
mod wave2_tests;
