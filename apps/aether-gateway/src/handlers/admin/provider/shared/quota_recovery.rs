use crate::ai_serving::{
    build_provider_key_pool_score_upsert, provider_key_pool_score_id, provider_key_pool_score_scope,
};
use crate::handlers::admin::admin_provider_pool_config;
use crate::handlers::admin::request::AdminAppState;
use crate::GatewayError;
use aether_data_contracts::repository::pool_scores::{
    GetPoolMemberScoresByIdsQuery, PoolMemberHardState, PoolMemberIdentity,
};
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeySchedulingStateCasUpdate, StoredProviderCatalogKey,
    StoredProviderCatalogProvider,
};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use super::support::admin_provider_pool_quota_probe_active_members_key;

/// 调用方持有额度投影锁；幂等重建，避免清除阻断后遗留旧 Pool 分数或探测成员。
pub(crate) async fn reconcile_quota_recovery(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    key_id: &str,
) -> Result<(), GatewayError> {
    rebuild_pool_score_after_quota_recovery(state, provider, key_id).await?;
    let _ = state
        .as_ref()
        .runtime_state
        .set_remove(
            &admin_provider_pool_quota_probe_active_members_key(&provider.id),
            key_id,
        )
        .await;
    Ok(())
}

async fn rebuild_pool_score_after_quota_recovery(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    key_id: &str,
) -> Result<(), GatewayError> {
    let Some(pool_config) = admin_provider_pool_config(provider) else {
        return Ok(());
    };
    let Some(key) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
        .await?
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    let identity = PoolMemberIdentity::provider_api_key(provider.id.clone(), key.id.clone());
    let scope = provider_key_pool_score_scope();
    let score_id = provider_key_pool_score_id(&identity, &scope);
    let existing = state
        .as_ref()
        .data
        .get_pool_member_scores_by_ids(&GetPoolMemberScoresByIdsQuery {
            ids: vec![score_id],
        })
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?
        .into_iter()
        .next();
    if existing
        .as_ref()
        .is_some_and(|score| score.hard_state != PoolMemberHardState::QuotaExhausted)
    {
        // The pool score is owned by another blocker (for example auth-invalid
        // or banned). Clearing quota state must not recover that condition.
        return Ok(());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let score = build_provider_key_pool_score_upsert(
        &key,
        provider.provider_type.as_str(),
        existing.as_ref(),
        now,
        pool_config.score_rules,
    );
    state
        .as_ref()
        .data
        .upsert_pool_member_score(score)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?;
    Ok(())
}
/// Match the actual OAuth quota hold captured before the request. A disabled
/// account or a manually written block is never released by a quota refresh.
pub(crate) async fn recover_oauth_quota_hold(
    state: &AdminAppState<'_>,
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    observed_at: u64,
    incoming: &Value,
) -> Result<(), GatewayError> {
    let held = key
        .status_snapshot
        .as_ref()
        .and_then(|s| s.get("scheduling"));
    if !key.is_active
        || !held.is_some_and(|s| {
            s.get("credential_fingerprint")
                .and_then(Value::as_str)
                .is_some()
                && matches!(
                    s.get("code").and_then(Value::as_str),
                    Some("quota_exhausted" | "quota_suspected")
                )
        })
    {
        return Ok(());
    }
    let Some(current) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&key.id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    let Some(bucket) = current
        .upstream_metadata
        .as_ref()
        .and_then(|m| m.get(provider_type))
    else {
        return Ok(());
    };
    let Some(snapshot) = current
        .status_snapshot
        .as_ref()
        .and_then(|s| s.get("quota"))
    else {
        return Ok(());
    };
    let positive = match provider_type {
        "codex" => {
            codex_account_windows_available(incoming) && codex_account_windows_available(bucket)
        }
        "antigravity" => incoming
            .get("quota_by_model")
            .and_then(Value::as_object)
            .is_some_and(|models| {
                models.iter().any(|(model, window)| {
                    window.get("disabled").and_then(Value::as_bool) != Some(true)
                        && window
                            .get("remaining_fraction")
                            .and_then(Value::as_f64)
                            .is_some_and(|n| n > 0.0)
                        && bucket.get("quota_by_model").and_then(|m| m.get(model)) == Some(window)
                        && aether_provider_pool::provider_pool_key_model_quota_exhausted(
                            &current,
                            provider_type,
                            model,
                        ) == Some(false)
                })
            }),
        _ => false,
    };
    if positive
        && current.is_active
        && current.oauth_invalid_reason.is_none()
        && current
            .status_snapshot
            .as_ref()
            .and_then(|s| s.get("scheduling"))
            == held
    {
        recover_quota_scheduling(state, key, observed_at, snapshot).await?;
    }
    Ok(())
}

/// A sparse header patch or one healthy window is not proof of account recovery.
pub(crate) fn codex_account_windows_available(bucket: &Value) -> bool {
    !aether_admin::provider::quota::codex_rate_limit_metadata_exhausted(bucket)
        && bucket.get("account_disabled").and_then(Value::as_bool) != Some(true)
        && ["primary", "secondary"].iter().all(|prefix| {
            bucket
                .get(format!("{prefix}_window_minutes"))
                .and_then(Value::as_f64)
                != Some(0.0)
                && bucket
                    .get(format!("{prefix}_used_percent"))
                    .and_then(Value::as_f64)
                    .is_some_and(|used| (0.0..100.0 - 1e-6).contains(&used))
        })
}

/// Re-admit only quota holds older than this confirmed snapshot for the same credential.
pub(crate) async fn recover_quota_scheduling(
    state: &AdminAppState<'_>,
    observed_key: &StoredProviderCatalogKey,
    observed_at: u64,
    snapshot: &Value,
) -> Result<(), GatewayError> {
    let key_id = &observed_key.id;
    let provider_id = &observed_key.provider_id;
    let projection_lock = state
        .app()
        .acquire_provider_key_quota_projection_lock(provider_id, key_id)
        .await?;
    let result = async {
        let Some(key) = state
            .app()
            .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(key_id))
            .await?
            .into_iter()
            .next()
        else {
            return Ok(());
        };
        // 查询期间换凭据或出现更新的额度错误时，旧查询不能解除新的阻断。
        if key.provider_id != *provider_id
            || key.encrypted_api_key != observed_key.encrypted_api_key
            || key.encrypted_auth_config != observed_key.encrypted_auth_config
            || key.auth_type != observed_key.auth_type
            || key
                .status_snapshot
                .as_ref()
                .and_then(|status| status.get("quota"))
                != Some(snapshot)
        {
            return Ok(());
        }
        let scheduling = key
            .status_snapshot
            .as_ref()
            .and_then(|status| status.get("scheduling"));
        if let Some(scheduling) = scheduling.filter(|value| !value.is_null()) {
            if matches!(observed_key.status_snapshot.as_ref().and_then(|s| s.get("scheduling")),
                Some(original) if original.get("credential_fingerprint").is_some() && original != scheduling)
            {
                return Ok(());
            }
            if !matches!(
                scheduling.get("code").and_then(Value::as_str),
                Some("quota_suspected" | "quota_exhausted")
            ) || scheduling
                .get("last_observed_at")
                .and_then(Value::as_u64)
                .is_none_or(|observed| observed >= observed_at)
            {
                return Ok(());
            }
            let cleared = state
                .app()
                .compare_and_update_provider_catalog_key_scheduling_state(
                    &ProviderCatalogKeySchedulingStateCasUpdate {
                        key_id: key.id.clone(),
                        expected_encrypted_api_key: key.encrypted_api_key.clone(),
                        expected_encrypted_auth_config: key.encrypted_auth_config.clone(),
                        expected_auth_type: key.auth_type.clone(),
                        expected_scheduling: Some(scheduling.clone()),
                        scheduling: None,
                        updated_at_unix_secs: Some(observed_at),
                    },
                )
                .await?;
            if !cleared {
                return Ok(());
            }
        }
        if let Some(provider) = state
            .read_provider_catalog_providers_by_ids(std::slice::from_ref(provider_id))
            .await?
            .into_iter()
            .next()
        {
            reconcile_quota_recovery(state, &provider, key_id).await?;
        }
        Ok(())
    }
    .await;
    state
        .app()
        .release_provider_key_quota_projection_lock(projection_lock)
        .await;
    result
}

#[cfg(test)]
mod tests {
    use super::codex_account_windows_available;
    use serde_json::json;

    #[test]
    fn codex_quota_recovery_requires_both_live_windows() {
        let mut quota = json!({"primary_used_percent": 10});
        assert!(!codex_account_windows_available(&quota));
        quota["secondary_used_percent"] = json!(100);
        quota["allowed"] = json!(true);
        assert!(!codex_account_windows_available(&quota));
        quota["secondary_used_percent"] = json!(20);
        assert!(codex_account_windows_available(&quota));
        quota["primary_window_minutes"] = json!(0);
        assert!(!codex_account_windows_available(&quota));
    }
}
