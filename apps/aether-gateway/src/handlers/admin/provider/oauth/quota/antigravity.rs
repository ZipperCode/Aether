use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload, coerce_json_f64,
    coerce_json_string, execute_provider_quota_plan, extract_execution_error_message,
    oauth_refresh_auto_removed_result, persist_provider_quota_refresh_state,
    quota_key_auto_removed, quota_refresh_success_invalid_state,
    resolve_provider_quota_execution_timeouts, ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::provider::shared::payloads::{
    AdminImportProviderModelSource, AdminImportProviderModelsRequest,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::GatewayError;
use aether_admin::provider::quota::parse_antigravity_usage_response;
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::build_antigravity_pool_quota_request;
use serde_json::json;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

/// 从本地 `models` 或上游 `quota_by_model` metadata 提取可路由模型 ID，并复用统一黑名单。
fn antigravity_discovered_model_ids(metadata_update: Option<&serde_json::Value>) -> Vec<String> {
    metadata_update
        .and_then(|value| value.get("antigravity"))
        .and_then(|antigravity| {
            antigravity
                .get("models")
                .and_then(serde_json::Value::as_object)
                .or_else(|| {
                    antigravity
                        .get("quota_by_model")
                        .and_then(serde_json::Value::as_object)
                })
        })
        .into_iter()
        .flat_map(|models| models.keys())
        .map(String::as_str)
        .filter(|model_id| aether_model_fetch::antigravity_model_id_is_routable(model_id))
        .map(ToOwned::to_owned)
        .collect()
}

/// 把成功额度刷新发现的模型导入目录，并把每个模型精确绑定到本次请求的 Endpoint。
async fn sync_antigravity_discovered_models(
    state: &AdminAppState<'_>,
    provider_id: &str,
    endpoint: &StoredProviderCatalogEndpoint,
    metadata_update: Option<&serde_json::Value>,
) {
    if !state.has_global_model_data_reader() || !state.has_global_model_data_writer() {
        return;
    }
    let models = antigravity_discovered_model_ids(metadata_update)
        .into_iter()
        .map(|id| AdminImportProviderModelSource {
            id,
            api_formats: vec![endpoint.api_format.clone()],
            endpoint_ids: vec![endpoint.id.clone()],
        })
        .collect::<Vec<_>>();
    if models.is_empty() {
        return;
    }

    let result = state
        .build_admin_import_provider_models_payload(
            provider_id,
            AdminImportProviderModelsRequest {
                model_ids: Vec::new(),
                models,
                tiered_pricing: None,
                price_per_request: None,
            },
        )
        .await;
    match result {
        Ok(payload) => {
            let errors = payload
                .get("errors")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if errors > 0 {
                warn!(
                    provider_id,
                    endpoint_id = %endpoint.id,
                    errors,
                    "Antigravity discovered-model catalog sync completed with item errors"
                );
            }
        }
        Err(error) => warn!(
            provider_id,
            endpoint_id = %endpoint.id,
            error = %error,
            "Antigravity discovered-model catalog sync failed"
        ),
    }
}

async fn execute_antigravity_quota_plan(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: (String, String),
    project_id: &str,
    identity_headers: BTreeMap<String, String>,
    proxy_override: Option<&ProxySnapshot>,
) -> Result<ProviderQuotaExecutionOutcome, GatewayError> {
    let proxy = match proxy_override {
        Some(proxy) => Some(proxy.clone()),
        None => {
            state
                .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
                .await
        }
    };
    let timeouts = Some(resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        proxy.as_ref(),
    ));
    let spec = build_antigravity_pool_quota_request(
        &transport.key.id,
        &transport.endpoint.base_url,
        authorization,
        project_id,
        identity_headers,
    );
    let plan = build_provider_quota_execution_plan(
        transport,
        spec,
        proxy,
        state.resolve_transport_profile(transport),
        timeouts,
    );

    execute_provider_quota_plan(state, transport, plan, "antigravity").await
}

