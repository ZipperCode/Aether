use super::super::shared::{
    build_provider_quota_execution_plan, execute_provider_quota_plan, ProviderQuotaExecutionOutcome,
};
use super::domain::{AttemptResult, ExecutionRoute, QuotaKind, SourceAttempt, StableErrorClass};
use super::routing::{official_balance_execution_timeouts, resolve_execution_route};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use aether_contracts::{ExecutionPlan, ExecutionResult, ProxySnapshot};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{
    build_deepseek_balance_request, build_minimax_balance_request,
    build_minimax_token_plan_request, build_official_api_key_quota_request,
    build_openrouter_credits_request, build_zhipu_account_balance_request,
    build_zhipu_team_quota_request, is_retired_official_api_key_quota_endpoint,
    official_api_key_quota_sources, parse_deepseek_balance,
    parse_official_api_key_quota_for_endpoint, parse_openrouter_credits,
    parse_zhipu_standard_balance, ProviderQuotaSource,
};

pub(super) struct PrepareInput<'a> {
    pub(super) state: &'a AdminAppState<'a>,
    pub(super) provider: &'a StoredProviderCatalogProvider,
    pub(super) endpoint: &'a StoredProviderCatalogEndpoint,
    pub(super) key: &'a StoredProviderCatalogKey,
    pub(super) proxy_override: Option<ProxySnapshot>,
}

struct PreparedQuery {
    plan: ExecutionPlan,
    quota_kind: QuotaKind,
    sources: Vec<ProviderQuotaSource>,
    plan_scope: Option<&'static str>,
}

