use std::collections::BTreeSet;
use std::fmt::Debug;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::candidate_selection::StoredMinimalCandidateSelectionRow;
use aether_data_contracts::repository::global_models::PublicGlobalModelQuery;
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
use super::models_shared::{
    filter_catalog_for_models, filter_eligible_model_rows, filter_global_models_for_models,
    models_api_format, models_detail_id, project_gemini_model_value,
    provider_type_supports_gemini_count_tokens,
};
use super::{query_param_value, AppState, GatewayPublicRequestContext};

#[cfg(not(test))]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const MODELS_ROUTE_READ_TIMEOUT: Duration = Duration::from_millis(50);
const PUBLIC_MODELS_OWNER: &str = "aether";
const PUBLIC_MODELS_FETCH_LIMIT: usize = 10_000;
const CODEX_MODELS_QUERY_API_FORMATS: &[&str] = &["openai:responses"];
const CODEX_MODELS_MAX_RESPONSE_MODELS: usize = 512;
const CODEX_MODELS_MAX_RESPONSE_JSON_BYTES: usize = 8 * 1024 * 1024;

/// 校验聚合后的 Codex 动态目录是否仍在响应数量与序列化体积边界内。
fn codex_projected_catalog_fits_response_limits(cards: &[Value]) -> bool {
    if cards.len() > CODEX_MODELS_MAX_RESPONSE_MODELS {
        return false;
    }
    serde_json::to_vec(&serde_json::json!({ "models": cards }))
        .is_ok_and(|body| body.len() <= CODEX_MODELS_MAX_RESPONSE_JSON_BYTES)
}

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

/// 按稳定调度字段排序 Codex 候选，确保目录投影在节点间可复现。
fn sort_model_rows(
    mut rows: Vec<StoredMinimalCandidateSelectionRow>,
) -> Vec<StoredMinimalCandidateSelectionRow> {
    rows.sort_by(|left, right| {
        left.global_model_name
            .cmp(&right.global_model_name)
            .then(left.provider_priority.cmp(&right.provider_priority))
            .then(left.key_internal_priority.cmp(&right.key_internal_priority))
            .then(left.provider_id.cmp(&right.provider_id))
            .then(left.endpoint_id.cmp(&right.endpoint_id))
            .then(left.key_id.cmp(&right.key_id))
            .then(left.model_id.cmp(&right.model_id))
    });
    rows
}

fn is_codex_models_api_format(api_format: &str) -> bool {
    crate::ai_serving::normalize_api_format_alias(api_format) == "openai:responses"
}

fn is_standard_openai_models_api_format(api_format: &str) -> bool {
    crate::ai_serving::normalize_api_format_alias(api_format) == "openai:chat"
}

fn is_codex_provider_row(row: &StoredMinimalCandidateSelectionRow) -> bool {
    row.provider_type.trim().eq_ignore_ascii_case("codex")
}

