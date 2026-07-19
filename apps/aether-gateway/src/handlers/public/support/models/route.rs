use std::collections::BTreeSet;
use std::fmt::Debug;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::model_catalog::StoredModelCatalogEntry;
use axum::{
    body::Body,
    response::{IntoResponse, Response},
    Json,
};
use futures_util::stream::{self, StreamExt};
use serde_json::{json, Value};
use tokio::time::timeout;
use tracing::warn;

use super::models_responses::{
    build_codex_models_list_response, build_empty_models_list_response,
    build_models_auth_error_response, build_models_not_found_response,
};
use super::models_shared::{filter_catalog_for_models, models_api_format, models_detail_id};
use super::{query_param_value, AppState, GatewayPublicRequestContext};

#[cfg(not(test))]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_millis(50);
const PUBLIC_MODELS_OWNER: &str = "aether";

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

fn is_codex_models_api_format(api_format: &str) -> bool {
    crate::ai_serving::normalize_api_format_alias(api_format) == "openai:responses"
}

fn is_codex_provider_row(row: &StoredModelCatalogEntry) -> bool {
    row.provider_type.trim().eq_ignore_ascii_case("codex")
}

fn codex_model_card_is_complete(card: &serde_json::Map<String, Value>) -> bool {
    card.get("slug").and_then(Value::as_str).is_some()
        && card.get("display_name").and_then(Value::as_str).is_some()
        && card
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .is_some()
        && card.get("shell_type").and_then(Value::as_str).is_some()
        && card.get("visibility").and_then(Value::as_str).is_some()
        && card
            .get("supported_in_api")
            .and_then(Value::as_bool)
            .is_some()
        && card.get("priority").and_then(Value::as_i64).is_some()
        && card
            .get("base_instructions")
            .and_then(Value::as_str)
            .is_some()
        && card
            .get("supports_reasoning_summary_parameter")
            .is_none_or(Value::is_boolean)
        && card
            .get("support_verbosity")
            .and_then(Value::as_bool)
            .is_some()
        && card
            .get("truncation_policy")
            .and_then(Value::as_object)
            .is_some()
        && card
            .get("supports_parallel_tool_calls")
            .and_then(Value::as_bool)
            .is_some()
        && card
            .get("experimental_supported_tools")
            .and_then(Value::as_array)
            .is_some()
}

fn project_codex_model_card(
    cached_models: &[Value],
    source_model: &str,
    global_model: &str,
) -> Option<Value> {
    let mut card = cached_models
        .iter()
        .find(|model| {
            model.get("id").and_then(Value::as_str) == Some(source_model)
                || model.get("slug").and_then(Value::as_str) == Some(source_model)
        })?
        .as_object()?
        .clone();
    if !codex_model_card_is_complete(&card) {
        return None;
    }

    card.remove("id");
    card.remove("api_formats");
    card.insert("slug".to_string(), Value::String(global_model.to_string()));
    Some(Value::Object(card))
}

fn codex_catalog_cards(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    value?
        .get("codex_models")
        .and_then(|catalog| catalog.get("cards"))
        .and_then(Value::as_object)
}

fn codex_catalog_card_candidates(row: &StoredModelCatalogEntry) -> Vec<Value> {
    [
        row.provider_model_config.as_ref(),
        row.global_model_config.as_ref(),
    ]
    .into_iter()
    .filter_map(codex_catalog_cards)
    .flat_map(|cards| {
        [
            row.provider_model_name.as_str(),
            row.global_model_name.as_str(),
            row.provider_model_id.as_str(),
        ]
        .into_iter()
        .filter_map(|name| cards.get(name).cloned())
        .collect::<Vec<_>>()
    })
    .collect()
}

async fn load_codex_model_cards(_state: &AppState, rows: &[StoredModelCatalogEntry]) -> Vec<Value> {
    let mut seen_global_models = BTreeSet::new();
    let mut cards = Vec::new();
    for row in rows.iter().filter(|row| is_codex_provider_row(row)) {
        if seen_global_models.contains(&row.global_model_name) {
            continue;
        }
        let cached_models = codex_catalog_card_candidates(row);
        let source_model = row.provider_model_name.as_str();
        let Some(card) =
            project_codex_model_card(&cached_models, source_model, row.global_model_name.as_str())
        else {
            continue;
        };
        seen_global_models.insert(row.global_model_name.clone());
        cards.push(card);
    }
    cards
}

fn build_openai_catalog_models_list_response(rows: &[StoredModelCatalogEntry]) -> Response<Body> {
    Json(json!({
        "object": "list",
        "data": rows.iter().map(|row| {
            json!({
                "id": row.global_model_name,
                "object": "model",
                "created": 0,
                "owned_by": PUBLIC_MODELS_OWNER,
            })
        }).collect::<Vec<_>>(),
    }))
    .into_response()
}

fn build_openai_catalog_model_detail_response(row: &StoredModelCatalogEntry) -> Response<Body> {
    Json(json!({
        "id": row.global_model_name,
        "object": "model",
        "created": 0,
        "owned_by": PUBLIC_MODELS_OWNER,
    }))
    .into_response()
}

