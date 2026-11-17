use crate::ai_serving::{
    build_provider_key_pool_score_upsert, provider_key_pool_score_id, provider_key_pool_score_scope,
};
use crate::handlers::admin::admin_provider_pool_config;
use crate::handlers::admin::request::AdminAppState;
use crate::GatewayError;
use aether_data_contracts::repository::pool_scores::{
    GetPoolMemberScoresByIdsQuery, PoolMemberHardState, PoolMemberIdentity,
};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider;
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