async fn load_codex_model_cards(
    state: &AppState,
    rows: &[StoredMinimalCandidateSelectionRow],
    targets: &[crate::model_fetch::CodexCatalogTarget],
    client_version: crate::model_fetch::NormalizedCodexClientVersion,
) -> (Vec<Value>, Option<String>) {
    let catalogs = crate::model_fetch::load_codex_catalogs(state, targets, &client_version).await;
    for target in catalogs.stale_targets() {
        let state = state.clone();
        let target = target.clone();
        let client_version = client_version.clone();
        tokio::spawn(async move {
            crate::model_fetch::refresh_codex_catalog_target(&state, &target, &client_version)
                .await;
        });
    }
    if !catalogs.is_complete() {
        warn!(
            event_name = "codex_catalog_aggregate_incomplete",
            client_version = %client_version.as_str(),
            target_count = targets.len(),
            "Codex catalog aggregation was incomplete; serving cards from available last-known-good snapshots"
        );
    }
    let mut seen_global_models = BTreeSet::new();
    let possible_inference_catalogs = rows
        .iter()
        .filter(|row| is_codex_provider_row(row))
        .map(|row| (row.provider_id.clone(), row.key_id.clone()))
        .collect::<BTreeSet<_>>();
    let expected_global_models = rows
        .iter()
        .filter(|row| is_codex_provider_row(row))
        .map(|row| row.global_model_name.clone())
        .collect::<BTreeSet<_>>();
    let mut cards = Vec::new();
    for row in rows.iter().filter(|row| is_codex_provider_row(row)) {
        if seen_global_models.contains(&row.global_model_name) {
            continue;
        }
        let Some(snapshot) = catalogs.snapshot(&row.provider_id, &row.key_id) else {
            continue;
        };
        let source_model =
            aether_scheduler_core::select_provider_model_name(row, "openai:responses");
        let Some(card) = crate::ai_serving::project_codex_catalog_model_card(
            &snapshot.models,
            source_model.as_str(),
            row.global_model_name.as_str(),
        ) else {
            warn!(
                event_name = "codex_catalog_authorized_model_missing",
                provider_id = %row.provider_id,
                key_id = %row.key_id,
                client_version = %client_version.as_str(),
                source_model = %source_model,
                global_model = %row.global_model_name,
                "authorized Codex model was not present in this upstream catalog mapping"
            );
            continue;
        };
        seen_global_models.insert(row.global_model_name.clone());
        cards.push(card);
        if cards.len() > CODEX_MODELS_MAX_RESPONSE_MODELS {
            warn!(
                event_name = "codex_catalog_aggregate_model_limit",
                client_version = %client_version.as_str(),
                model_count = cards.len(),
                limit = CODEX_MODELS_MAX_RESPONSE_MODELS,
                "Codex projected catalog exceeded the aggregate model limit; returning an empty remote catalog"
            );
            return (Vec::new(), None);
        }
    }
    let missing_model_count = expected_global_models
        .difference(&seen_global_models)
        .count();
    if missing_model_count > 0 {
        warn!(
            event_name = "codex_catalog_authorized_models_incomplete",
            client_version = %client_version.as_str(),
            expected_model_count = expected_global_models.len(),
            projected_model_count = cards.len(),
            missing_model_count,
            "Codex upstream catalogs omitted authorized mappings; serving the available cards without fabricating missing model metadata"
        );
    }
    if !codex_projected_catalog_fits_response_limits(&cards) {
        warn!(
            event_name = "codex_catalog_aggregate_body_limit",
            client_version = %client_version.as_str(),
            model_count = cards.len(),
            limit_bytes = CODEX_MODELS_MAX_RESPONSE_JSON_BYTES,
            "Codex projected catalog exceeded the aggregate response body limit; returning an empty remote catalog"
        );
        return (Vec::new(), None);
    }
    if cards.is_empty() {
        return (cards, None);
    }
    let etag = if possible_inference_catalogs.len() == 1 {
        possible_inference_catalogs
            .iter()
            .next()
            .and_then(|(provider_id, key_id)| catalogs.snapshot(provider_id, key_id))
            .and_then(|snapshot| snapshot.etag.clone())
    } else {
        None
    };
    (cards, etag)
}

struct ModelRowsForClientFormat {
    rows: Vec<StoredMinimalCandidateSelectionRow>,
    codex_catalog_targets: Vec<crate::model_fetch::CodexCatalogTarget>,
}

