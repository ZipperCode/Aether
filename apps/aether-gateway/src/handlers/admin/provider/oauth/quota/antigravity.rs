use super::shared::{
    build_provider_quota_execution_plan, build_quota_snapshot_payload, coerce_json_f64,
    coerce_json_string, execute_provider_quota_plan, extract_execution_error_message,
    force_provider_quota_redirects_disabled, merge_upstream_metadata,
    oauth_refresh_auto_removed_result, persist_credential_fenced_provider_quota_refresh_state,
    persist_provider_quota_refresh_state, quota_key_auto_removed,
    quota_refresh_success_invalid_state, resolve_provider_quota_execution_timeouts,
    ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use crate::GatewayError;
use aether_admin::provider::quota::{
    parse_antigravity_quota_summary_response, parse_antigravity_usage_response,
};
use aether_admin::provider::redaction::admin_provider_metadata_bucket_safe_json;
use aether_contracts::ProxySnapshot;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_model_fetch::build_antigravity_load_code_assist_plan;
use aether_provider_pool::{
    build_antigravity_pool_quota_request, build_antigravity_pool_quota_summary_request,
};
use aether_provider_transport::antigravity::{
    antigravity_account_observation_update, antigravity_entitlement_candidates,
    is_antigravity_entitlement_rejection, parse_antigravity_code_assist_response,
    AntigravityCodeAssistInfo,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

async fn execute_antigravity_quota_plan(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: (String, String),
    project_id: &str,
    tier: Option<&str>,
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
        tier,
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

pub(crate) async fn fetch_antigravity_code_assist_full(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    proxy_override: Option<&ProxySnapshot>,
) -> (Option<AntigravityCodeAssistInfo>, Option<serde_json::Value>) {
    let mut plan = match build_antigravity_load_code_assist_plan(state.app(), transport).await {
        Ok(plan) => plan,
        Err(error) => {
            warn!(error = %error, "Antigravity loadCodeAssist plan build failed");
            return (None, None);
        }
    };
    // 模型发现 builder 使用用户自定义显示名；额度执行器需要规范提供商类型校验源站。
    plan.provider_name = Some("antigravity".to_string());
    force_provider_quota_redirects_disabled(&mut plan.headers);

    let proxy = match proxy_override {
        Some(proxy) => Some(proxy.clone()),
        None => {
            state
                .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
                .await
        }
    };
    if let Some(proxy) = proxy {
        plan.proxy = Some(proxy);
    }
    plan.timeouts = Some(resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        plan.proxy.as_ref(),
    ));

    let outcome = match execute_provider_quota_plan(state, transport, plan, "antigravity").await {
        Ok(outcome) => outcome,
        Err(error) => {
            warn!(error = ?error, "Antigravity loadCodeAssist request failed");
            return (None, None);
        }
    };
    let result = match outcome {
        ProviderQuotaExecutionOutcome::Response(result) => result,
        ProviderQuotaExecutionOutcome::Failure(_) => {
            warn!("Antigravity loadCodeAssist execution failed");
            return (None, None);
        }
    };

    if result.status_code == 200 {
        let json_body = result.body.and_then(|body| body.json_body);
        let parsed = json_body
            .as_ref()
            .and_then(parse_antigravity_code_assist_response);
        (parsed, json_body)
    } else {
        warn!(
            status_code = result.status_code,
            "Antigravity loadCodeAssist returned a non-success status"
        );
        (None, None)
    }
}

pub(crate) async fn fetch_antigravity_code_assist_best_effort(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    proxy_override: Option<&ProxySnapshot>,
) -> Option<AntigravityCodeAssistInfo> {
    fetch_antigravity_code_assist_full(state, transport, proxy_override)
        .await
        .0
}

async fn fetch_antigravity_quota_summary_best_effort(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: (String, String),
    project_id: &str,
    identity_headers: BTreeMap<String, String>,
    proxy_override: Option<&ProxySnapshot>,
) -> Option<serde_json::Value> {
    let mut request_project_id = Some(project_id);

    loop {
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
        let spec = build_antigravity_pool_quota_summary_request(
            &transport.key.id,
            &transport.endpoint.base_url,
            authorization.clone(),
            request_project_id,
            identity_headers.clone(),
        );
        let plan = build_provider_quota_execution_plan(
            transport,
            spec,
            proxy,
            state.resolve_transport_profile(transport),
            timeouts,
        );
        let outcome = match execute_provider_quota_plan(state, transport, plan, "antigravity").await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                warn!(error = ?error, "Antigravity grouped quota request failed");
                return None;
            }
        };
        let result = match outcome {
            ProviderQuotaExecutionOutcome::Response(result) => result,
            ProviderQuotaExecutionOutcome::Failure(detail) => {
                warn!(detail = %detail, "Antigravity grouped quota execution failed");
                return None;
            }
        };

        if result.status_code == 200 {
            return result
                .body
                .as_ref()
                .and_then(|body| body.json_body.as_ref())
                .and_then(parse_antigravity_quota_summary_response);
        }
        if result.status_code == 403 && request_project_id.is_some() {
            request_project_id = None;
            continue;
        }

        warn!(
            status_code = result.status_code,
            "Antigravity grouped quota request returned a non-success status"
        );
        return None;
    }
}

