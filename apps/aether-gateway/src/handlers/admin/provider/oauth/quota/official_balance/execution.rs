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
    build_openrouter_credits_request, parse_deepseek_balance, parse_official_api_key_quota,
    parse_openrouter_credits,
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
    let plan = build_provider_quota_execution_plan(
        &transport,
        spec,
        route.proxy.clone(),
        input.state.resolve_transport_profile(&transport),
        timeouts,
    );
    Ok(PreparedAttempt {
        transport,
        plan,
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
        provider_type,
        quota_kind,
        ..
    } = prepared;
    match execute_provider_quota_plan(state, &transport, plan, quota_kind.as_str()).await {
        Ok(ProviderQuotaExecutionOutcome::Response(result)) => {
            execution_result_to_attempt(result, quota_kind, &provider_type)
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
    let parsed = match provider_type {
        "deepseek" => parse_deepseek_balance(&body),
        "openrouter" => parse_openrouter_credits(&body),
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
