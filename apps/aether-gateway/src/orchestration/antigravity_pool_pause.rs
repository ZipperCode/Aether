//! Provider-scoped, temporary Antigravity pool pause after a definitive TOS response.
//!
//! The durable OAuth account block isolates the offending credential. This module only
//! suppresses admission to the associated provider pool through the shared runtime KV,
//! so already-materialized candidates cannot continue an upstream failover sweep.

use std::time::Duration;

use aether_contracts::ExecutionPlan;
use aether_provider_transport::GatewayProviderTransportSnapshot;
use aether_runtime_state::RuntimeState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::clock::current_unix_secs;
use crate::{AppState, GatewayError};

const ANTIGRAVITY_POOL_PAUSE_KEY_PREFIX: &str = "aether:antigravity:tos-pool-pause:v1";
const ANTIGRAVITY_POOL_PAUSE_RECORD_VERSION: u8 = 1;
const ANTIGRAVITY_POOL_PAUSE_DEFAULT_SECONDS: u64 = 900;
const ANTIGRAVITY_POOL_PAUSE_MAX_SECONDS: u64 = 86_400;
const GOOGLE_ERROR_INFO_TYPE: &str = "type.googleapis.com/google.rpc.ErrorInfo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AntigravityPoolPauseSettings {
    enabled: bool,
    ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AntigravityPoolPauseRecord {
    version: u8,
    provider_id: String,
    deadline_unix_secs: u64,
}

/// Only the structured Google account TOS terminal qualifies. Message wording,
/// generic 403 responses, and reasons attached to non-ErrorInfo details do not.
pub(crate) fn antigravity_tos_violation_response(
    provider_type: &str,
    status_code: u16,
    response_text: Option<&str>,
) -> bool {
    if status_code != 403 || !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return false;
    }
    let Some(payload) = response_text
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
    else {
        return false;
    };
    let Some(error) = payload.get("error").and_then(Value::as_object) else {
        return false;
    };
    if error.get("status").and_then(Value::as_str) != Some("PERMISSION_DENIED") {
        return false;
    }
    error
        .get("details")
        .and_then(Value::as_array)
        .is_some_and(|details| {
            details.iter().any(|detail| {
                detail.get("@type").and_then(Value::as_str) == Some(GOOGLE_ERROR_INFO_TYPE)
                    && detail.get("reason").and_then(Value::as_str) == Some("TOS_VIOLATION")
            })
        })
}