/// 刷新 Antigravity Key 额度与上游模型元数据；模型目录仅由显式导入操作写入。
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

        let now_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        let account_info =
            fetch_antigravity_code_assist_best_effort(state, &transport, proxy_override.as_ref())
                .await;

        // 成功时写权威套餐/原始 ID（未知则写入显式 null 以清除失效套餐）；失败保留旧观察并记录失败状态。
        let observation_update =
            antigravity_account_observation_update(account_info.as_ref(), now_unix_secs);
        transport.key.upstream_metadata = Some(merge_upstream_metadata(
            transport.key.upstream_metadata.as_ref(),
            &observation_update,
        ));

        let identity = state.resolve_local_antigravity_identity_headers(&transport);
        let Some((project_id, identity_headers)) = identity else {
            failed_count += 1;
            let (oauth_invalid_at_unix_secs, oauth_invalid_reason) =
                quota_refresh_success_invalid_state(&key);

            // 即使缺少 project_id，也必须持久化成功获取的套餐信息及查询状态；写入失败必须报错。
            if !persist_provider_quota_refresh_state(
                state,
                &key.id,
                Some(&observation_update),
                oauth_invalid_at_unix_secs,
                oauth_invalid_reason,
                None,
            )
            .await?
            {
                results.push(json!({
                    "key_id": key.id,
                    "key_name": key.name,
                    "status": "error",
                    "message": "Key 状态写入失败",
                }));
                continue;
            }

            let mut payload = serde_json::Map::new();
            payload.insert("key_id".to_string(), json!(key.id));
            payload.insert("key_name".to_string(), json!(key.name));
            payload.insert("status".to_string(), json!("error"));
            payload.insert(
                "message".to_string(),
                json!("缺少 Antigravity project_id，loadCodeAssist 未返回可用项目信息"),
            );
            if let Some(metadata) = transport
                .key
                .upstream_metadata
                .as_ref()
                .and_then(|value| value.get("antigravity"))
            {
                payload.insert(
                    "metadata".to_string(),
                    admin_provider_metadata_bucket_safe_json("antigravity", Some(metadata)),
                );
            }
            results.push(serde_json::Value::Object(payload));
            continue;
        };

        // 在真实上游额度请求前捕获当前精确模型事实；成功后仅清理仍为同一观察的
        // 正向模型。失败、CAS 冲突或凭据替换都不会清理。
        let expected_credential_fence = state
            .app()
            .capture_provider_transport_credential_fence(&transport)
            .await?;
        let captured_quota =
            crate::orchestration::capture_antigravity_model_quota_refresh(state.app(), &transport)
                .await?;

        // 按 raw paid tier -> raw current tier -> None 逐级尝试；仅在明确的权益拒绝时降级重试。
        let candidates =
            antigravity_entitlement_candidates(transport.key.upstream_metadata.as_ref());
        let mut final_result = None;

        for (idx, candidate_tier) in candidates.iter().enumerate() {
            let outcome = execute_antigravity_quota_plan(
                state,
                &transport,
                authorization.clone(),
                &project_id,
                candidate_tier.as_deref(),
                identity_headers.clone(),
                proxy_override.as_ref(),
            )
            .await?;

            let res = match outcome {
                ProviderQuotaExecutionOutcome::Response(res) => res,
                ProviderQuotaExecutionOutcome::Failure(_) => {
                    final_result = None;
                    break;
                }
            };

            if res.status_code == 200 {
                final_result = Some(res);
                break;
            }

            let is_last = idx + 1 == candidates.len();
            let body_json = res
                .body
                .as_ref()
                .and_then(|body| body.json_body.as_ref())
                .unwrap_or(&serde_json::Value::Null);

            if !is_last && is_antigravity_entitlement_rejection(res.status_code, body_json) {
                warn!(
                    status_code = res.status_code,
                    tier = ?candidate_tier,
                    "Antigravity fetchAvailableModels entitlement rejected, falling back to next candidate"
                );
                continue;
            }

            final_result = Some(res);
            break;
        }
        let result = final_result;

        let (mut oauth_invalid_at_unix_secs, mut oauth_invalid_reason) =
            quota_refresh_success_invalid_state(&key);
        let mut status = "error".to_string();
        let mut message = None::<String>;
        let mut quota_metadata = None::<serde_json::Value>;

        let status_code = result.as_ref().map(|result| result.status_code);
        if status_code == Some(200) {
            if let Some(body_json) = result
                .as_ref()
                .and_then(|result| result.body.as_ref())
                .and_then(|body| body.json_body.as_ref())
            {
                if let Some(mut metadata) =
                    parse_antigravity_usage_response(body_json, now_unix_secs)
                {
                    if let Some(metadata) = metadata.as_object_mut() {
                        metadata.insert("project_id".to_string(), json!(project_id));
                    }
                    if let Some(quota_groups) = fetch_antigravity_quota_summary_best_effort(
                        state,
                        &transport,
                        authorization,
                        &project_id,
                        identity_headers,
                        proxy_override.as_ref(),
                    )
                    .await
                    {
                        if let Some(metadata) = metadata.as_object_mut() {
                            metadata.insert("quota_groups".to_string(), quota_groups);
                            metadata.insert(
                                "quota_groups_updated_at".to_string(),
                                json!(now_unix_secs),
                            );
                        }
                    }
                    quota_metadata = Some(json!({ "antigravity": metadata }));
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
            message = Some(match status_code {
                Some(code) => format!("fetchAvailableModels 返回状态码 {code}"),
                None => "fetchAvailableModels 请求执行失败".to_string(),
            });
            if status_code == Some(403) {
                let reason = "账户访问被禁止".to_string();
                oauth_invalid_at_unix_secs = Some(now_unix_secs);
                oauth_invalid_reason = Some(format!("账户访问被禁止: {reason}"));
                let anti_meta = json!({
                    "is_forbidden": true,
                    "forbidden_reason": reason,
                    "forbidden_at": now_unix_secs,
                    "updated_at": now_unix_secs,
                });
                quota_metadata = Some(json!({ "antigravity": anti_meta }));
            }
        }

        // 构造仅包含本次更新的 metadata patch，由持久化层负责合并至全局 metadata。
        let mut metadata_update = quota_metadata;
        if let Some(obs_obj) = observation_update
            .get("antigravity")
            .and_then(serde_json::Value::as_object)
        {
            let mut anti = metadata_update
                .as_mut()
                .and_then(|v| v.get_mut("antigravity"))
                .and_then(serde_json::Value::as_object_mut);
            if let Some(anti) = anti.as_mut() {
                for (k, v) in obs_obj {
                    if !anti.contains_key(k) {
                        anti.insert(k.clone(), v.clone());
                    }
                }
            } else {
                metadata_update = Some(observation_update.clone());
            }
        }
        // 页面直接应用本次刷新结果，若账号查询失败需在返回 payload 中保留未能更新的旧套餐。
        if account_info.is_none() {
            if let Some(tier) = transport
                .key
                .upstream_metadata
                .as_ref()
                .and_then(|m| m.pointer("/antigravity/subscription_tier"))
            {
                if let Some(anti) = metadata_update
                    .as_mut()
                    .and_then(|v| v.get_mut("antigravity"))
                    .and_then(serde_json::Value::as_object_mut)
                {
                    if !anti.contains_key("subscription_tier") {
                        anti.insert("subscription_tier".to_string(), tier.clone());
                    }
                }
            }
        }
        let persisted = if let Some(fence) = expected_credential_fence.as_ref() {
            persist_credential_fenced_provider_quota_refresh_state(
                state,
                &key.id,
                metadata_update.as_ref(),
                oauth_invalid_at_unix_secs,
                oauth_invalid_reason,
                None,
                fence,
            )
            .await?
        } else {
            persist_provider_quota_refresh_state(
                state,
                &key.id,
                metadata_update.as_ref(),
                oauth_invalid_at_unix_secs,
                oauth_invalid_reason,
                None,
            )
            .await?
        };
        if !persisted {
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
            // Only positive exact models from this persisted refresh clear facts,
            // and only when the captured observation is still byte-identical.
            if let Some(captured) = captured_quota.as_ref() {
                if let Err(err) = crate::orchestration::reconcile_antigravity_model_quota_refresh(
                    state.app(),
                    captured,
                    metadata_update.as_ref(),
                )
                .await
                {
                    warn!(
                        key_id = %key.id,
                        error = ?err,
                        "Antigravity quota refresh could not reconcile scoped model facts"
                    );
                }
            }
            // 仅已落库的模型额度更新重算候选；失败和套餐观察不恢复账号，亲和映射保持不变。
            state.app().invalidate_provider_quota_candidate_caches();
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
        {
            payload.insert(
                "metadata".to_string(),
                admin_provider_metadata_bucket_safe_json("antigravity", Some(metadata)),
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GatewayDataState;
    use crate::AppState;
    use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
    use aether_data_contracts::repository::provider_catalog::{
        StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
    };
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn refresh_antigravity_missing_project_persists_observation_and_returns_error() {
        let provider = StoredProviderCatalogProvider::new(
            "provider-ant".to_string(),
            "Antigravity".to_string(),
            None,
            "antigravity".to_string(),
        )
        .expect("provider should build");

        let endpoint = StoredProviderCatalogEndpoint::new(
            "endpoint-ant".to_string(),
            "provider-ant".to_string(),
            "gemini:generate_content".to_string(),
            None,
            None,
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            "https://127.0.0.1:9".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build");

        let encrypted_auth_config = aether_crypto::encrypt_python_fernet_plaintext(
            aether_crypto::DEVELOPMENT_ENCRYPTION_KEY,
            r#"{
                "client_version":"1.18.4",
                "session_id":"session-ant-1"
            }"#,
        )
        .expect("auth config ciphertext should build");

        let mut key = StoredProviderCatalogKey::new(
            "key-ant-missing-proj".to_string(),
            provider.id.clone(),
            "default".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(json!(["gemini:generate_content"])),
            aether_crypto::encrypt_python_fernet_plaintext(
                aether_crypto::DEVELOPMENT_ENCRYPTION_KEY,
                "ya29.ant-token",
            )
            .expect("api key ciphertext should build"),
            Some(encrypted_auth_config),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build");

        key.upstream_metadata = Some(json!({
            "antigravity": {
                "subscription_tier": "pro",
                "paid_tier_id": "g1-pro-tier",
            }
        }));

        let repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
            vec![provider.clone()],
            vec![endpoint.clone()],
            vec![key.clone()],
        ));
        let data =
            GatewayDataState::with_provider_catalog_repository_for_tests(Arc::clone(&repository))
                .with_cached_provider_catalog_reader_for_tests(Arc::clone(&repository))
                .with_encryption_key_for_tests(aether_crypto::DEVELOPMENT_ENCRYPTION_KEY);
        let app = AppState::new()
            .expect("app should build")
            .with_execution_runtime_override_base_url("http://127.0.0.1:9")
            .with_data_state_for_tests(data);
        let admin_state = AdminAppState::new(&app);

        let result = refresh_antigravity_provider_quota_locally(
            &admin_state,
            &provider,
            &endpoint,
            vec![key.clone()],
            None,
        )
        .await
        .expect("refresh should execute");

        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res["failed"], json!(1));
        assert_eq!(res["results"][0]["status"], json!("error"));

        let updated_keys = app
            .read_provider_catalog_keys_by_ids(&["key-ant-missing-proj".to_string()])
            .await
            .expect("read should succeed");
        let meta = updated_keys[0].upstream_metadata.as_ref().unwrap();
        assert_eq!(
            meta.pointer("/antigravity/account_info_status"),
            Some(&json!("error"))
        );
        assert_eq!(
            meta.pointer("/antigravity/subscription_tier"),
            Some(&json!("pro"))
        );
    }
}