pub(super) struct PreparedAttempt {
    transport: AdminGatewayProviderTransportSnapshot,
    endpoint: StoredProviderCatalogEndpoint,
    queries: Vec<PreparedQuery>,
    provider_type: String,
    pub(super) route: ExecutionRoute,
    pub(super) quota_kind: QuotaKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PreparationFailure {
    pub(super) class: StableErrorClass,
}

pub(super) async fn prepare_attempt(
    input: PrepareInput<'_>,
) -> Result<PreparedAttempt, PreparationFailure> {
    let provider_type = input.provider.provider_type.trim().to_ascii_lowercase();
    // 已退役接口不需要读取或解密 Key，直接由入口持久化“不支持”来源状态。
    if is_retired_official_api_key_quota_endpoint(&provider_type, input.endpoint) {
        return Err(PreparationFailure {
            class: StableErrorClass::QueryUnsupported,
        });
    }
    let transport = match input
        .state
        .read_provider_transport_snapshot(&input.provider.id, &input.endpoint.id, &input.key.id)
        .await
    {
        Ok(Some(transport)) => transport,
        Ok(None) => {
            return Err(PreparationFailure {
                class: StableErrorClass::TransportUnavailable,
            });
        }
        Err(_) => {
            return Err(PreparationFailure {
                class: StableErrorClass::TransportFailed,
            });
        }
    };
    let secret = transport.key.decrypted_api_key.trim();
    let request_failure = |_| PreparationFailure {
        class: StableErrorClass::RequestInvalid,
    };
    let mut specs = Vec::new();
    if provider_type == "minimax" {
        // 同一区域内账户余额与 Token Plan 是两个独立来源；官方 Key 前缀不能排他选择单一接口。
        specs.push((
            build_minimax_balance_request(&input.key.id, input.endpoint, || secret.into())
                .map_err(request_failure)?,
            None,
        ));
        specs.push((
            build_minimax_token_plan_request(&input.key.id, input.endpoint, || secret.into())
                .map_err(request_failure)?,
            None,
        ));
    } else {
        let spec = match provider_type.as_str() {
            "deepseek" => {
                build_deepseek_balance_request(&input.key.id, input.endpoint, || secret.into())
            }
            "openrouter" => {
                build_openrouter_credits_request(&input.key.id, input.endpoint, || secret.into())
            }
            "moonshot" | "kimi_coding" | "siliconflow" | "zhipu" | "zai" => {
                build_official_api_key_quota_request(
                    &provider_type,
                    &input.key.id,
                    input.endpoint,
                    || secret.into(),
                )
            }
            _ => Err("unsupported official balance provider"),
        }
        .map_err(request_failure)?;
        let personal_scope =
            matches!(provider_type.as_str(), "zhipu" | "zai").then_some("personal");
        specs.push((spec, personal_scope));
        if provider_type == "zhipu" {
            // 三个接口分别反映个人套餐、团队套餐和账户余额，不能用成功结果提前终止其他查询。
            specs.push((
                build_zhipu_team_quota_request(&input.key.id, input.endpoint, || secret.into())
                    .map_err(request_failure)?,
                Some("team"),
            ));
            specs.push((
                build_zhipu_account_balance_request(&input.key.id, input.endpoint, || {
                    secret.into()
                })
                .map_err(request_failure)?,
                None,
            ));
        }
    }
    // 主查询类型跟随首个请求；MiniMax 以余额为主，套餐结果由来源独立表达。
    let quota_kind = QuotaKind::from_spec(&specs[0].0.quota_kind)
        .map_err(|class| PreparationFailure { class })?;
    let route = resolve_execution_route(input.proxy_override, || async {
        let proxy = input
            .state
            .resolve_transport_proxy_snapshot_with_tunnel_affinity(&transport)
            .await;
        let source = input
            .state
            .resolve_transport_proxy_source_with_tunnel_affinity(&transport)
            .await;
        (proxy, source)
    })
    .await;
    let timeouts = Some(official_balance_execution_timeouts(
        input.state.resolve_transport_execution_timeouts(&transport),
        route.proxy.as_ref(),
    ));
    let transport_profile = input.state.resolve_transport_profile(&transport);
    let mut queries = Vec::with_capacity(specs.len());
    for (spec, plan_scope) in specs {
        let query_kind =
            QuotaKind::from_spec(&spec.quota_kind).map_err(|class| PreparationFailure { class })?;
        let mut sources =
            official_api_key_quota_sources(&provider_type, input.endpoint, query_kind.as_str());
        if let Some(scope) = plan_scope {
            for source in &mut sources {
                set_zhipu_source_scope(source, scope);
            }
        }
        queries.push(PreparedQuery {
            plan: build_provider_quota_execution_plan(
                &transport,
                spec,
                route.proxy.clone(),
                transport_profile.clone(),
                timeouts.clone(),
            ),
            quota_kind: query_kind,
            sources,
            plan_scope,
        });
    }
    Ok(PreparedAttempt {
        transport,
        endpoint: input.endpoint.clone(),
        queries,
        provider_type,
        route,
        quota_kind,
    })
}

pub(super) async fn execute_prepared(
    state: &AdminAppState<'_>,
    prepared: PreparedAttempt,
) -> AttemptResult {
    let PreparedAttempt {
        transport,
        endpoint,
        queries,
        provider_type,
        quota_kind,
        ..
    } = prepared;
    let mut attempts = Vec::with_capacity(queries.len());
    // 每个 Key 最多三个固定来源，顺序执行以复用现有并发、超时和代理限制。
    for query in queries {
        let mut result = execute_plan(
            state,
            &transport,
            query.plan,
            query.quota_kind,
            &provider_type,
            &endpoint,
        )
        .await;
        if let Some(scope) = query.plan_scope {
            result = apply_zhipu_plan_scope(result, scope);
        }
        attempts.push(SourceAttempt {
            sources: query.sources,
            result,
        });
    }
    AttemptResult::Sources {
        attempts,
        quota_kind,
    }
}

pub(super) fn apply_zhipu_plan_scope(
    mut attempt: AttemptResult,
    scope: &'static str,
) -> AttemptResult {
    if let AttemptResult::Success { snapshot, .. } = &mut attempt {
        for source in &mut snapshot.sources {
            set_zhipu_source_scope(source, scope);
        }
        for source_id in snapshot
            .windows
            .iter_mut()
            .map(|window| &mut window.source_id)
            .chain(
                snapshot
                    .balances
                    .iter_mut()
                    .map(|balance| &mut balance.source_id),
            )
        {
            if source_id.as_deref() == Some("subscription") {
                *source_id = Some(scope.into());
            }
        }
    }
    attempt
}

fn set_zhipu_source_scope(source: &mut ProviderQuotaSource, scope: &str) {
    if source.id == "subscription" {
        source.id = scope.into();
        source.scope = scope.into();
        source.label = match scope {
            "team" => "团队 Coding Plan",
            _ => "个人 Coding Plan",
        }
        .into();
    }
}

async fn execute_plan(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    plan: ExecutionPlan,
    quota_kind: QuotaKind,
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> AttemptResult {
    match execute_provider_quota_plan(state, transport, plan, quota_kind.as_str()).await {
        Ok(ProviderQuotaExecutionOutcome::Response(result)) => {
            execution_result_to_attempt(result, quota_kind, provider_type, endpoint)
        }
        Ok(ProviderQuotaExecutionOutcome::Failure(_)) | Err(_) => AttemptResult::TransportFailure {
            class: StableErrorClass::TransportFailed,
            quota_kind: Some(quota_kind),
        },
    }
}

pub(super) fn execution_result_to_attempt(
    result: ExecutionResult,
    quota_kind: QuotaKind,
    provider_type: &str,
    endpoint: &StoredProviderCatalogEndpoint,
) -> AttemptResult {
    if !(200..300).contains(&result.status_code) {
        return AttemptResult::HttpFailure {
            status_code: result.status_code,
            class: StableErrorClass::from_http_status(result.status_code),
            headers: result.headers,
            quota_kind,
        };
    }
    let Some(body) = result.body.and_then(|body| body.json_body) else {
        return AttemptResult::ParseFailure {
            class: StableErrorClass::ParseFailed,
            quota_kind,
        };
    };
    if matches!(provider_type, "zhipu" | "zai") {
        if let Some((class, upstream_code, detail)) = zhipu_business_failure(&body) {
            return AttemptResult::BusinessFailure {
                status_code: result.status_code,
                class,
                quota_kind,
                upstream_code,
                detail,
            };
        }
    }
    if provider_type == "minimax" {
        if let Some(code) = body
            .pointer("/base_resp/status_code")
            .and_then(|code| match code {
                serde_json::Value::Number(code) => code.as_u64(),
                serde_json::Value::String(code) => code.trim().parse().ok(),
                _ => None,
            })
        {
            match code {
                // 已确认的业务认证失败使用固定诊断；不复制上游任意消息，也不推断其他业务码。
                2049 => {
                    return AttemptResult::BusinessFailure {
                        status_code: result.status_code,
                        class: StableErrorClass::HttpUnauthorized,
                        quota_kind,
                        upstream_code: Some(2049),
                        detail: "MiniMax quota upstream rejected authentication".into(),
                    };
                }
                // 现场证实 2062 表示该 Key 无适用 Token Plan；仅套餐来源记不适用，余额来源互不影响。
                2062 if quota_kind == QuotaKind::Subscription => {
                    return AttemptResult::BusinessFailure {
                        status_code: result.status_code,
                        class: StableErrorClass::QueryNotApplicable,
                        quota_kind,
                        upstream_code: Some(2062),
                        detail: "MiniMax account has no applicable Token Plan".into(),
                    };
                }
                _ => {}
            }
        }
    }
    let parsed = match provider_type {
        "deepseek" => parse_deepseek_balance(&body),
        "openrouter" => parse_openrouter_credits(&body),
        "zhipu" if quota_kind == QuotaKind::Balance => parse_zhipu_standard_balance(&body),
        "moonshot" | "kimi_coding" | "siliconflow" | "zhipu" | "zai" | "minimax" => {
            parse_official_api_key_quota_for_endpoint(provider_type, &body, endpoint)
        }
        _ => Err("unsupported official balance provider"),
    };
    match parsed {
        Ok(snapshot) => AttemptResult::Success {
            snapshot,
            status_code: result.status_code,
            quota_kind,
        },
        Err(_) => AttemptResult::ParseFailure {
            class: StableErrorClass::ParseFailed,
            quota_kind,
        },
    }
}

fn zhipu_business_failure(
    body: &serde_json::Value,
) -> Option<(StableErrorClass, Option<u16>, String)> {
    let code = body.get("code").and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.trim().to_owned()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    });
    let unsuccessful = body.get("success").and_then(serde_json::Value::as_bool) == Some(false);
    let code_is_success = code
        .as_deref()
        .is_some_and(|code| matches!(code, "0" | "200"));
    if !unsuccessful && (code.is_none() || code_is_success) {
        return None;
    }

