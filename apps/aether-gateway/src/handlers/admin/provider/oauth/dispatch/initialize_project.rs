use crate::handlers::admin::provider::oauth::errors::build_internal_control_error_response;
use crate::handlers::admin::provider::oauth::quota::shared::{
    execute_provider_quota_plan, force_provider_quota_redirects_disabled,
    resolve_provider_quota_execution_timeouts, ProviderQuotaExecutionOutcome,
};
use crate::handlers::admin::provider::oauth::runtime::resolve_provider_oauth_runtime_endpoints;
use crate::handlers::admin::provider::shared::paths::admin_provider_oauth_initialize_project_key_id;
use crate::handlers::admin::request::{
    AdminAppState, AdminGatewayProviderTransportSnapshot, AdminRequestContext,
};
use crate::provider_key_auth::provider_key_is_oauth_managed;
use crate::GatewayError;
use aether_admin::provider::redaction::admin_provider_upstream_metadata_safe_json;
use aether_contracts::{ExecutionPlan, RequestBody};
use aether_model_fetch::build_antigravity_load_code_assist_plan;
use aether_provider_transport::antigravity::{
    antigravity_account_observation_update, build_antigravity_static_client_headers,
    parse_antigravity_code_assist_response, resolve_local_antigravity_request_auth,
    AntigravityCodeAssistInfo, AntigravityRequestAuthSupport, ANTIGRAVITY_PROVIDER_TYPE,
    ANTIGRAVITY_REQUEST_USER_AGENT,
};
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_ONBOARD_POLLS: usize = 5;
const ONBOARD_POLL_INTERVAL: Duration = Duration::from_millis(1500);
const WORKFLOW_TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
const ANTIGRAVITY_PROD_URL: &str = "https://cloudcode-pa.googleapis.com";

