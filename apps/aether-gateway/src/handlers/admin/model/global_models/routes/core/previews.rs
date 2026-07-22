use super::super::super::super::{
    build_admin_global_model_mapping_preview_payload, AdminGlobalModelMappingPreviewRequest,
};
use super::super::super::helpers::build_admin_global_models_data_unavailable_response;
use super::shared::{
    global_model_missing_response, global_model_not_found_response, parse_required_json_body,
};
use crate::handlers::admin::model::shared::admin_global_model_mapping_preview_id;
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::GatewayError;
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};

pub(super) async fn maybe_build_local_admin_global_models_preview_response(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    let Some(decision) = request_context.decision() else {
        return Ok(None);
    };
    if decision.route_family.as_deref() != Some("global_models_manage")
        || decision.route_kind.as_deref() != Some("mapping_preview")
        || request_context.method() != http::Method::POST
    {
        return Ok(None);
    }
    if !state.has_global_model_data_reader() || !state.has_provider_catalog_data_reader() {
        return Ok(Some(build_admin_global_models_data_unavailable_response()));
    }
    let Some(global_model_id) = admin_global_model_mapping_preview_id(request_context.path())
    else {
        return Ok(Some(global_model_missing_response()));
    };
    let request =
        match parse_required_json_body::<AdminGlobalModelMappingPreviewRequest>(request_body) {
            Ok(request) => request,
            Err(response) => return Ok(Some(response)),
        };
    Ok(Some(
        match build_admin_global_model_mapping_preview_payload(state, &global_model_id, request)
            .await
        {
            Some(payload) => Json(payload).into_response(),
            None => global_model_not_found_response(&global_model_id),
        },
    ))
}