    let numeric_code = code.as_deref().and_then(|code| code.parse::<u16>().ok());
    // 诊断只输出已解析的业务码，避免上游任意字符串携带敏感内容。
    let code = numeric_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".into());
    let class = match numeric_code {
        Some(401 | 1000 | 1001 | 1003) => StableErrorClass::HttpUnauthorized,
        Some(403 | 1220) => StableErrorClass::HttpForbidden,
        Some(429 | 1302 | 1305 | 1308 | 1310 | 1313 | 1316..=1321) => {
            StableErrorClass::HttpRateLimited
        }
        Some(500 | 1200 | 1230 | 1234) => StableErrorClass::HttpServer,
        _ => StableErrorClass::HttpClient,
    };
    let reason = match numeric_code {
        Some(1000 | 1001) => "authentication rejected",
        Some(1003) => "authentication token expired",
        Some(1113) => "account balance is insufficient",
        Some(1220) => "quota endpoint is not permitted for this API key",
        Some(1308 | 1310 | 1316..=1321) => "quota limit reached",
        Some(1309) => "GLM Coding Plan subscription expired",
        Some(1315) => "API key product type does not match the selected endpoint",
        Some(1302 | 1305 | 1313) => "quota request was rate limited",
        _ => "quota upstream returned a business error",
    };
    Some((
        class,
        numeric_code,
        format!("upstream business code {code}: {reason}"),
    ))
}