pub(super) async fn handle_admin_provider_oauth_initialize_key_project(
    state: &AdminAppState<'_>,
    request_context: &AdminRequestContext<'_>,
) -> Result<Response<Body>, GatewayError> {
    let Some(key_id) = admin_provider_oauth_initialize_project_key_id(request_context.path())
    else {
        return Ok(build_internal_control_error_response(
            StatusCode::NOT_FOUND,
            "Key 不存在",
        ));
    };

    let key = state
        .read_provider_catalog_keys_by_ids(std::slice::from_ref(&key_id))
        .await?
        .into_iter()
        .next();
    let Some(key) = key else {
        return Ok(build_internal_control_error_response(
            StatusCode::NOT_FOUND,
            "Key 不存在",
        ));
    };

    let provider = state
        .read_provider_catalog_providers_by_ids(std::slice::from_ref(&key.provider_id))
        .await?
        .into_iter()
        .next();
    let Some(provider) = provider else {
        return Ok(build_internal_control_error_response(
            StatusCode::NOT_FOUND,
            "Provider 不存在",
        ));
    };

    let provider_type = provider.provider_type.trim().to_ascii_lowercase();
    if provider_type != ANTIGRAVITY_PROVIDER_TYPE {
        return Ok(build_internal_control_error_response(
            StatusCode::BAD_REQUEST,
            "该 Provider 不是 Antigravity 类型",
        ));
    }

    if !provider_key_is_oauth_managed(&key, &provider_type) {
        return Ok(build_internal_control_error_response(
            StatusCode::BAD_REQUEST,
            "该 Key 不是 OAuth 管理账号",
        ));
    }

    let endpoint_resolution =
        resolve_provider_oauth_runtime_endpoints(state, &provider, &provider_type).await?;
    let Some(endpoint) = endpoint_resolution.runtime_endpoint else {
        return Ok(build_internal_control_error_response(
            StatusCode::BAD_REQUEST,
            "找不到有效端点，无法初始化项目",
        ));
    };

    let Some(transport) = state
        .read_provider_transport_snapshot_uncached(&key.provider_id, &endpoint.id, &key_id)
        .await?
    else {
        return Ok(build_internal_control_error_response(
            StatusCode::BAD_REQUEST,
            "找不到有效 transport 凭据，无法初始化项目",
        ));
    };

    let Some(authorization) = state
        .resolve_local_oauth_header_auth(&transport)
        .await?
        .filter(|(_, value)| !value.trim().is_empty())
    else {
        return Ok(build_internal_control_error_response(
            StatusCode::BAD_REQUEST,
            "缺少有效 OAuth 认证信息，无法初始化项目",
        ));
    };

    let workflow_result = tokio::time::timeout(WORKFLOW_TOTAL_TIMEOUT, async {
        // 1. 优先检查本地已配置的伴生项目（auth_config 或 upstream_metadata）
        let existing_configured_project = if let AntigravityRequestAuthSupport::Supported(auth) =
            resolve_local_antigravity_request_auth(&transport)
        {
            Some(auth.project_id)
        } else {
            None
        };

        if let Some(project_id) = existing_configured_project {
            let now_secs = current_unix_timestamp();
            let discovered = execute_load_code_assist(state, &transport, &authorization).await;

            let observation = match discovered {
                Ok((Some(mut info), _)) => {
                    if info.project_id.is_none() {
                        info.project_id = Some(project_id.clone());
                    }
                    antigravity_account_observation_update(Some(&info), now_secs)
                }
                _ => antigravity_account_observation_update(None, now_secs),
            };

            let Some(final_meta) = state
                .app()
                .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                .await?
            else {
                return Ok(build_internal_control_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Key 状态写入失败",
                ));
            };

            return Ok(build_success_response(
                &key_id,
                Some(project_id),
                final_meta,
                false,
                "伴生项目已配置，无需重复初始化",
            ));
        }

        // 2. 本地无项目，执行固定走生产端的 loadCodeAssist 发现
        let now_secs = current_unix_timestamp();
        let initial_discovery = execute_load_code_assist(state, &transport, &authorization).await;

        let (discovered_info, raw_json) = match initial_discovery {
            Ok((Some(info), raw)) => (info, raw),
            Ok((None, _)) => {
                let observation = antigravity_account_observation_update(None, now_secs);
                state
                    .app()
                    .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                    .await?;
                return Ok(build_internal_control_error_response(
                    StatusCode::BAD_GATEWAY,
                    "上游项目发现响应格式异常",
                ));
            }
            Err(err) => {
                let observation = antigravity_account_observation_update(None, now_secs);
                state
                    .app()
                    .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                    .await?;
                return Ok(build_internal_control_error_response(
                    StatusCode::BAD_GATEWAY,
                    err,
                ));
            }
        };

        // 如果发现请求直接返回了伴生项目，跳过 onboardUser
        if let Some(project_id) = discovered_info
            .project_id
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            let observation =
                antigravity_account_observation_update(Some(&discovered_info), now_secs);
            let Some(final_meta) = state
                .app()
                .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                .await?
            else {
                return Ok(build_internal_control_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Key 状态写入失败",
                ));
            };

            return Ok(build_success_response(
                &key_id,
                Some(project_id.to_string()),
                final_meta,
                false,
                "发现已有伴生项目，已同步账号状态",
            ));
        }

        // 将首次发现的有效套餐观察先行原子落盘，防止后续引导失败丢失刚刚观察到的有效套餐
        let initial_obs = antigravity_account_observation_update(Some(&discovered_info), now_secs);
        let Some(_saved_initial) = state
            .app()
            .persist_antigravity_account_observation(&key_id, &initial_obs, now_secs)
            .await?
        else {
            return Ok(build_internal_control_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Key 状态写入失败",
            ));
        };

        // 3. 上游未绑定项目，提取默认允许套餐发起 onboardUser 引导
        let bootstrap_tier_id = extract_default_allowed_tier_id(&raw_json);

        // 4. 轮询 onboardUser：硬错误立即退出，仅 pending 重试至最多 5 次
        let mut acquired_project_id: Option<String> = None;
        let mut poll_error: Option<String> = None;

        for attempt in 0..MAX_ONBOARD_POLLS {
            match execute_single_onboard_user(state, &transport, &authorization, &bootstrap_tier_id)
                .await
            {
                Ok(Some(project_id)) => {
                    acquired_project_id = Some(project_id);
                    break;
                }
                Ok(None) => {
                    if attempt + 1 < MAX_ONBOARD_POLLS {
                        tokio::time::sleep(ONBOARD_POLL_INTERVAL).await;
                    }
                }
                Err(hard_err) => {
                    poll_error = Some(hard_err);
                    break;
                }
            }
        }

        if let Some(project_id) = acquired_project_id {
            // (7) 获取项目后立即持久化保存伴生项目，防止跟进发现失败或超时丢失已初始化项目
            let direct_project_update = json!({
                "antigravity": {
                    "project_id": project_id,
                }
            });
            let Some(_direct_saved) = state
                .app()
                .persist_antigravity_account_observation(&key_id, &direct_project_update, now_secs)
                .await?
            else {
                return Ok(build_internal_control_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Key 状态写入失败",
                ));
            };

            // 跟进一次 loadCodeAssist 以获取与项目关联后的套餐观察
            let followup_now_secs = current_unix_timestamp();
            let followup_discovery =
                execute_load_code_assist(state, &transport, &authorization).await;

            let followup_observation = match followup_discovery {
                Ok((Some(mut info), _)) => {
                    if info.project_id.is_none() {
                        info.project_id = Some(project_id.clone());
                    }
                    antigravity_account_observation_update(Some(&info), followup_now_secs)
                }
                _ => {
                    // (3) 跟进发现失败：记录错误观察并保留有效套餐
                    antigravity_account_observation_update(None, followup_now_secs)
                }
            };

            let Some(final_meta) = state
                .app()
                .persist_antigravity_account_observation(
                    &key_id,
                    &followup_observation,
                    followup_now_secs,
                )
                .await?
            else {
                return Ok(build_internal_control_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Key 状态写入失败",
                ));
            };

            Ok(build_success_response(
                &key_id,
                Some(project_id),
                final_meta,
                true,
                "伴生项目初始化成功",
            ))
        } else {
            let now_secs = current_unix_timestamp();
            let observation = antigravity_account_observation_update(None, now_secs);
            state
                .app()
                .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                .await?;

            let message = poll_error.unwrap_or_else(|| "伴生项目初始化未能完成".to_string());
            Ok(build_internal_control_error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                message,
            ))
        }
    })
    .await;

    match workflow_result {
        Ok(response) => response,
        Err(_) => {
            let now_secs = current_unix_timestamp();
            let observation = antigravity_account_observation_update(None, now_secs);
            state
                .app()
                .persist_antigravity_account_observation(&key_id, &observation, now_secs)
                .await?;

            Ok(build_internal_control_error_response(
                StatusCode::GATEWAY_TIMEOUT,
                "伴生项目初始化超时，请稍后重试",
            ))
        }
    }
}

