use crate::ai_serving::{
    build_provider_key_pool_score_upsert, provider_key_pool_score_id, provider_key_pool_score_scope,
};
use crate::handlers::admin::admin_provider_pool_config;
use crate::handlers::admin::provider::shared::paths::admin_clear_quota_exhausted_key_id;
use crate::handlers::admin::provider::shared::support::admin_provider_pool_quota_probe_active_members_key;
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::GatewayError;
use aether_data_contracts::repository::pool_scores::{
    GetPoolMemberScoresByIdsQuery, PoolMemberHardState, PoolMemberIdentity,
};
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) async fn maybe_handle(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    _request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    let Some(decision) = request_context.decision() else {
        return Ok(None);
    };
    if decision.route_family.as_deref() != Some("endpoints_manage")
        || decision.route_kind.as_deref() != Some("clear_quota_exhausted")
        || request_context.method() != http::Method::POST
        || !request_context.path().ends_with("/clear-quota-exhausted")
    {
        return Ok(None);
    }

    let Some(key_id) = admin_clear_quota_exhausted_key_id(request_context.path()) else {
        return Ok(Some(not_found_response("Key 不存在")));
    };
    let Some(key) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(Some(not_found_response(format!("Key {key_id} 不存在"))));
    };
    let Some(provider) = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&key.provider_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(Some(not_found_response(format!(
            "Provider {} 不存在",
            key.provider_id
        ))));
    };

    let projection_lock = state
        .as_ref()
        .acquire_provider_key_quota_projection_lock(&provider.id, &key_id)
        .await?;
    let recovery_result = async {
        let cleared = state
            .clear_provider_catalog_key_quota_scheduling_state(&key_id)
            .await?;
        // Reconcile even for an idempotent retry. A previous request may have
        // cleared scheduling successfully and then failed while rebuilding the
        // derived Pool score.
        rebuild_pool_score_after_quota_recovery(state, &provider, &key_id).await?;
        // Membership removal is idempotent and keeps a previously stale probe
        // set from reintroducing the recovered key before the next refresh.
        let _ = state
            .as_ref()
            .runtime_state
            .set_remove(
                &admin_provider_pool_quota_probe_active_members_key(&provider.id),
                &key_id,
            )
            .await;
        Ok::<bool, GatewayError>(cleared)
    }
    .await;
    state
        .as_ref()
        .release_provider_key_quota_projection_lock(projection_lock)
        .await;
    let cleared = recovery_result?;

    Ok(Some(
        Json(json!({
            "key_id": key_id,
            "cleared": cleared,
            "message": if cleared {
                "已清除额度耗尽阻断，Key 将按其他调度状态重新评估"
            } else {
                "该 Key 当前无额度耗尽阻断，无需清除"
            },
        }))
        .into_response(),
    ))
}

async fn rebuild_pool_score_after_quota_recovery(
    state: &AdminAppState<'_>,
    provider: &aether_data_contracts::repository::provider_catalog::StoredProviderCatalogProvider,
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

fn not_found_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::NOT_FOUND,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}
