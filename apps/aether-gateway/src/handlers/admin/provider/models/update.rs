use super::payloads::build_admin_provider_model_response;
use crate::handlers::admin::provider::shared::paths::admin_provider_model_route_parts;
use crate::handlers::admin::provider::shared::payloads::AdminProviderModelUpdatePatch;
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::GatewayError;
use aether_data_contracts::repository::global_models::{
    StoredAdminProviderModel, UpdateAdminProviderModelWithBindingsRecord,
    UpsertModelEndpointBindingRecord,
};
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) async fn maybe_handle(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    if request_context.route_family() == Some("provider_models_manage")
        && request_context.route_kind() == Some("update_provider_model")
        && request_context.method() == http::Method::PATCH
        && request_context.path().contains("/models/")
    {
        let Some((provider_id, model_id)) =
            admin_provider_model_route_parts(request_context.path())
        else {
            return Ok(Some(
                (
                    http::StatusCode::NOT_FOUND,
                    Json(json!({ "detail": "Model 不存在" })),
                )
                    .into_response(),
            ));
        };
        let Some(provider) = state
            .read_provider_catalog_providers_by_ids(std::slice::from_ref(&provider_id))
            .await?
            .into_iter()
            .next()
        else {
            return Ok(Some(
                (
                    http::StatusCode::NOT_FOUND,
                    Json(json!({ "detail": format!("Provider {provider_id} 不存在") })),
                )
                    .into_response(),
            ));
        };
        let Some(existing) = state
            .get_admin_provider_model(&provider_id, &model_id)
            .await?
        else {
            return Ok(Some(
                (
                    http::StatusCode::NOT_FOUND,
                    Json(json!({ "detail": format!("Model {model_id} 不存在") })),
                )
                    .into_response(),
            ));
        };
        let Some(request_body) = request_body else {
            return Ok(Some(
                (
                    http::StatusCode::BAD_REQUEST,
                    Json(json!({ "detail": "请求体不能为空" })),
                )
                    .into_response(),
            ));
        };
        let raw_value = match serde_json::from_slice::<serde_json::Value>(request_body) {
            Ok(value) => value,
            Err(_) => {
                return Ok(Some(
                    (
                        http::StatusCode::BAD_REQUEST,
                        Json(json!({ "detail": "请求体必须是合法的 JSON 对象" })),
                    )
                        .into_response(),
                ));
            }
        };
        let Some(raw_payload) = raw_value.as_object().cloned() else {
            return Ok(Some(
                (
                    http::StatusCode::BAD_REQUEST,
                    Json(json!({ "detail": "请求体必须是合法的 JSON 对象" })),
                )
                    .into_response(),
            ));
        };
        let patch = match AdminProviderModelUpdatePatch::from_object(raw_payload) {
            Ok(patch) => patch,
            Err(_) => {
                return Ok(Some(
                    (
                        http::StatusCode::BAD_REQUEST,
                        Json(json!({ "detail": "请求体必须是合法的 JSON 对象" })),
                    )
                        .into_response(),
                ));
            }
        };
        let automatic_binding_inputs_changed = patch.contains("provider_model_name")
            || patch.contains("provider_model_mappings")
            || patch.contains("global_model_id");
        let endpoint_bindings = if let Some(bindings) = patch.payload.endpoint_bindings.as_ref() {
            if bindings.is_empty() {
                return Ok(Some(
                    (
                        http::StatusCode::BAD_REQUEST,
                        Json(json!({ "detail": "至少提供一个 Endpoint 绑定" })),
                    )
                        .into_response(),
                ));
            }
            let provider_endpoint_ids = state
                .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(&provider_id))
                .await?
                .into_iter()
                .map(|endpoint| endpoint.id)
                .collect::<BTreeSet<_>>();
            let mut records = Vec::with_capacity(bindings.len());
            let mut seen_endpoint_ids = BTreeSet::new();
            for binding in bindings {
                let endpoint_id = binding.endpoint_id.trim();
                if endpoint_id.is_empty() || !provider_endpoint_ids.contains(endpoint_id) {
                    return Ok(Some(
                        (
                            http::StatusCode::BAD_REQUEST,
                            Json(json!({
                                "detail": format!(
                                    "Endpoint {} 不属于当前 Provider",
                                    binding.endpoint_id
                                )
                            })),
                        )
                            .into_response(),
                    ));
                }
                if !seen_endpoint_ids.insert(endpoint_id.to_string()) {
                    return Ok(Some(
                        (
                            http::StatusCode::BAD_REQUEST,
                            Json(json!({
                                "detail": format!("Endpoint {endpoint_id} 在绑定列表中重复")
                            })),
                        )
                            .into_response(),
                    ));
                }
                records.push(
                    UpsertModelEndpointBindingRecord::new(
                        existing.id.clone(),
                        endpoint_id.to_string(),
                        "manual".to_string(),
                        binding.is_active,
                    )
                    .map_err(|err| GatewayError::Internal(err.to_string()))?,
                );
            }
            Some(records)
        } else {
            None
        };
        let record = match state
            .build_admin_provider_model_update_record(&existing, patch)
            .await
        {
            Ok(record) => record,
            Err(detail) => {
                return Ok(Some(
                    (
                        http::StatusCode::BAD_REQUEST,
                        Json(json!({ "detail": detail })),
                    )
                        .into_response(),
                ));
            }
        };
        let automatic_binding_update = if automatic_binding_inputs_changed {
            if let Some(bindings) = endpoint_bindings.as_ref() {
                Some((
                    bindings
                        .iter()
                        .map(|binding| binding.endpoint_id.clone())
                        .collect(),
                    "manual".to_string(),
                ))
            } else {
                let prospective_model = StoredAdminProviderModel {
                    id: record.id.clone(),
                    provider_id: record.provider_id.clone(),
                    global_model_id: record.global_model_id.clone(),
                    provider_model_name: record.provider_model_name.clone(),
                    provider_model_mappings: record.provider_model_mappings.clone(),
                    price_per_request: record.price_per_request,
                    tiered_pricing: record.tiered_pricing.clone(),
                    supports_vision: record.supports_vision,
                    supports_function_calling: record.supports_function_calling,
                    supports_streaming: record.supports_streaming,
                    supports_extended_thinking: record.supports_extended_thinking,
                    supports_image_generation: record.supports_image_generation,
                    is_active: record.is_active,
                    is_available: record.is_available,
                    config: record.config.clone(),
                    created_at_unix_ms: existing.created_at_unix_ms,
                    updated_at_unix_secs: existing.updated_at_unix_secs,
                    global_model_name: existing.global_model_name.clone(),
                    global_model_display_name: existing.global_model_display_name.clone(),
                    global_model_default_price_per_request: existing
                        .global_model_default_price_per_request,
                    global_model_default_tiered_pricing: existing
                        .global_model_default_tiered_pricing
                        .clone(),
                    global_model_supported_capabilities: existing
                        .global_model_supported_capabilities
                        .clone(),
                    global_model_config: existing.global_model_config.clone(),
                };
                let (endpoint_ids, source) = match state
                    .infer_unambiguous_admin_model_endpoint_ids(&provider_id, &prospective_model)
                    .await
                {
                    Ok(inference) => inference,
                    Err(GatewayError::Client { message, .. }) => {
                        return Ok(Some(
                            (
                                http::StatusCode::BAD_REQUEST,
                                Json(json!({ "detail": message })),
                            )
                                .into_response(),
                        ));
                    }
                    Err(err) => return Err(err),
                };
                Some((endpoint_ids, source.to_string()))
            }
        } else {
            None
        };
        let (automatic_endpoint_ids, automatic_source) = match automatic_binding_update {
            Some((endpoint_ids, source)) => (Some(endpoint_ids), Some(source)),
            None => (None, None),
        };
        let mutation = UpdateAdminProviderModelWithBindingsRecord::new(
            record,
            automatic_endpoint_ids,
            automatic_source,
            endpoint_bindings.unwrap_or_default(),
        )
        .map_err(|err| GatewayError::Internal(err.to_string()))?;
        return Ok(Some(
            match state
                .update_admin_provider_model_with_bindings(&mutation)
                .await?
            {
                Some(updated) => {
                    let now_unix_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_secs())
                        .unwrap_or(0);
                    Json(build_admin_provider_model_response(
                        &provider,
                        &updated,
                        now_unix_secs,
                    ))
                    .into_response()
                }
                None => (
                    http::StatusCode::NOT_FOUND,
                    Json(json!({ "detail": format!("Model {model_id} 不存在") })),
                )
                    .into_response(),
            },
        ));
    }

    Ok(None)
}