/// Read the provider-only switch defensively: malformed/raw config remains off,
/// and malformed TTLs use the documented default instead of creating a longer pause.
fn antigravity_pool_pause_settings(config: Option<&Value>) -> AntigravityPoolPauseSettings {
    let settings = config
        .and_then(Value::as_object)
        .and_then(|config| config.get("pool_advanced"))
        .and_then(Value::as_object)
        .and_then(|pool| pool.get("antigravity"))
        .and_then(Value::as_object);
    AntigravityPoolPauseSettings {
        enabled: settings
            .and_then(|settings| settings.get("tos_pause_enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ttl_seconds: settings
            .and_then(|settings| settings.get("tos_pause_seconds"))
            .and_then(Value::as_u64)
            .filter(|seconds| (1..=ANTIGRAVITY_POOL_PAUSE_MAX_SECONDS).contains(seconds))
            .unwrap_or(ANTIGRAVITY_POOL_PAUSE_DEFAULT_SECONDS),
    }
}

fn antigravity_pool_pause_key(provider_id: &str) -> String {
    let digest = Sha256::digest(provider_id.trim().as_bytes());
    let mut opaque = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(opaque, "{byte:02x}");
    }
    format!("{ANTIGRAVITY_POOL_PAUSE_KEY_PREFIX}:{opaque}")
}

/// Clears the provider marker when the feature is disabled or administratively changed.
/// This must also be called by the provider config update path when the switch is off;
/// otherwise disabling and re-enabling without intervening traffic could revive a marker.
pub(crate) async fn clear_antigravity_pool_pause(
    state: &AppState,
    provider_id: &str,
) -> Result<(), GatewayError> {
    state
        .runtime_kv_del(&antigravity_pool_pause_key(provider_id))
        .await
        .map(|_| ())
}

async fn record_antigravity_pool_pause_response(
    state: &AppState,
    transport: &GatewayProviderTransportSnapshot,
    status_code: u16,
    response_text: Option<&str>,
) -> Result<bool, GatewayError> {
    if !antigravity_tos_violation_response(
        transport.provider.provider_type.as_str(),
        status_code,
        response_text,
    ) {
        return Ok(false);
    }
    let settings = antigravity_pool_pause_settings(transport.provider.config.as_ref());
    if !settings.enabled {
        clear_antigravity_pool_pause(state, &transport.provider.id).await?;
        return Ok(false);
    }

    let now_unix_secs = current_unix_secs();
    let record = AntigravityPoolPauseRecord {
        version: ANTIGRAVITY_POOL_PAUSE_RECORD_VERSION,
        provider_id: transport.provider.id.clone(),
        deadline_unix_secs: now_unix_secs.saturating_add(settings.ttl_seconds),
    };
    let value =
        serde_json::to_string(&record).map_err(|err| GatewayError::Internal(err.to_string()))?;
    state
        .runtime_state
        .kv_set(
            &antigravity_pool_pause_key(&record.provider_id),
            value,
            Some(Duration::from_secs(settings.ttl_seconds)),
        )
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?;
    info!(
        event_name = "antigravity_tos_pool_paused",
        log_type = "event",
        provider_id = %record.provider_id,
        ttl_seconds = settings.ttl_seconds,
        deadline_unix_secs = record.deadline_unix_secs,
        "gateway paused Antigravity provider admission after a structured TOS violation"
    );
    Ok(true)
}

async fn antigravity_pool_pause_blocks_provider(
    runtime: &RuntimeState,
    provider_id: &str,
    provider_type: &str,
    config: Option<&Value>,
    now_unix_secs: u64,
) -> Result<bool, GatewayError> {
    if !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return Ok(false);
    }
    let settings = antigravity_pool_pause_settings(config);
    let key = antigravity_pool_pause_key(provider_id);
    if !settings.enabled {
        // 禁用不仅绕过，还清除旧标记，避免重新启用后恢复历史暂停。
        runtime
            .kv_delete(&key)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))?;
        return Ok(false);
    }
    let value = runtime
        .kv_get(&key)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?;
    let Some(value) = value else {
        return Ok(false);
    };
    let blocks = serde_json::from_str::<AntigravityPoolPauseRecord>(&value)
        .ok()
        .is_some_and(|record| {
            record.version == ANTIGRAVITY_POOL_PAUSE_RECORD_VERSION
                && record.provider_id == provider_id
                && record.deadline_unix_secs > now_unix_secs
        });
    if !blocks {
        runtime
            .kv_delete(&key)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))?;
    }
    Ok(blocks)
}

/// Runtime admission guard for a materialized execution plan. Backend/read failures
/// are returned to the caller, which must fail open rather than inventing a TOS pause.
pub(crate) async fn antigravity_pool_pause_blocks_plan(
    state: &AppState,
    plan: &ExecutionPlan,
) -> Result<bool, GatewayError> {
    let Some(transport) = state
        .read_provider_transport_snapshot(&plan.provider_id, &plan.endpoint_id, &plan.key_id)
        .await?
    else {
        return Ok(false);
    };
    antigravity_pool_pause_blocks_provider(
        state.runtime_state.as_ref(),
        &transport.provider.id,
        &transport.provider.provider_type,
        transport.provider.config.as_ref(),
        current_unix_secs(),
    )
    .await
}

