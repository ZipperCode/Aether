use crate::ai_serving::build_provider_key_pool_score_upsert;
use crate::handlers::admin::admin_provider_pool_config;
use crate::handlers::admin::provider::shared::paths::admin_update_key_id;
use crate::handlers::admin::provider::shared::payloads::AdminProviderKeyUpdatePatch;
use crate::handlers::admin::provider::write::keys::{
    admin_provider_key_update_requires_immediate_model_fetch,
    build_provider_catalog_key_admin_cas_update,
};
use crate::handlers::admin::request::{AdminAppState, AdminRequestContext};
use crate::handlers::admin::shared::{
    antigravity_authoritative_model_map, antigravity_model_entry_is_disabled,
};
use crate::maintenance::ensure_provider_key_pool_scores_for_keys;
use crate::provider_key_auth::provider_key_effective_api_formats;
use crate::{model_fetch::perform_model_fetch_for_key, GatewayError};
use aether_admin::provider::redaction::admin_safe_antigravity_quota_group_key;
use aether_data_contracts::repository::pool_scores::PoolMemberScoreUpsertMode;
use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeyRuntimeMetadataUpdate, StoredProviderCatalogKey,
};
use aether_provider_pool::antigravity_quota_group_key;
use axum::{
    body::{Body, Bytes},
    http,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) async fn maybe_handle(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
    request_body: Option<&Bytes>,
) -> Result<Option<Response<Body>>, GatewayError> {
    let Some(decision) = request_context.decision() else {
        return Ok(None);
    };
    if decision.route_family.as_deref() != Some("endpoints_manage")
        || decision.route_kind.as_deref() != Some("update_key")
        || request_context.method() != http::Method::PUT
        || !request_context
            .path()
            .starts_with("/api/admin/endpoints/keys/")
    {
        return Ok(None);
    }

    let Some(key_id) = admin_update_key_id(request_context.path()) else {
        return Ok(Some(not_found_response("Key 不存在")));
    };
    let Some(request_body) = request_body else {
        return Ok(Some(bad_request_response("请求体不能为空")));
    };
    if !state.has_provider_catalog_data_reader() {
        return Ok(None);
    }

    let raw_value = match serde_json::from_slice::<serde_json::Value>(request_body) {
        Ok(value) => value,
        Err(_) => return Ok(Some(bad_request_response("请求体必须是合法的 JSON 对象"))),
    };
    let Some(raw_payload) = raw_value.as_object().cloned() else {
        return Ok(Some(bad_request_response("请求体必须是合法的 JSON 对象")));
    };
    if raw_payload.contains_key("antigravity_quota_bindings") {
        return handle_update_antigravity_quota_bindings(state, &key_id, &raw_payload)
            .await
            .map(Some);
    }
    let patch = match AdminProviderKeyUpdatePatch::from_object(raw_payload) {
        Ok(patch) => patch,
        Err(_) => return Ok(Some(bad_request_response("请求体必须是合法的 JSON 对象"))),
    };

    let Some(existing_key) = state
        .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(Some(not_found_response(format!("Key {key_id} 不存在"))));
    };
    let Some(provider) = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&existing_key.provider_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(Some(not_found_response(format!(
            "Provider {} 不存在",
            existing_key.provider_id
        ))));
    };

    let updated_record = match state
        .build_admin_update_provider_key_record(&provider, &existing_key, patch)
        .await
    {
        Ok(record) => record,
        Err(detail) => return Ok(Some(bad_request_response(detail))),
    };
    let admin_update = build_provider_catalog_key_admin_cas_update(
        &existing_key,
        updated_record.clone(),
        &provider.provider_type,
    );
    if !state
        .compare_and_update_provider_catalog_key_admin_state(&admin_update)
        .await?
    {
        return Ok(Some(conflict_response(
            "Key 凭据或配置已被其他请求更新，请刷新后重试",
        )));
    }
    let Some(mut updated) = state
        .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    if updated_record.learned_rpm_limit != existing_key.learned_rpm_limit {
        let Some(reloaded) = state
            .set_provider_catalog_key_learned_rpm_limit(
                &key_id,
                updated_record.learned_rpm_limit,
                updated_record.updated_at_unix_secs,
            )
            .await?
        else {
            return Ok(None);
        };
        updated = reloaded;
    }
    let should_overwrite_allowed_models_immediately =
        admin_provider_key_update_requires_immediate_model_fetch(&existing_key, &updated);
    let updated = if should_overwrite_allowed_models_immediately {
        let summary =
            perform_model_fetch_for_key(state.as_ref(), &provider.id, &updated.id).await?;
        if summary.succeeded == 0 {
            let detail = state
                .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
                .await?
                .into_iter()
                .next()
                .and_then(|key| key.last_models_fetch_error)
                .unwrap_or_else(|| "未获取到可用上游模型".to_string());
            return Err(GatewayError::Internal(format!(
                "开启自动获取模型后同步上游模型失败: {detail}"
            )));
        }

        state
            .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
            .await?
            .into_iter()
            .next()
            .unwrap_or(updated)
    } else {
        updated
    };
    let now_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let endpoints = state
        .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(&provider.id))
        .await?;
    if let Some(pool_config) = admin_provider_pool_config(&provider) {
        if provider_key_update_requires_pool_score_recovery(&existing_key, &updated) {
            let projection_lock = state
                .as_ref()
                .acquire_provider_key_quota_projection_lock(&provider.id, &updated.id)
                .await?;
            let recovery_result = async {
                let current = state
                    .as_ref()
                    .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&updated.id))
                    .await?
                    .into_iter()
                    .next()
                    .filter(|key| key.is_active && key.provider_id == provider.id);
                let Some(current) = current else {
                    return Ok::<(), GatewayError>(());
                };
                let upsert = build_provider_key_pool_score_upsert(
                    &current,
                    provider.provider_type.as_str(),
                    None,
                    now_unix_secs,
                    pool_config.score_rules,
                );
                state
                    .as_ref()
                    .data
                    .upsert_pool_member_score_with_mode(
                        upsert,
                        PoolMemberScoreUpsertMode::OAuthRecovery,
                    )
                    .await
                    .map_err(|err| {
                        GatewayError::Internal(format!(
                            "failed to recover pool score for updated provider key '{}': {err}",
                            updated.id
                        ))
                    })?;
                Ok(())
            }
            .await;
            state
                .as_ref()
                .release_provider_key_quota_projection_lock(projection_lock)
                .await;
            recovery_result?;
        } else {
            let score_ensure_budget =
                (pool_config.score_fallback_scan_limit as usize).clamp(1, 50_000);
            if let Err(err) = ensure_provider_key_pool_scores_for_keys(
                state.as_ref(),
                &provider,
                &pool_config,
                &endpoints,
                std::slice::from_ref(&updated),
                now_unix_secs,
                score_ensure_budget,
            )
            .await
            {
                tracing::debug!(
                    provider_id = %provider.id,
                    key_id = %updated.id,
                    error = ?err,
                    "gateway admin provider key update: failed to seed pool score rows"
                );
            }
        }
    }
    let api_formats =
        provider_key_effective_api_formats(&updated, &provider.provider_type, &endpoints);

    Ok(Some(
        Json(state.build_admin_provider_key_response(
            &updated,
            &provider.provider_type,
            &api_formats,
            now_unix_secs,
        ))
        .into_response(),
    ))
}

