use std::fmt::Debug;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{body::Body, response::Response};
use tokio::time::timeout;
use tracing::warn;

use super::models_responses::{
    build_claude_model_detail_response, build_claude_models_list_response,
    build_empty_models_list_response, build_gemini_model_detail_response,
    build_gemini_models_list_response, build_models_auth_error_response,
    build_models_not_found_response, build_openai_model_detail_response,
    build_openai_models_list_response,
};
use super::models_shared::{filter_catalog_for_models, models_api_format, models_detail_id};
use super::{query_param_value, AppState, GatewayPublicRequestContext};

#[cfg(not(test))]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_millis(50);

async fn await_models_route_read<T, E, Fut>(operation: &'static str, future: Fut) -> Option<T>
where
    E: Debug,
    Fut: Future<Output = Result<T, E>>,
{
    match timeout(MODELS_ROUTE_READ_TIMEOUT, future).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            warn!(
                event_name = "models_route_read_error",
                log_type = "ops",
                operation,
                error = ?error,
                "gateway local models route read failed"
            );
            None
        }
        Err(_) => {
            warn!(
                event_name = "models_route_read_timeout",
                log_type = "ops",
                operation,
                timeout_ms = MODELS_ROUTE_READ_TIMEOUT.as_millis() as u64,
                "gateway local models route read timed out"
            );
            None
        }
    }
}

fn build_models_read_fallback_response(
    request_context: &GatewayPublicRequestContext,
    api_format: &str,
) -> Response<Body> {
    let route_kind = request_context
        .control_decision
        .as_ref()
        .and_then(|decision| decision.route_kind.as_deref());
    match route_kind {
        Some("detail") => {
            let model_id = models_detail_id(&request_context.request_path)
                .unwrap_or_else(|| "unknown".to_string());
            build_models_not_found_response(&model_id, api_format)
        }
        _ => build_empty_models_list_response(api_format),
    }
}

async fn list_model_rows_for_client_format(
    state: &AppState,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<Vec<aether_data_contracts::repository::model_catalog::StoredModelCatalogEntry>> {
    let rows = await_models_route_read("model_catalog", state.data.list_model_catalog()).await?;
    Some(filter_catalog_for_models(rows, auth_snapshot, api_format))
}

async fn detail_model_rows_for_client_format(
    state: &AppState,
    model_id: &str,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<Vec<aether_data_contracts::repository::model_catalog::StoredModelCatalogEntry>> {
    let rows = await_models_route_read(
        "model_catalog_detail",
        state.data.read_model_catalog_detail(model_id),
    )
    .await?;
    Some(filter_catalog_for_models(rows, auth_snapshot, api_format))
}

pub(super) async fn maybe_build_local_models_route_response(
    state: &AppState,
    request_context: &GatewayPublicRequestContext,
) -> Option<Response<Body>> {
    let decision = request_context.control_decision.as_ref()?;
    if decision.route_family.as_deref() != Some("models") {
        return None;
    }
    let api_format = models_api_format(request_context)?;
    let auth_context = decision.auth_context.as_ref()?;
    let now_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let auth_snapshot = match await_models_route_read(
        "auth_api_key_snapshot",
        state.data.read_auth_api_key_snapshot(
            &auth_context.user_id,
            &auth_context.api_key_id,
            now_unix_secs,
        ),
    )
    .await
    {
        Some(snapshot) => snapshot,
        None => {
            return Some(build_models_read_fallback_response(
                request_context,
                api_format,
            ))
        }
    };
    let auth_snapshot = auth_snapshot.as_ref();

    match decision.route_kind.as_deref() {
        Some("list") => {
            let rows =
                match list_model_rows_for_client_format(state, api_format, auth_snapshot).await {
                    Some(rows) => rows,
                    None => {
                        return Some(build_models_read_fallback_response(
                            request_context,
                            api_format,
                        ))
                    }
                };
            if rows.is_empty() {
                return Some(build_empty_models_list_response(api_format));
            }
            let response = match api_format {
                "claude:messages" => {
                    let before_id = query_param_value(
                        request_context.request_query_string.as_deref(),
                        "before_id",
                    );
                    let after_id = query_param_value(
                        request_context.request_query_string.as_deref(),
                        "after_id",
                    );
                    let limit =
                        query_param_value(request_context.request_query_string.as_deref(), "limit")
                            .and_then(|value| value.parse::<usize>().ok())
                            .filter(|value| *value > 0)
                            .unwrap_or(20);
                    build_claude_models_list_response(
                        &rows,
                        before_id.as_deref(),
                        after_id.as_deref(),
                        limit,
                    )
                }
                "gemini:generate_content" => {
                    let page_size = query_param_value(
                        request_context.request_query_string.as_deref(),
                        "pageSize",
                    )
                    .and_then(|value| value.parse::<usize>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(50);
                    let page_token = query_param_value(
                        request_context.request_query_string.as_deref(),
                        "pageToken",
                    );
                    build_gemini_models_list_response(&rows, page_size, page_token.as_deref())
                }
                _ => build_openai_models_list_response(&rows),
            };
            Some(response)
        }
        Some("detail") => {
            let model_id = models_detail_id(&request_context.request_path)?;
            let rows = match detail_model_rows_for_client_format(
                state,
                &model_id,
                api_format,
                auth_snapshot,
            )
            .await
            {
                Some(rows) => rows,
                None => {
                    return Some(build_models_read_fallback_response(
                        request_context,
                        api_format,
                    ))
                }
            };
            let Some(row) = rows.iter().find(|row| row.global_model_name == model_id) else {
                return Some(build_models_not_found_response(&model_id, api_format));
            };
            let response = match api_format {
                "claude:messages" => build_claude_model_detail_response(row),
                "gemini:generate_content" => build_gemini_model_detail_response(row),
                _ => build_openai_model_detail_response(row),
            };
            Some(response)
        }
        _ => Some(build_models_auth_error_response(api_format)),
    }
}