fn build_success_response(
    key_id: &str,
    project_id: Option<String>,
    merged_metadata: Value,
    onboarded: bool,
    message: &'static str,
) -> Response<Body> {
    let anti_obj = merged_metadata.get("antigravity");
    let subscription_tier = anti_obj
        .and_then(|v| v.get("subscription_tier"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let paid_tier_id = anti_obj
        .and_then(|v| v.get("paid_tier_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let current_tier_id = anti_obj
        .and_then(|v| v.get("current_tier_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let safe_metadata = admin_provider_upstream_metadata_safe_json(Some(&merged_metadata));
    Json(json!({
        "key_id": key_id,
        "project_id": project_id,
        "subscription_tier": subscription_tier,
        "paid_tier_id": paid_tier_id,
        "current_tier_id": current_tier_id,
        "onboarded": onboarded,
        "message": message,
        "metadata": safe_metadata,
    }))
    .into_response()
}

async fn execute_load_code_assist(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: &(String, String),
) -> Result<(Option<AntigravityCodeAssistInfo>, Value), String> {
    let mut plan = build_antigravity_load_code_assist_plan(state.app(), transport)
        .await
        .map_err(|_| "构建 loadCodeAssist 请求失败".to_string())?;

    plan.provider_name = Some(ANTIGRAVITY_PROVIDER_TYPE.to_string());
    force_provider_quota_redirects_disabled(&mut plan.headers);
    plan.headers
        .insert(authorization.0.clone(), authorization.1.clone());

    let proxy = state
        .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
        .await;
    plan.proxy = proxy;
    plan.timeouts = Some(resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        plan.proxy.as_ref(),
    ));

    let outcome = execute_provider_quota_plan(state, transport, plan, "antigravity")
        .await
        .map_err(|_| "执行 loadCodeAssist 运行时调用失败".to_string())?;

    let result = match outcome {
        ProviderQuotaExecutionOutcome::Response(res) => res,
        ProviderQuotaExecutionOutcome::Failure(_) => {
            return Err("loadCodeAssist 执行失败".to_string());
        }
    };

    if result.status_code == 200 {
        let raw = result
            .body
            .as_ref()
            .and_then(|b| b.json_body.as_ref())
            .cloned()
            .unwrap_or(Value::Null);
        let info = parse_antigravity_code_assist_response(&raw);
        Ok((info, raw))
    } else {
        Err("loadCodeAssist 返回非成功状态".to_string())
    }
}

async fn execute_single_onboard_user(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    authorization: &(String, String),
    tier_id: &str,
) -> Result<Option<String>, String> {
    let mut headers = build_antigravity_static_client_headers(None, None);
    headers.insert(authorization.0.clone(), authorization.1.clone());
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert("accept".to_string(), "application/json".to_string());
    headers
        .entry("user-agent".to_string())
        .or_insert_with(|| ANTIGRAVITY_REQUEST_USER_AGENT.to_string());
    force_provider_quota_redirects_disabled(&mut headers);

    let body_json = json!({
        "tier_id": tier_id,
        "metadata": {
            "ide_type": "ANTIGRAVITY",
            "ide_version": aether_provider_transport::client_identity::ANTIGRAVITY.version,
            "ide_name": "antigravity",
        }
    });

    let proxy = state
        .resolve_transport_proxy_snapshot_with_tunnel_affinity(transport)
        .await;
    let timeouts = resolve_provider_quota_execution_timeouts(
        state.resolve_transport_execution_timeouts(transport),
        proxy.as_ref(),
    );

    let plan = ExecutionPlan {
        request_id: "antigravity-onboard-user".to_string(),
        candidate_id: None,
        provider_name: Some(ANTIGRAVITY_PROVIDER_TYPE.to_string()),
        provider_id: transport.provider.id.clone(),
        endpoint_id: transport.endpoint.id.clone(),
        key_id: transport.key.id.clone(),
        method: "POST".to_string(),
        url: format!(
            "{}/v1internal:onboardUser",
            transport.endpoint.base_url.trim().trim_end_matches('/')
        ),
        headers,
        content_type: Some("application/json".to_string()),
        content_encoding: None,
        body: RequestBody::from_json(body_json),
        stream: false,
        client_api_format: "gemini:generate_content".to_string(),
        provider_api_format: "antigravity:onboard_user".to_string(),
        model_name: Some("onboardUser".to_string()),
        proxy,
        transport_profile: state.resolve_transport_profile(transport),
        timeouts: Some(timeouts),
    };

    let outcome = execute_provider_quota_plan(state, transport, plan, "antigravity")
        .await
        .map_err(|_| "执行 onboardUser 运行时调用失败".to_string())?;

    let result = match outcome {
        ProviderQuotaExecutionOutcome::Response(res) => res,
        ProviderQuotaExecutionOutcome::Failure(_) => {
            return Err("onboardUser 执行失败".to_string());
        }
    };

    if result.status_code == 200 {
        let json_body = result
            .body
            .as_ref()
            .and_then(|b| b.json_body.as_ref())
            .unwrap_or(&Value::Null);

        match json_body.get("done").and_then(Value::as_bool) {
            Some(true) => extract_companion_project_id_from_onboard_response(json_body)
                .map(Some)
                .ok_or_else(|| "onboardUser 返回已完成但缺少伴生项目标识".to_string()),
            Some(false) => Ok(None),
            None => Err("onboardUser 响应缺少有效完成状态".to_string()),
        }
    } else {
        Err(format!("onboardUser 返回 HTTP {}", result.status_code))
    }
}

fn extract_default_allowed_tier_id(value: &Value) -> String {
    let allowed_tiers = value.get("allowedTiers").and_then(Value::as_array);
    if let Some(tiers) = allowed_tiers {
        for tier in tiers {
            if tier.get("isDefault").and_then(Value::as_bool) == Some(true) {
                if let Some(id) = tier
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if !id.eq_ignore_ascii_case("legacy-tier") {
                        return id.to_string();
                    }
                }
            }
        }
        for tier in tiers {
            if let Some(id) = tier
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if !id.eq_ignore_ascii_case("legacy-tier") {
                    return id.to_string();
                }
            }
        }
    }
    "free-tier".to_string()
}

fn extract_companion_project_id_from_onboard_response(value: &Value) -> Option<String> {
    let response = value.get("response").unwrap_or(value);
    let project_val = response
        .get("cloudaicompanionProject")
        .or_else(|| response.get("cloudAiCompanionProject"))
        .or_else(|| value.get("cloudaicompanionProject"))
        .or_else(|| value.get("cloudAiCompanionProject"))?;

    if let Some(id) = project_val
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(id.to_string());
    }

    if let Some(obj) = project_val.as_object() {
        if let Some(id) = obj
            .get("id")
            .or_else(|| obj.get("project_id"))
            .or_else(|| obj.get("projectId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(id.to_string());
        }
    }

    None
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