fn provider_key_update_requires_pool_score_recovery(
    existing: &aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey,
    updated: &aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey,
) -> bool {
    if !updated.is_active || !provider_key_has_credential(updated) {
        return false;
    }

    !existing
        .auth_type
        .trim()
        .eq_ignore_ascii_case(updated.auth_type.trim())
        || existing.encrypted_api_key != updated.encrypted_api_key
        || existing.encrypted_auth_config != updated.encrypted_auth_config
}

fn provider_key_has_credential(
    key: &aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey,
) -> bool {
    let has_api_key = key
        .encrypted_api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_auth_config = key
        .encrypted_auth_config
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());

    match key.auth_type.trim().to_ascii_lowercase().as_str() {
        "api_key" | "bearer" => has_api_key,
        "service_account" | "vertex_ai" => has_auth_config,
        "oauth" => has_api_key || has_auth_config,
        _ => has_api_key || has_auth_config,
    }
}

fn bad_request_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::BAD_REQUEST,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}

fn not_found_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::NOT_FOUND,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}

fn conflict_response(detail: impl Into<String>) -> Response<Body> {
    (
        http::StatusCode::CONFLICT,
        Json(json!({ "detail": detail.into() })),
    )
        .into_response()
}

async fn handle_update_antigravity_quota_bindings(
    state: &AdminAppState<'_>,
    key_id: &str,
    raw_payload: &Map<String, Value>,
) -> Result<Response<Body>, GatewayError> {
    if raw_payload.len() != 1 {
        return Ok(bad_request_response(
            "antigravity_quota_bindings 不能与其他 Key 配置字段混用",
        ));
    }
    let Some(raw_bindings) = raw_payload
        .get("antigravity_quota_bindings")
        .and_then(Value::as_object)
    else {
        return Ok(bad_request_response(
            "antigravity_quota_bindings 必须是模型到配额组键的 JSON 对象",
        ));
    };
    let Some(existing_key) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
        .await?
        .into_iter()
        .next()
    else {
        return Ok(not_found_response(format!("Key {key_id} 不存在")));
    };
    let Some(provider) = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&existing_key.provider_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(not_found_response(format!(
            "Provider {} 不存在",
            existing_key.provider_id
        )));
    };

    let normalized_bindings = match validate_antigravity_quota_bindings(
        &provider.provider_type,
        &existing_key,
        raw_bindings,
    ) {
        Ok(bindings) => bindings,
        Err(detail) => return Ok(bad_request_response(detail)),
    };
    let expected_bindings = existing_key
        .upstream_metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity_quota_bindings"))
        .cloned();
    let now_unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let updated = state
        .app()
        .update_provider_catalog_key_runtime_metadata(&ProviderCatalogKeyRuntimeMetadataUpdate {
            key_id: existing_key.id.clone(),
            namespace: "antigravity_quota_bindings".to_string(),
            expected_upstream_metadata_value: expected_bindings,
            upstream_metadata_value: Value::Object(normalized_bindings),
            status_snapshot_patch: json!({}),
            updated_at_unix_secs: Some(now_unix_secs),
        })
        .await?;
    if !updated {
        return Ok(conflict_response(
            "Key 配额组绑定已被其他请求更新，请刷新后重试",
        ));
    }
    state.app().invalidate_provider_quota_candidate_caches();

    let Some(persisted) = state
        .app()
        .list_provider_catalog_keys_by_ids_strong(&[key_id.to_string()])
        .await?
        .into_iter()
        .next()
    else {
        return Ok(not_found_response(format!("Key {key_id} 不存在")));
    };
    let endpoints = state
        .list_provider_catalog_endpoints_by_provider_ids(std::slice::from_ref(&provider.id))
        .await?;
    let api_formats =
        provider_key_effective_api_formats(&persisted, &provider.provider_type, &endpoints);

    Ok(Json(state.build_admin_provider_key_response(
        &persisted,
        &provider.provider_type,
        &api_formats,
        now_unix_secs,
    ))
    .into_response())
}