/// Stops the triggering request itself, before another already-materialized plan runs.
pub(crate) async fn antigravity_pool_pause_stops_failover(
    state: &AppState,
    plan: &ExecutionPlan,
    status_code: u16,
    response_text: Option<&str>,
) -> bool {
    if status_code != 403 {
        return false;
    }
    let transport = match state
        .read_provider_transport_snapshot(&plan.provider_id, &plan.endpoint_id, &plan.key_id)
        .await
    {
        Ok(Some(transport)) => transport,
        Ok(None) => return false,
        Err(err) => {
            warn!(
                error = ?err,
                provider_id = %plan.provider_id,
                "gateway could not read Antigravity pool pause policy"
            );
            return false;
        }
    };
    if !antigravity_tos_violation_response(
        transport.provider.provider_type.as_str(),
        status_code,
        response_text,
    ) {
        return false;
    }
    match record_antigravity_pool_pause_response(state, &transport, status_code, response_text)
        .await
    {
        Ok(paused) => paused,
        Err(err) => {
            warn!(error = ?err, provider_id = %plan.provider_id, "failed to persist Antigravity pool pause");
            // 已确认违规且开启止损时，即使共享存储失败也不继续当前请求扫号。
            antigravity_pool_pause_settings(transport.provider.config.as_ref()).enabled
        }
    }
}

#[cfg(test)]
mod tests {
    use aether_runtime_state::{MemoryRuntimeStateConfig, RuntimeState};
    use serde_json::{json, Value};

    use super::{
        antigravity_pool_pause_blocks_provider, antigravity_pool_pause_key,
        antigravity_pool_pause_settings, antigravity_tos_violation_response,
        AntigravityPoolPauseRecord, ANTIGRAVITY_POOL_PAUSE_RECORD_VERSION,
    };

    const AUTH_PROVIDER: &str = "antigravity";

