use super::read_keys::admin_pool_key_visible_status_filter;
use super::{
    admin_pool_provider_id_from_path, admin_provider_pool_config, build_admin_pool_error_response,
    parse_admin_pool_status_value, pool_selection, read_admin_provider_pool_runtime_state,
    resolve_admin_pool_selection_snapshot_key_ids, AdminPoolResolveSelectionRequest,
    AdminPoolSelectionSnapshotReference, AdminProviderPoolRuntimeState,
    ADMIN_POOL_PROVIDER_CATALOG_READER_UNAVAILABLE_DETAIL,
};
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::provider_key_auth::{
    provider_key_auth_config_is_agent_identity, provider_key_auth_config_uses_header_authorization,
    provider_key_auth_semantics, provider_key_can_export_oauth, provider_key_can_refresh_oauth,
};
use crate::GatewayError;
use aether_admin::provider::pool as admin_provider_pool_pure;
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::BTreeMap;

const ADMIN_POOL_SELECTION_KEY_FETCH_CHUNK_SIZE: usize = 500;

fn build_admin_pool_resolved_selection_item(
    state: &AdminAppState<'_>,
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> serde_json::Value {
    let auth_semantics = provider_key_auth_semantics(key, provider_type);
    let auth_config = state.parse_catalog_auth_config_json(key);
    json!({
        "key_id": key.id,
        "key_name": key.name,
        "auth_type": key.auth_type,
        "auth_type_by_format": key.auth_type_by_format,
        "credential_kind": auth_semantics.credential_kind().as_str(),
        "runtime_auth_kind": auth_semantics.runtime_auth_kind().as_str(),
        "oauth_managed": auth_semantics.oauth_managed(),
        "agent_identity": provider_key_auth_config_is_agent_identity(
            provider_type,
            auth_config.as_ref(),
        ),
        "can_refresh_oauth": provider_key_can_refresh_oauth(
            auth_semantics,
            provider_type,
            auth_config.as_ref(),
        ),
        "can_export_oauth": provider_key_can_export_oauth(
            auth_semantics,
            provider_type,
            auth_config.as_ref(),
        ),
        "can_edit_oauth": auth_semantics.can_edit_oauth(),
        "oauth_header_auth": auth_semantics.oauth_managed()
            && provider_key_auth_config_uses_header_authorization(auth_config.as_ref()),
    })
}

pub(super) async fn build_admin_pool_resolve_selection_response(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    request_body: Option<&Bytes>,
) -> Result<Response<Body>, GatewayError> {
    if !state.has_provider_catalog_data_reader() {
        return Ok(build_admin_pool_error_response(
            http::StatusCode::SERVICE_UNAVAILABLE,
            ADMIN_POOL_PROVIDER_CATALOG_READER_UNAVAILABLE_DETAIL,
        ));
    }

    let Some(provider_id) = admin_pool_provider_id_from_path(request_context.path()) else {
        return Ok(build_admin_pool_error_response(
            http::StatusCode::BAD_REQUEST,
            "provider_id 无效",
        ));
    };

    let payload = match request_body {
        None => AdminPoolResolveSelectionRequest::default(),
        Some(body) if body.is_empty() => AdminPoolResolveSelectionRequest::default(),
        Some(body) => match serde_json::from_slice::<AdminPoolResolveSelectionRequest>(body) {
            Ok(value) => value,
            Err(_) => {
                return Ok(build_admin_pool_error_response(
                    http::StatusCode::BAD_REQUEST,
                    "Invalid JSON request body",
                ));
            }
        },
    };
    let Some(provider) = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&provider_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(build_admin_pool_error_response(
            http::StatusCode::NOT_FOUND,
            format!("Provider {provider_id} 不存在"),
        ));
    };

    let provider_type = provider.provider_type.clone();
    if let Some(snapshot_id) = payload
        .snapshot_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some(expected_total) = payload.expected_total else {
            return Ok(build_admin_pool_error_response(
                http::StatusCode::BAD_REQUEST,
                "selection.expected_total is required",
            ));
        };
        let reference = AdminPoolSelectionSnapshotReference {
            snapshot_id: snapshot_id.to_string(),
            expected_total,
        };
        let key_ids = match resolve_admin_pool_selection_snapshot_key_ids(
            state,
            request_context,
            &provider.id,
            &reference,
        )
        .await?
        {
            Ok(key_ids) => key_ids,
            Err(response) => return Ok(response),
        };
        let mut keys_by_id = BTreeMap::new();
        for chunk in key_ids.chunks(ADMIN_POOL_SELECTION_KEY_FETCH_CHUNK_SIZE) {
            for key in state
                .list_provider_catalog_keys_by_ids(chunk)
                .await?
                .into_iter()
                .filter(|key| key.provider_id == provider.id)
            {
                keys_by_id.insert(key.id.clone(), key);
            }
        }
        let items = key_ids
            .iter()
            .filter_map(|key_id| keys_by_id.get(key_id))
            .map(|key| build_admin_pool_resolved_selection_item(state, key, &provider_type))
            .collect::<Vec<_>>();
        let snapshot_total = key_ids.len();
        let missing_count = snapshot_total.saturating_sub(items.len());

        return Ok(Json(json!({
            "total": items.len(),
            "snapshot_total": snapshot_total,
            "missing_count": missing_count,
            "items": items,
        }))
        .into_response());
    }

    let status = match parse_admin_pool_status_value(payload.status.as_deref()) {
        Ok(value) => value,
        Err(detail) => {
            return Ok(build_admin_pool_error_response(
                http::StatusCode::BAD_REQUEST,
                detail,
            ));
        }
    };
    let search = payload.search.trim();
    let quick_selectors =
        admin_provider_pool_pure::admin_pool_sanitize_quick_selectors(payload.quick_selectors);
    let use_full_search = status != "all" || !quick_selectors.is_empty();

    let mut keys = state
        .list_provider_catalog_keys_by_provider_ids(std::slice::from_ref(&provider.id))
        .await?
        .into_iter()
        .filter(|key| {
            if use_full_search {
                pool_selection::admin_pool_matches_search(state, key, &provider_type, Some(search))
            } else {
                pool_selection::admin_pool_matches_catalog_search(key, Some(search))
            }
        })
        .filter(|key| {
            quick_selectors.is_empty()
                || quick_selectors.iter().all(|selector| {
                    pool_selection::admin_pool_matches_quick_selector(
                        state,
                        key,
                        &provider_type,
                        selector,
                    )
                })
        })
        .collect::<Vec<_>>();

    if status != "all" {
        let pool_config = admin_provider_pool_config(&provider);
        let key_ids = keys.iter().map(|key| key.id.clone()).collect::<Vec<_>>();
        let runtime = match pool_config.as_ref() {
            Some(pool_config) if !key_ids.is_empty() => {
                read_admin_provider_pool_runtime_state(
                    state.runtime_state(),
                    &provider.id,
                    &key_ids,
                    pool_config,
                    None,
                )
                .await
            }
            _ => AdminProviderPoolRuntimeState::default(),
        };
        let now_unix_secs = admin_provider_pool_pure::admin_pool_now_unix_secs();
        keys.retain(|key| {
            admin_pool_key_visible_status_filter(
                state,
                key,
                &provider_type,
                pool_config.as_ref(),
                &runtime,
                now_unix_secs,
            ) == status
        });
    }

    keys.sort_by(|left, right| {
        left.internal_priority
            .cmp(&right.internal_priority)
            .then_with(|| left.name.cmp(&right.name))
    });

    let items = keys
        .iter()
        .map(|key| build_admin_pool_resolved_selection_item(state, key, &provider_type))
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "total": items.len(),
        "items": items,
    }))
    .into_response())
}