/// 校验账号级 Antigravity 模型到配额组的显式绑定：
/// 1. 仅支持 `antigravity` Provider；空对象 `{}` 清空绑定。
/// 2. 模型必须精确存在于当前账号权威模型映射（首个 `quota_by_model`/`models`/`model_quotas` 对象）且未禁用；
///    按 trim + ASCII 大小写不敏感拒绝重复绑定，落库键使用账号权威模型名。
/// 3. 配额组键必须精确存在于当前账号已保存的 `quota_groups`（`group.quota_group` 或 `antigravity_quota_group_key`）。
fn validate_antigravity_quota_bindings(
    provider_type: &str,
    key: &StoredProviderCatalogKey,
    raw_bindings: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    if !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return Err("仅 Antigravity Provider 支持 antigravity_quota_bindings".to_string());
    }
    if raw_bindings.is_empty() {
        return Ok(Map::new());
    }

    let Some(antigravity_bucket) = key
        .upstream_metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return Err("当前账号缺少 Antigravity 模型与配额组元数据，请先刷新配额".to_string());
    };
    let Some(authoritative_models) =
        antigravity_authoritative_model_map(key.upstream_metadata.as_ref())
    else {
        return Err("当前账号缺少已发现模型映射，请先刷新配额或同步模型".to_string());
    };
    let valid_group_keys = antigravity_bucket
        .get("quota_groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|group| {
            group
                .get("quota_group")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| antigravity_quota_group_key(group))
        })
        .collect::<BTreeSet<_>>();
    if valid_group_keys.is_empty() {
        return Err("当前账号缺少可绑定的配额组，请先刷新配额".to_string());
    }

    let mut seen_normalized_models = BTreeSet::new();
    let mut normalized_bindings = Map::new();
    for (raw_model, raw_group) in raw_bindings {
        let requested_model = raw_model.trim();
        if requested_model.is_empty() || requested_model.starts_with("RateLimitResetCredit_") {
            return Err("绑定的模型名称不能为空或内部额度标识".to_string());
        }
        let Some(requested_group) = raw_group
            .as_str()
            .and_then(admin_safe_antigravity_quota_group_key)
        else {
            return Err(format!("模型 {requested_model} 的配额组键无效"));
        };
        if !seen_normalized_models.insert(requested_model.to_ascii_lowercase()) {
            return Err(format!("模型 {requested_model} 存在重复的配额组绑定"));
        }
        let Some((canonical_model, _)) =
            authoritative_models.iter().find(|(entry_model, entry)| {
                let entry_model = entry_model.trim();
                !entry_model.is_empty()
                    && entry_model.eq_ignore_ascii_case(requested_model)
                    && !antigravity_model_entry_is_disabled(entry)
            })
        else {
            return Err(format!(
                "模型 {requested_model} 不属于当前账号的已发现可用模型"
            ));
        };
        if !valid_group_keys.contains(requested_group) {
            return Err(format!(
                "配额组 {requested_group} 不存在于当前账号的已保存配额组"
            ));
        }
        normalized_bindings.insert(
            canonical_model.trim().to_string(),
            Value::String(requested_group.to_string()),
        );
    }

    Ok(normalized_bindings)
}

