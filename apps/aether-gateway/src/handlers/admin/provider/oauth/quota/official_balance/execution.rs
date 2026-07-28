use super::super::shared::{
    build_provider_quota_execution_plan, execute_provider_quota_plan,
    resolve_provider_quota_execution_timeouts, ProviderQuotaExecutionOutcome,
};
use super::domain::{AttemptResult, ExecutionRoute, QuotaKind, StableErrorClass};
use super::routing::{official_balance_execution_timeouts, resolve_execution_route};
use crate::handlers::admin::request::{AdminAppState, AdminGatewayProviderTransportSnapshot};
use aether_contracts::{ExecutionPlan, ExecutionResult, ProxySnapshot};
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_provider_pool::{
    build_deepseek_balance_request, build_official_api_key_quota_request,
    build_openrouter_credits_request, build_zhipu_account_balance_request, parse_deepseek_balance,
    parse_official_api_key_quota, parse_openrouter_credits, parse_zhipu_standard_balance,
    ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD, ZHIPU_TOKEN_PLAN_STATUS_FIELD,
};

pub(super) struct PrepareInput<'a> {
    pub(super) state: &'a AdminAppState<'a>,
    pub(super) provider: &'a StoredProviderCatalogProvider,
    pub(super) endpoint: &'a StoredProviderCatalogEndpoint,
    pub(super) key: &'a StoredProviderCatalogKey,
    pub(super) proxy_override: Option<ProxySnapshot>,
}

pub(super) struct PreparedAttempt {
    transport: AdminGatewayProviderTransportSnapshot,
    plan: ExecutionPlan,
    fallback_plan: Option<ExecutionPlan>,
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
    let provider_type = input.provider.provider_type.trim().to_ascii_lowercase();
    let secret = transport.key.decrypted_api_key.trim().to_owned();
    let fallback_secret = secret.clone();
    let spec = match provider_type.as_str() {
        "deepseek" => build_deepseek_balance_request(&input.key.id, input.endpoint, move || secret),
        "openrouter" => {
            build_openrouter_credits_request(&input.key.id, input.endpoint, move || secret)
        }
        "moonshot" | "kimi_coding" | "siliconflow" | "zhipu" | "zai" => {
            build_official_api_key_quota_request(
                &provider_type,
                &input.key.id,
                input.endpoint,
                move || secret,
            )
        }
        _ => Err("unsupported official balance provider"),
    }
    .map_err(|_| PreparationFailure {
        class: StableErrorClass::RequestInvalid,
    })?;
    let fallback_spec = (provider_type == "zhipu")
        .then(|| {
            build_zhipu_account_balance_request(&input.key.id, input.endpoint, move || {
                fallback_secret
            })
        })
        .transpose()
        .ok()
        .flatten();
    let quota_kind =
        QuotaKind::from_spec(&spec.quota_kind).map_err(|class| PreparationFailure { class })?;
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
    let fallback_plan = fallback_spec.map(|fallback_spec| {
        build_provider_quota_execution_plan(
            &transport,
            fallback_spec,
            route.proxy.clone(),
            transport_profile.clone(),
            timeouts.clone(),
        )
    });
    let plan = build_provider_quota_execution_plan(
        &transport,
        spec,
        route.proxy.clone(),
        transport_profile,
        timeouts,
    );
    Ok(PreparedAttempt {
        transport,
        plan,
        fallback_plan,
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
        plan,
        fallback_plan,
        provider_type,
        quota_kind,
        ..
    } = prepared;
    let attempt = execute_plan(state, &transport, plan, quota_kind, &provider_type).await;
    if should_fallback_to_zhipu_balance(&attempt) {
        if let Some(fallback_plan) = fallback_plan {
            let fallback = execute_plan(
                state,
                &transport,
                fallback_plan,
                QuotaKind::Balance,
                &provider_type,
            )
            .await;
            return apply_zhipu_token_plan_fallback_policy(&attempt, fallback);
        }
    }
    attempt
}

pub(super) fn apply_zhipu_token_plan_fallback_policy(
    primary: &AttemptResult,
    mut fallback: AttemptResult,
) -> AttemptResult {
    let status = match primary {
        AttemptResult::BusinessFailure {
            quota_kind: QuotaKind::Subscription,
            upstream_code: Some(1220),
            ..
        } => "not_permitted",
        AttemptResult::BusinessFailure {
            quota_kind: QuotaKind::Subscription,
            upstream_code: Some(1309),
            ..
        } => "expired",
        AttemptResult::BusinessFailure {
            quota_kind: QuotaKind::Subscription,
            upstream_code: Some(1315),
            ..
        } => "product_mismatch",
        AttemptResult::BusinessFailure {
            quota_kind: QuotaKind::Subscription,
            class: StableErrorClass::HttpClient | StableErrorClass::HttpForbidden,
            ..
        } => "business_error",
        AttemptResult::ParseFailure {
            quota_kind: QuotaKind::Subscription,
            ..
        } => "unverified",
        _ => return fallback,
    };
    if let AttemptResult::Success {
        snapshot,
        quota_kind: QuotaKind::Balance,
        ..
    } = &mut fallback
    {
        snapshot.exhausted = true;
        snapshot.extensions.insert(
            ZHIPU_TOKEN_PLAN_STATUS_FIELD.into(),
            serde_json::Value::String(status.into()),
        );
        snapshot.extensions.insert(
            ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD.into(),
            serde_json::Value::Bool(true),
        );
        snapshot.extensions.insert(
            "scheduling_block_reason".into(),
            serde_json::Value::String("token_plan_unavailable".into()),
        );
    }
    fallback
}

async fn execute_plan(
    state: &AdminAppState<'_>,
    transport: &AdminGatewayProviderTransportSnapshot,
    plan: ExecutionPlan,
    quota_kind: QuotaKind,
    provider_type: &str,
) -> AttemptResult {
    match execute_provider_quota_plan(state, transport, plan, quota_kind.as_str()).await {
        Ok(ProviderQuotaExecutionOutcome::Response(result)) => {
            execution_result_to_attempt(result, quota_kind, provider_type)
        }
        Ok(ProviderQuotaExecutionOutcome::Failure(_)) | Err(_) => AttemptResult::TransportFailure {
            class: StableErrorClass::TransportFailed,
            quota_kind: Some(quota_kind),
        },
    }
}

pub(super) fn should_fallback_to_zhipu_balance(attempt: &AttemptResult) -> bool {
    matches!(
        attempt,
        AttemptResult::ParseFailure {
            quota_kind: QuotaKind::Subscription,
            ..
        } | AttemptResult::BusinessFailure {
            quota_kind: QuotaKind::Subscription,
            class: StableErrorClass::HttpClient | StableErrorClass::HttpForbidden,
            ..
        }
    )
}

pub(super) fn execution_result_to_attempt(
    result: ExecutionResult,
    quota_kind: QuotaKind,
    provider_type: &str,
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
    let parsed = match provider_type {
        "deepseek" => parse_deepseek_balance(&body),
        "openrouter" => parse_openrouter_credits(&body),
        "zhipu" if quota_kind == QuotaKind::Balance => parse_zhipu_standard_balance(&body),
        "moonshot" | "kimi_coding" | "siliconflow" | "zhipu" | "zai" => {
            parse_official_api_key_quota(provider_type, &body)
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

    let code = code.unwrap_or_else(|| "unknown".into());
    let numeric_code = code.parse::<u16>().ok();
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
