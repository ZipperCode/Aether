use crate::handlers::admin::provider::shared::paths::admin_clear_quota_exhausted_key_id;
use crate::handlers::admin::provider::shared::quota_recovery::reconcile_quota_recovery;
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::GatewayError;
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

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
        reconcile_quota_recovery(state, &provider, &key_id).await?;
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

fn not_found_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::NOT_FOUND,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}