fn build_claude_catalog_models_list_response(
    rows: &[StoredModelCatalogEntry],
    before_id: Option<&str>,
    after_id: Option<&str>,
    limit: usize,
) -> Response<Body> {
    let model_data = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.global_model_name,
                "type": "model",
                "display_name": row.global_model_name,
                "created_at": Value::Null,
            })
        })
        .collect::<Vec<_>>();

    let mut start_idx = 0usize;
    if let Some(after_id) = after_id {
        if let Some(index) = model_data.iter().position(|item| item["id"] == after_id) {
            start_idx = index.saturating_add(1);
        }
    }
    let mut end_idx = model_data.len();
    if let Some(before_id) = before_id {
        if let Some(index) = model_data.iter().position(|item| item["id"] == before_id) {
            end_idx = index;
        }
    }
    let window = &model_data[start_idx.min(end_idx)..end_idx];
    let paginated = window.iter().take(limit).cloned().collect::<Vec<_>>();
    let first_id = paginated
        .first()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    let last_id = paginated
        .last()
        .and_then(|item| item.get("id"))
        .cloned()
        .unwrap_or(Value::Null);

    Json(json!({
        "data": paginated,
        "has_more": window.len() > limit,
        "first_id": first_id,
        "last_id": last_id,
    }))
    .into_response()
}

fn build_claude_catalog_model_detail_response(row: &StoredModelCatalogEntry) -> Response<Body> {
    Json(json!({
        "id": row.global_model_name,
        "type": "model",
        "display_name": row.global_model_name,
        "created_at": Value::Null,
    }))
    .into_response()
}

fn build_gemini_catalog_model_value(row: &StoredModelCatalogEntry) -> Value {
    json!({
        "name": format!("models/{}", row.global_model_name),
        "baseModelId": row.global_model_name,
        "version": "001",
        "displayName": row.global_model_name,
        "description": format!("Model {}", row.global_model_name),
        "inputTokenLimit": 128000,
        "outputTokenLimit": 8192,
        "supportedGenerationMethods": ["generateContent", "countTokens"],
        "temperature": 1.0,
        "maxTemperature": 2.0,
        "topP": 0.95,
        "topK": 64,
    })
}

fn build_gemini_catalog_models_list_response(
    rows: &[StoredModelCatalogEntry],
    page_size: usize,
    page_token: Option<&str>,
) -> Response<Body> {
    let start_idx = page_token
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let end_idx = start_idx.saturating_add(page_size);
    let window = rows
        .iter()
        .skip(start_idx)
        .take(page_size)
        .map(build_gemini_catalog_model_value)
        .collect::<Vec<_>>();
    let mut payload = json!({ "models": window });
    if end_idx < rows.len() {
        payload["nextPageToken"] = Value::String(end_idx.to_string());
    }
    Json(payload).into_response()
}

fn build_gemini_catalog_model_detail_response(row: &StoredModelCatalogEntry) -> Response<Body> {
    Json(build_gemini_catalog_model_value(row)).into_response()
}

async fn list_model_rows_for_client_format(
    state: &AppState,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<Vec<StoredModelCatalogEntry>> {
    let rows = await_models_route_read("model_catalog", state.data.list_model_catalog()).await?;
    // 模型列表只表达配置与调用方权限；临时额度、并发和 Key 状态由实际请求调度处理。
    Some(filter_catalog_for_models(rows, auth_snapshot, api_format))
}

async fn detail_model_rows_for_client_format(
    state: &AppState,
    model_id: &str,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
) -> Option<Vec<StoredModelCatalogEntry>> {
    let rows = await_models_route_read(
        "model_catalog_detail",
        state.data.read_model_catalog_detail(model_id),
    )
    .await?;
    retain_routable_model_rows(state, rows, api_format, auth_snapshot, now_unix_secs).await
}

async fn retain_routable_model_rows(
    state: &AppState,
    rows: Vec<StoredModelCatalogEntry>,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
) -> Option<Vec<StoredModelCatalogEntry>> {
    let rows = filter_catalog_for_models(rows, auth_snapshot, api_format);
    let visibility = async {
        stream::iter(rows.into_iter().map(|row| async move {
            crate::ai_serving::PlannerAppState::new(state)
                .list_selectable_candidates(
                    api_format,
                    &row.global_model_name,
                    false,
                    None,
                    auth_snapshot,
                    None,
                    now_unix_secs,
                    false,
                )
                .await
                .map(|candidates| (!candidates.is_empty()).then_some(row))
        }))
        .buffered(8)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, crate::GatewayError>>()
    };
    let results = await_models_route_read("model_routability", visibility).await?;
    Some(results.into_iter().flatten().collect())
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
    if !state.has_minimal_candidate_selection_reader() {
        return None;
    }

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
            if is_codex_models_api_format(api_format) {
                let models = load_codex_model_cards(state, &rows).await;
                return Some(build_codex_models_list_response(models));
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
                    build_claude_catalog_models_list_response(
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
                    build_gemini_catalog_models_list_response(
                        &rows,
                        page_size,
                        page_token.as_deref(),
                    )
                }
                _ => build_openai_catalog_models_list_response(&rows),
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
                now_unix_secs,
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
                "claude:messages" => build_claude_catalog_model_detail_response(row),
                "gemini:generate_content" => build_gemini_catalog_model_detail_response(row),
                _ => build_openai_catalog_model_detail_response(row),
            };
            Some(response)
        }
        _ => Some(build_models_auth_error_response(api_format)),
    }
}