#[cfg(test)]
mod tests {
    use super::validate_antigravity_quota_bindings;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
    use serde_json::json;

    fn sample_antigravity_key() -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            "key-ag".to_string(),
            "provider-ag".to_string(),
            "ag-account".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-high": {
                        "display_name": "Gemini 4 Argon (High)",
                        "remaining_fraction": 0.8
                    },
                    "gemini-3.8-flash-medium": {
                        "display_name": "Gemini 3.8 Flash (Medium)"
                    },
                    "gemini-4-argon-disabled": {
                        "display_name": "Disabled Argon",
                        "disabled": true
                    }
                },
                "models": {
                    "legacy-resurrected-model": {
                        "remaining_fraction": 1.0
                    }
                },
                "quota_groups": [
                    {
                        "group_id": "opaque-pool",
                        "display_name": "Named ID group",
                        "buckets": [
                            {"bucket_id": "opaque-short", "window": "5h"}
                        ]
                    },
                    {
                        "display_name": "Bucket set group",
                        "buckets": [
                            {"bucket_id": "3p-weekly", "window": "weekly"},
                            {"bucket_id": "3p-5h", "window": "5h"}
                        ]
                    }
                ]
            }
        }));
        key
    }

    #[test]
    fn validates_and_normalizes_antigravity_quota_bindings_and_rejects_invalid_inputs() {
        let key = sample_antigravity_key();

        let valid = validate_antigravity_quota_bindings(
            "antigravity",
            &key,
            json!({
                "  Gemini-4-Argon-High ": "id:opaque-pool",
                "gemini-3.8-flash-medium": "buckets:[\"3p-5h\",\"3p-weekly\"]"
            })
            .as_object()
            .unwrap(),
        )
        .expect("discovered models and exact group keys should validate");
        assert_eq!(
            serde_json::Value::Object(valid),
            json!({
                "gemini-4-argon-high": "id:opaque-pool",
                "gemini-3.8-flash-medium": "buckets:[\"3p-5h\",\"3p-weekly\"]"
            })
        );

        let cleared = validate_antigravity_quota_bindings(
            "antigravity",
            &key,
            json!({}).as_object().unwrap(),
        )
        .expect("empty object should clear bindings");
        assert!(cleared.is_empty());

        for (provider_type, payload) in [
            ("openai", json!({})),
            (
                "antigravity",
                json!({"undiscovered-model": "id:opaque-pool"}),
            ),
            (
                "antigravity",
                json!({"legacy-resurrected-model": "id:opaque-pool"}),
            ),
            (
                "antigravity",
                json!({"gemini-4-argon-disabled": "id:opaque-pool"}),
            ),
            (
                "antigravity",
                json!({"gemini-4-argon-high": "id:unknown-group"}),
            ),
            (
                "antigravity",
                json!({
                    "gemini-4-argon-high": "id:opaque-pool",
                    " GEMINI-4-ARGON-HIGH ": "id:opaque-pool"
                }),
            ),
        ] {
            assert!(
                validate_antigravity_quota_bindings(
                    provider_type,
                    &key,
                    payload.as_object().unwrap(),
                )
                .is_err(),
                "invalid binding must fail: provider={provider_type} payload={payload}"
            );
        }
    }
}