    fn tos_body() -> String {
        json!({
            "error": {
                "code": 403,
                "status": "PERMISSION_DENIED",
                "message": "Account access is restricted",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "TOS_VIOLATION",
                    "domain": "googleapis.com"
                }]
            }
        })
        .to_string()
    }

    fn provider(
        id: &str,
        provider_type: &str,
        enabled: Value,
    ) -> aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider {
        let mut provider = aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider::new(
            id.to_string(), id.to_string(), None, provider_type.to_string(),
        ).expect("provider should build");
        provider.config = Some(json!({
            "pool_advanced": {"antigravity": {
                "tos_pause_enabled": enabled, "tos_pause_seconds": 900
            }}
        }));
        provider
    }

    fn runtime() -> RuntimeState {
        RuntimeState::memory(MemoryRuntimeStateConfig::default())
    }

    async fn install(runtime: &RuntimeState, provider_id: &str, deadline_unix_secs: u64) {
        let value = serde_json::to_string(&AntigravityPoolPauseRecord {
            version: ANTIGRAVITY_POOL_PAUSE_RECORD_VERSION,
            provider_id: provider_id.to_string(),
            deadline_unix_secs,
        })
        .expect("record should serialize");
        runtime
            .kv_set(
                &antigravity_pool_pause_key(provider_id),
                value,
                Some(std::time::Duration::from_secs(60)),
            )
            .await
            .expect("pause should install");
    }

    #[test]
    fn tos_trigger_requires_the_complete_structured_google_error() {
        assert!(antigravity_tos_violation_response(
            AUTH_PROVIDER,
            403,
            Some(&tos_body())
        ));

        let non_triggers = [
            (AUTH_PROVIDER.to_string(), 400, tos_body()),
            (AUTH_PROVIDER.to_string(), 429, tos_body()),
            ("codex".to_string(), 403, tos_body()),
            (
                AUTH_PROVIDER.to_string(),
                403,
                json!({"error": {"status": "PERMISSION_DENIED", "details": [{
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "VALIDATION_REQUIRED"
                }]}})
                .to_string(),
            ),
            (
                AUTH_PROVIDER.to_string(),
                403,
                json!({"error": {"status": "RESOURCE_EXHAUSTED", "details": [{
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "TOS_VIOLATION"
                }]}})
                .to_string(),
            ),
            (
                AUTH_PROVIDER.to_string(),
                403,
                json!({"error": {"status": "PERMISSION_DENIED", "details": [{
                    "@type": "type.googleapis.com/google.rpc.RetryInfo",
                    "reason": "TOS_VIOLATION"
                }]}})
                .to_string(),
            ),
            (
                AUTH_PROVIDER.to_string(),
                403,
                json!({"error": {"status": "PERMISSION_DENIED", "message": "TOS_VIOLATION"}})
                    .to_string(),
            ),
        ];
        for (provider_type, status, body) in non_triggers {
            assert!(
                !antigravity_tos_violation_response(&provider_type, status, Some(&body)),
                "non-trigger must not pause: status={status}, body={body}"
            );
        }
        assert!(!antigravity_tos_violation_response(
            AUTH_PROVIDER,
            403,
            Some("not json")
        ));
        assert!(!antigravity_tos_violation_response(
            AUTH_PROVIDER,
            403,
            None
        ));
    }

    #[test]
    fn raw_pause_config_defaults_off_and_bounds_malformed_ttl() {
        assert!(!antigravity_pool_pause_settings(None).enabled);
        assert!(
            !antigravity_pool_pause_settings(Some(&json!({
                "pool_advanced": {"antigravity": {"tos_pause_enabled": "true"}}
            })))
            .enabled
        );
        let settings = antigravity_pool_pause_settings(Some(&json!({
            "pool_advanced": {"antigravity": {
                "tos_pause_enabled": true,
                "tos_pause_seconds": 86_401
            }}
        })));
        assert!(settings.enabled);
        assert_eq!(settings.ttl_seconds, 900);
        for seconds in [0, 86_401] {
            assert_eq!(
                antigravity_pool_pause_settings(Some(&json!({
                    "pool_advanced": {"antigravity": {"tos_pause_seconds": seconds}}
                })))
                .ttl_seconds,
                900
            );
        }
    }

    #[tokio::test]
    async fn pause_is_provider_scoped_and_disabled_read_clears_before_reenable() {
        let runtime = runtime();
        let paused = provider("provider-a", AUTH_PROVIDER, json!(true));
        let sibling = provider("provider-b", AUTH_PROVIDER, json!(true));
        install(&runtime, &paused.id, 2_000).await;

        assert!(antigravity_pool_pause_blocks_provider(
            &runtime,
            &paused.id,
            &paused.provider_type,
            paused.config.as_ref(),
            1_000
        )
        .await
        .expect("pause read should succeed"));
        assert!(!antigravity_pool_pause_blocks_provider(
            &runtime,
            &sibling.id,
            &sibling.provider_type,
            sibling.config.as_ref(),
            1_000
        )
        .await
        .expect("sibling read should succeed"));

        let disabled = provider("provider-a", AUTH_PROVIDER, json!(false));
        assert!(!antigravity_pool_pause_blocks_provider(
            &runtime,
            &disabled.id,
            &disabled.provider_type,
            disabled.config.as_ref(),
            1_001
        )
        .await
        .expect("disabled read should bypass"));
        assert!(runtime
            .kv_get(&antigravity_pool_pause_key(&paused.id))
            .await
            .expect("marker read should succeed")
            .is_none());
        assert!(!antigravity_pool_pause_blocks_provider(
            &runtime,
            &paused.id,
            &paused.provider_type,
            paused.config.as_ref(),
            1_002
        )
        .await
        .expect("re-enabled provider should not inherit stale marker"));
    }

    #[tokio::test]
    async fn expired_and_foreign_provider_markers_do_not_block() {
        let runtime = runtime();
        let paused = provider("provider-a", AUTH_PROVIDER, json!(true));
        install(&runtime, &paused.id, 1_000).await;
        assert!(!antigravity_pool_pause_blocks_provider(
            &runtime,
            &paused.id,
            &paused.provider_type,
            paused.config.as_ref(),
            1_000
        )
        .await
        .expect("expiry should recover"));

        install(&runtime, &paused.id, 2_000).await;
        let mut replaced = paused.clone();
        replaced.id = "provider-replacement".to_string();
        assert!(!antigravity_pool_pause_blocks_provider(
            &runtime,
            &replaced.id,
            &replaced.provider_type,
            replaced.config.as_ref(),
            1_001
        )
        .await
        .expect("replacement provider should remain independent"));
    }
}