/// 刷新 Antigravity Key 额度；状态成功持久化后，再以当前 Endpoint 证据非阻塞同步模型目录。
pub(crate) async fn refresh_antigravity_provider_quota_locally(
    state: &AdminAppState<'_>,
    provider: &StoredProviderCatalogProvider,
    endpoint: &StoredProviderCatalogEndpoint,
    keys: Vec<StoredProviderCatalogKey>,
    proxy_override: Option<ProxySnapshot>,
) -> Result<Option<serde_json::Value>, GatewayError> {
    let mut results = Vec::new();
    let mut success_count = 0usize;
    let mut failed_count = 0usize;
    let mut auto_removed_count = 0usize;

    for key in keys {
        let mut transport = match state
            .read_provider_transport_snapshot(&provider.id, &endpoint.id, &key.id)
            .await?
        {
            Some(transport) => transport,
            None => {
                failed_count += 1;
                results.push(json!({
                    "key_id": key.id,
                    "key_name": key.name,
                    "status": "error",
                    "message": "Provider transport snapshot unavailable",
                }));
                continue;
            }
        };

        let authorization = match state.resolve_local_oauth_header_auth(&transport).await? {
            Some(auth) => auth,
            _ => {
                if quota_key_auto_removed(state, &key.id).await? {
                    auto_removed_count += 1;
                    results.push(oauth_refresh_auto_removed_result(&key));
                    continue;
                }
                failed_count += 1;
                results.push(json!({
                    "key_id": key.id,
                    "key_name": key.name,
                    "status": "error",
                    "message": "缺少 OAuth 认证信息，请先授权/刷新 Token",
                }));
                continue;
            }
        };

        let identity = match state.resolve_local_antigravity_identity_headers(&transport) {
            Some(identity) => Some(identity),
            None => state
                .app()
                .hydrate_antigravity_project_metadata_for_transport(&transport)
                .await
                .and_then(|hydrated| {
                    let identity = state.resolve_local_antigravity_identity_headers(&hydrated);
                    transport = hydrated;
                    identity
                }),
        };
        let Some((project_id, identity_headers)) = identity else {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "缺少 Antigravity project_id，loadCodeAssist 未返回可用项目信息",
            }));
            continue;
        };

        let result = match execute_antigravity_quota_plan(
            state,
            &transport,
            authorization,
            &project_id,
            identity_headers,
            proxy_override.as_ref(),
        )
        .await?
        {
            ProviderQuotaExecutionOutcome::Response(result) => result,
            ProviderQuotaExecutionOutcome::Failure(detail) => {
                failed_count += 1;
                results.push(json!({
                    "key_id": key.id,
                    "key_name": key.name,
                    "status": "error",
                    "message": format!("fetchAvailableModels 请求执行失败: {detail}"),
                    "status_code": 502,
                }));
                continue;
            }
        };

        let now_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let mut metadata_update = None::<serde_json::Value>;
        let (mut oauth_invalid_at_unix_secs, mut oauth_invalid_reason) =
            quota_refresh_success_invalid_state(&key);
        let mut status = "error".to_string();
        let mut message = None::<String>;

        if result.status_code == 200 {
            if let Some(body_json) = result
                .body
                .as_ref()
                .and_then(|body| body.json_body.as_ref())
            {
                metadata_update = parse_antigravity_usage_response(body_json, now_unix_secs)
                    .map(|metadata| json!({ "antigravity": metadata }));
                if metadata_update.is_some() {
                    status = "success".to_string();
                } else {
                    status = "no_metadata".to_string();
                    message = Some("响应中未包含配额信息".to_string());
                }
            } else {
                status = "no_metadata".to_string();
                message = Some("响应中未包含配额信息".to_string());
            }
        } else {
            let err_msg = extract_execution_error_message(&result);
            message = Some(match err_msg.as_deref() {
                Some(detail) if !detail.is_empty() => {
                    format!(
                        "fetchAvailableModels 返回状态码 {}: {}",
                        result.status_code, detail
                    )
                }
                _ => format!("fetchAvailableModels 返回状态码 {}", result.status_code),
            });
            if result.status_code == 403 {
                let reason = err_msg
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "账户访问被禁止".to_string());
                oauth_invalid_at_unix_secs = Some(now_unix_secs);
                oauth_invalid_reason = Some(format!("账户访问被禁止: {reason}"));
                metadata_update = Some(json!({
                    "antigravity": {
                        "is_forbidden": true,
                        "forbidden_reason": reason,
                        "forbidden_at": now_unix_secs,
                        "updated_at": now_unix_secs,
                    }
                }));
                status = "forbidden".to_string();
            }
        }

        if !persist_provider_quota_refresh_state(
            state,
            &key.id,
            metadata_update.as_ref(),
            oauth_invalid_at_unix_secs,
            oauth_invalid_reason,
            None,
        )
        .await?
        {
            failed_count += 1;
            results.push(json!({
                "key_id": key.id,
                "key_name": key.name,
                "status": "error",
                "message": "Key 状态写入失败",
            }));
            continue;
        }

        if status == "success" {
            sync_antigravity_discovered_models(
                state,
                &provider.id,
                endpoint,
                metadata_update.as_ref(),
            )
            .await;
        }

        if status == "success" {
            success_count += 1;
        } else {
            failed_count += 1;
        }

        let mut payload = serde_json::Map::new();
        payload.insert("key_id".to_string(), json!(key.id));
        payload.insert("key_name".to_string(), json!(key.name));
        payload.insert("status".to_string(), json!(status));
        if let Some(message) = message {
            payload.insert("message".to_string(), json!(message));
        }
        if let Some(metadata) = metadata_update
            .as_ref()
            .and_then(|value| value.get("antigravity"))
            .cloned()
        {
            payload.insert("metadata".to_string(), metadata);
        }
        if let Some(quota_snapshot) = build_quota_snapshot_payload(
            "antigravity",
            key.status_snapshot.as_ref(),
            metadata_update.as_ref(),
        ) {
            payload.insert("quota_snapshot".to_string(), quota_snapshot);
        }
        results.push(serde_json::Value::Object(payload));
    }

    Ok(Some(json!({
        "success": success_count,
        "failed": failed_count,
        "total": results.len(),
        "results": results,
        "message": format!("已处理 {} 个 Key", results.len()),
        "auto_removed": auto_removed_count,
    })))
}