fn build_openai_catalog_models_list_response(model_names: &[String]) -> Response<Body> {
    Json(json!({
        "object": "list",
        "data": model_names.iter().map(|model_name| {
            json!({
                "id": model_name,
                "object": "model",
                "created": 0,
                "owned_by": PUBLIC_MODELS_OWNER,
            })
        }).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// 读取可用于 Codex 动态目录投影的真实候选行，并保留具体 Endpoint/Key 绑定。
async fn list_codex_model_rows(
    state: &AppState,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<ModelRowsForClientFormat> {
    let mut collected = Vec::new();
    for query_format in CODEX_MODELS_QUERY_API_FORMATS {
        let rows = await_models_route_read(
            "candidate_selection_by_api_format",
            state.list_minimal_candidate_selection_rows_for_api_format(query_format),
        )
        .await?;
        let mut filtered = filter_eligible_model_rows(rows, auth_snapshot, query_format);
        collected.append(&mut filtered);
    }
    collected.retain(is_codex_provider_row);
    let codex_catalog_targets = crate::model_fetch::codex_catalog_targets(&collected);
    Some(ModelRowsForClientFormat {
        rows: sort_model_rows(collected),
        codex_catalog_targets,
    })
}

fn build_openai_catalog_model_detail_response(model_name: &str) -> Response<Body> {
    Json(json!({
        "id": model_name,
        "object": "model",
        "created": 0,
        "owned_by": PUBLIC_MODELS_OWNER,
    }))
    .into_response()
}

fn build_claude_catalog_models_list_response(
    model_names: &[String],
    before_id: Option<&str>,
    after_id: Option<&str>,
    limit: usize,
) -> Response<Body> {
    let model_data = model_names
        .iter()
        .map(|model_name| {
            json!({
                "id": model_name,
                "type": "model",
                "display_name": model_name,
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

fn build_gemini_catalog_models_list_response(
    model_names: &[String],
    count_tokens_models: &BTreeSet<String>,
    page_size: usize,
    page_token: Option<&str>,
) -> Response<Body> {
    let start_idx = page_token
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let end_idx = start_idx.saturating_add(page_size);
    let window = model_names
        .iter()
        .skip(start_idx)
        .take(page_size)
        .map(|model_name| {
            project_gemini_model_value(model_name, count_tokens_models.contains(model_name))
        })
        .collect::<Vec<_>>();
    let mut payload = json!({ "models": window });
    if end_idx < model_names.len() {
        payload["nextPageToken"] = Value::String(end_idx.to_string());
    }
    Json(payload).into_response()
}

fn build_gemini_catalog_model_detail_response(
    row: &StoredModelCatalogEntry,
    supports_count_tokens: bool,
) -> Response<Body> {
    Json(project_gemini_model_value(
        &row.global_model_name,
        supports_count_tokens,
    ))
    .into_response()
}

struct PublishedModelsList {
    model_names: Vec<String>,
    catalog_rows: Vec<StoredModelCatalogEntry>,
    gemini_count_tokens_models: BTreeSet<String>,
}

async fn load_gemini_count_tokens_models(
    state: &AppState,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<BTreeSet<String>> {
    let rows = await_models_route_read(
        "gemini_count_tokens_candidates",
        state.list_minimal_candidate_selection_rows_for_api_format("gemini:generate_content"),
    )
    .await?;
    Some(
        filter_eligible_model_rows(rows, auth_snapshot, "gemini:generate_content")
            .into_iter()
            .filter(|row| provider_type_supports_gemini_count_tokens(&row.provider_type))
            .filter(|row| {
                aether_scheduler_core::row_supports_requested_model_with_model_directives_and_request_operation(
                    row,
                    &row.global_model_name,
                    "gemini:generate_content",
                    false,
                    Some("count_tokens"),
                )
            })
            .map(|row| row.global_model_name)
            .collect(),
    )
}

async fn list_models_for_client_format(
    state: &AppState,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
) -> Option<PublishedModelsList> {
    let gemini_count_tokens_models =
        if crate::ai_serving::normalize_api_format_alias(api_format) == "gemini:generate_content" {
            load_gemini_count_tokens_models(state, auth_snapshot)
                .await
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
    // 标准 OpenAI 模型目录发布配置可见性，不把 `/v1/models` 误当作 Chat 调用能力校验。
    let model_family_filter =
        (!is_standard_openai_models_api_format(api_format)).then_some(api_format);
    if !state.has_global_model_data_reader() {
        let rows =
            await_models_route_read("model_catalog", state.data.list_model_catalog()).await?;
        let rows = filter_catalog_for_models(rows, auth_snapshot, model_family_filter);
        return Some(PublishedModelsList {
            model_names: rows
                .iter()
                .map(|row| row.global_model_name.clone())
                .collect(),
            gemini_count_tokens_models,
            catalog_rows: rows,
        });
    }

    let provider_restricted = auth_snapshot
        .and_then(crate::data::auth::GatewayAuthApiKeySnapshot::effective_allowed_providers)
        .is_some();
    let needs_catalog = provider_restricted
        || is_codex_models_api_format(api_format)
        || crate::ai_serving::normalize_api_format_alias(api_format) == "gemini:generate_content";
    let catalog_rows = if needs_catalog {
        let rows =
            await_models_route_read("model_catalog", state.data.list_model_catalog()).await?;
        filter_catalog_for_models(rows, auth_snapshot, model_family_filter)
    } else {
        Vec::new()
    };
    let allowed_global_model_ids = provider_restricted.then(|| {
        catalog_rows
            .iter()
            .map(|row| row.global_model_id.clone())
            .collect::<BTreeSet<_>>()
    });
    let page = await_models_route_read(
        "published_global_models",
        state.list_public_global_models(&PublicGlobalModelQuery {
            offset: 0,
            limit: PUBLIC_MODELS_FETCH_LIMIT,
            is_active: Some(true),
            search: None,
        }),
    )
    .await?;
    if page.items.is_empty() {
        let rows = if needs_catalog {
            catalog_rows
        } else {
            let rows =
                await_models_route_read("model_catalog", state.data.list_model_catalog()).await?;
            filter_catalog_for_models(rows, auth_snapshot, model_family_filter)
        };
        return Some(PublishedModelsList {
            model_names: rows
                .iter()
                .map(|row| row.global_model_name.clone())
                .collect(),
            gemini_count_tokens_models,
            catalog_rows: rows,
        });
    }
    let models = filter_global_models_for_models(
        page.items,
        auth_snapshot,
        model_family_filter,
        allowed_global_model_ids.as_ref(),
    );
    let visible_names = models
        .iter()
        .map(|model| model.name.clone())
        .collect::<BTreeSet<_>>();
    Some(PublishedModelsList {
        model_names: visible_names.iter().cloned().collect(),
        gemini_count_tokens_models,
        catalog_rows: catalog_rows
            .into_iter()
            .filter(|row| visible_names.contains(&row.global_model_name))
            .collect(),
    })
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

/// 仅保留当前格式和凭据可实际路由的模型目录行；无请求级策略时沿用系统默认排序。
async fn retain_routable_model_rows(
    state: &AppState,
    rows: Vec<StoredModelCatalogEntry>,
    api_format: &str,
    auth_snapshot: Option<&crate::data::auth::GatewayAuthApiKeySnapshot>,
    now_unix_secs: u64,
) -> Option<Vec<StoredModelCatalogEntry>> {
    let rows = filter_catalog_for_models(rows, auth_snapshot, Some(api_format));
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
                    None,
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
    if !auth_context.access_allowed || auth_context.local_rejection.is_some() {
        return Some(build_models_auth_error_response(api_format));
    }
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
    let Some(auth_snapshot) = auth_snapshot.as_ref() else {
        warn!(
            event_name = "models_route_auth_snapshot_missing",
            user_id = %auth_context.user_id,
            api_key_id = %auth_context.api_key_id,
            "gateway models route rejected a request whose authenticated API key snapshot disappeared"
        );
        return Some(build_models_auth_error_response(api_format));
    };
    if !auth_snapshot.currently_usable {
        return Some(build_models_auth_error_response(api_format));
    }
    let auth_snapshot = Some(auth_snapshot);

    match decision.route_kind.as_deref() {
        Some("list") => {
            if is_codex_models_api_format(api_format) {
                let listed = match list_codex_model_rows(state, auth_snapshot).await {
                    Some(rows) => rows,
                    None => {
                        return Some(build_models_read_fallback_response(
                            request_context,
                            api_format,
                        ))
                    }
                };
                let rows = listed.rows;
                if rows.is_empty() {
                    return Some(build_empty_models_list_response(api_format));
                }
                let raw_client_version = query_param_value(
                    request_context.request_query_string.as_deref(),
                    "client_version",
                );
                let client_version = crate::model_fetch::normalize_codex_client_version(
                    raw_client_version.as_deref(),
                );
                if client_version.used_fallback() {
                    warn!(
                        event_name = "codex_catalog_invalid_client_version",
                        raw_length = raw_client_version.as_ref().map_or(0, String::len),
                        fallback_version = %client_version.as_str(),
                        "invalid Codex client_version used the bounded fallback version"
                    );
                }
                let (models, etag) = load_codex_model_cards(
                    state,
                    &rows,
                    &listed.codex_catalog_targets,
                    client_version,
                )
                .await;
                return Some(build_codex_models_list_response(models, etag.as_deref()));
            }
            let published =
                match list_models_for_client_format(state, api_format, auth_snapshot).await {
                    Some(published) => published,
                    None => {
                        return Some(build_models_read_fallback_response(
                            request_context,
                            api_format,
                        ))
                    }
                };
            if published.model_names.is_empty() {
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
                    build_claude_catalog_models_list_response(
                        &published.model_names,
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
                        &published.model_names,
                        &published.gemini_count_tokens_models,
                        page_size,
                        page_token.as_deref(),
                    )
                }
                _ => build_openai_catalog_models_list_response(&published.model_names),
            };
            Some(response)
        }
        Some("detail") => {
            let model_id = models_detail_id(&request_context.request_path)?;
            if is_standard_openai_models_api_format(api_format) {
                let published =
                    match list_models_for_client_format(state, api_format, auth_snapshot).await {
                        Some(published) => published,
                        None => {
                            return Some(build_models_read_fallback_response(
                                request_context,
                                api_format,
                            ))
                        }
                    };
                if !published.model_names.iter().any(|name| name == &model_id) {
                    return Some(build_models_not_found_response(&model_id, api_format));
                }
                return Some(build_openai_catalog_model_detail_response(&model_id));
            }
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
                "gemini:generate_content" => {
                    let supports_count_tokens =
                        load_gemini_count_tokens_models(state, auth_snapshot)
                            .await
                            .unwrap_or_default()
                            .contains(&model_id);
                    build_gemini_catalog_model_detail_response(row, supports_count_tokens)
                }
                _ => build_openai_catalog_model_detail_response(&row.global_model_name),
            };
            Some(response)
        }
        _ => Some(build_models_auth_error_response(api_format)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        codex_projected_catalog_fits_response_limits, CODEX_MODELS_MAX_RESPONSE_JSON_BYTES,
        CODEX_MODELS_MAX_RESPONSE_MODELS,
    };

    #[test]
    fn projected_codex_catalog_enforces_aggregate_count_and_body_limits() {
        assert!(codex_projected_catalog_fits_response_limits(&[json!({
            "slug": "gpt-future-dynamic",
            "model_messages": {"instructions_template": "opaque"}
        })]));

        let too_many = (0..=CODEX_MODELS_MAX_RESPONSE_MODELS)
            .map(|index| json!({"slug": format!("gpt-future-{index}")}))
            .collect::<Vec<_>>();
        assert!(!codex_projected_catalog_fits_response_limits(&too_many));

        let oversized = vec![json!({
            "slug": "gpt-future-oversized",
            "future_capability": "x".repeat(CODEX_MODELS_MAX_RESPONSE_JSON_BYTES)
        })];
        assert!(!codex_projected_catalog_fits_response_limits(&oversized));
    }
}
