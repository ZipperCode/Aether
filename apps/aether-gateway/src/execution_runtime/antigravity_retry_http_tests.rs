use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aether_ai_serving::{AiStreamAttempt, AiSyncAttempt};
use aether_contracts::{ExecutionPlan, ExecutionTimeouts};
use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::candidates::InMemoryRequestCandidateRepository;
use aether_data::repository::provider_catalog::InMemoryProviderCatalogReadRepository;
use aether_data::repository::usage::InMemoryUsageReadRepository;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_usage_runtime::UsageRuntimeConfig;
use axum::body::to_bytes;
use serde_json::{json, Value};

use crate::control::GatewayControlDecision;
use crate::execution_runtime::antigravity_signature::tests::{
    context, signature_plan, signature_server_with_rejection,
};
use crate::executor::{
    execute_stream_plan_and_reports, execute_sync_plan_and_reports, LocalExecutionRequestOutcome,
};
use crate::AppState;

fn run_http_test(future: impl Future<Output = ()> + Send + 'static) {
    let thread = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(future);
        })
        .unwrap();
    if let Err(error) = thread.join() {
        std::panic::resume_unwind(error);
    }
}

fn retry_rejection(delay: &str) -> Value {
    json!({"error": {
        "code": 429,
        "status": "RESOURCE_EXHAUSTED",
        "message": "short-lived rate limit",
        "details": [{
            "@type": "type.googleapis.com/google.rpc.RetryInfo",
            "retryDelay": delay,
        }],
    }})
}

fn make_plans(stream: bool, url: &str, count: usize) -> Vec<ExecutionPlan> {
    (0..count)
        .map(|index| {
            let mut plan = signature_plan(stream, url);
            plan.provider_name = Some("antigravity".to_string());
            if index > 0 {
                plan.provider_id = format!("{}-{index}", plan.provider_id);
                plan.endpoint_id = format!("{}-{index}", plan.endpoint_id);
                plan.key_id = format!("{}-{index}", plan.key_id);
                plan.headers.insert(
                    "authorization".to_string(),
                    format!("Bearer signature-test-{index}"),
                );
            }
            plan
        })
        .collect()
}

fn normalize_request_identity(plans: &mut [ExecutionPlan]) {
    let request_id = plans[0].request_id.clone();
    for plan in plans.iter_mut().skip(1) {
        plan.request_id = request_id.clone();
    }
}

fn test_state(plans: &[ExecutionPlan], enabled: bool, max_delay_ms: u64) -> AppState {
    test_state_with_pause(plans, enabled, max_delay_ms, false)
}

fn test_state_with_pause(
    plans: &[ExecutionPlan],
    enabled: bool,
    max_delay_ms: u64,
    pause: bool,
) -> AppState {
    let credential_state = AppState::new()
        .expect("credential state should build")
        .with_data_state_for_tests(
            crate::data::GatewayDataState::disabled()
                .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
        );
    let mut providers = Vec::new();
    let mut endpoints = Vec::new();
    let mut keys = Vec::new();
    for plan in plans {
        let encrypted_api_key = credential_state
            .seal_provider_catalog_key_api_key(
                &plan.provider_id,
                &plan.key_id,
                "plain-upstream-key",
            )
            .expect("api key should encrypt");
        let provider = StoredProviderCatalogProvider::new(
            plan.provider_id.clone(),
            plan.provider_id.clone(),
            Some("https://provider.example".to_string()),
            "antigravity".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            false,
            None,
            Some(3),
            None,
            None,
            None,
            Some(json!({"pool_advanced": {"antigravity": {
                "short_retry_enabled": enabled,
                "short_retry_max_delay_ms": max_delay_ms,
                "tos_pause_enabled": pause,
                "tos_pause_seconds": 900,
            }}})),
        );
        let endpoint = StoredProviderCatalogEndpoint::new(
            plan.endpoint_id.clone(),
            plan.provider_id.clone(),
            plan.provider_api_format.clone(),
            None,
            None,
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(
            plan.url.clone(),
            None,
            None,
            Some(2),
            None,
            None,
            None,
            None,
        )
        .expect("endpoint transport should build");
        let key = StoredProviderCatalogKey::new(
            plan.key_id.clone(),
            plan.provider_id.clone(),
            plan.key_id.clone(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(json!([plan.provider_api_format.clone()])),
            encrypted_api_key,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build");
        providers.push(provider);
        endpoints.push(endpoint);
        keys.push(key);
    }
    let catalog = InMemoryProviderCatalogReadRepository::seed(providers, endpoints, keys);
    AppState::new()
        .expect("app state should build")
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_request_candidate_and_usage_repository_for_tests(
                Arc::new(InMemoryRequestCandidateRepository::default()),
                Arc::new(InMemoryUsageReadRepository::default()),
            )
            .with_provider_catalog_reader(Arc::new(catalog))
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
        )
        .with_usage_runtime_for_tests(UsageRuntimeConfig {
            enabled: true,
            ..Default::default()
        })
}

fn decision() -> GatewayControlDecision {
    GatewayControlDecision::synthetic(
        "/v1/chat/completions",
        Some("ai_public".to_string()),
        Some("antigravity".to_string()),
        Some("chat".to_string()),
        Some("gemini:generate_content".to_string()),
    )
    .with_execution_runtime_candidate(true)
}

fn report_context(plan: &ExecutionPlan, candidate_index: usize) -> Option<Value> {
    let mut report = context(plan).expect("Antigravity context");
    report["candidate_index"] = json!(candidate_index);
    report["retry_index"] = json!(0);
    Some(report)
}

async fn run_sync(plans: &[ExecutionPlan], state: &AppState) -> LocalExecutionRequestOutcome {
    let attempts = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| AiSyncAttempt {
            plan: plan.clone(),
            report_kind: None,
            report_context: report_context(plan, index),
        })
        .collect();
    let request = http::Request::builder()
        .uri("/v1beta/models/gemini-test:generateContent")
        .body(())
        .expect("test request should build");
    let (parts, _) = request.into_parts();
    execute_sync_plan_and_reports(
        state,
        &parts,
        &plans[0].request_id,
        &decision(),
        "gemini_chat_sync",
        attempts,
    )
    .await
    .expect("sync candidate loop should execute")
}

async fn run_stream(plans: &[ExecutionPlan], state: &AppState) -> LocalExecutionRequestOutcome {
    let attempts = plans
        .iter()
        .enumerate()
        .map(|(index, plan)| AiStreamAttempt {
            plan: plan.clone(),
            report_kind: None,
            report_context: report_context(plan, index),
        })
        .collect();
    execute_stream_plan_and_reports(
        state,
        &plans[0].request_id,
        &decision(),
        "gemini_chat_stream",
        attempts,
    )
    .await
    .expect("stream candidate loop should execute")
}

async fn assert_ok(outcome: LocalExecutionRequestOutcome) {
    let LocalExecutionRequestOutcome::Responded(response) = outcome else {
        panic!("RetryInfo recovery should return a response");
    };
    assert_eq!(response.status(), http::StatusCode::OK);
    let _ = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body should complete");
}

#[test]
fn sync_short_retry_uses_budget_once_then_recovers_or_fails_over() {
    run_http_test(sync_short_retry_impl());
}
async fn sync_short_retry_impl() {
    let server =
        signature_server_with_rejection(false, vec![429, 200], retry_rejection("0.025s")).await;
    let plans = make_plans(false, &server.url, 1);
    let state = test_state(&plans, true, 2_000);
    let started_at = Instant::now();
    assert_ok(run_sync(&plans, &state).await).await;
    assert!(started_at.elapsed() >= Duration::from_millis(20));
    assert_eq!(server.requests.lock().unwrap().len(), 2);

    let server =
        signature_server_with_rejection(false, vec![429, 429, 200], retry_rejection("0.025s"))
            .await;
    let mut plans = make_plans(false, &server.url, 2);
    normalize_request_identity(&mut plans);
    let state = test_state(&plans, true, 2_000);
    assert_ok(run_sync(&plans, &state).await).await;
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 3, "one same-key retry, then one failover");
    assert_eq!(
        requests[0].0.get("authorization"),
        requests[1].0.get("authorization")
    );
    assert_ne!(
        requests[1].0.get("authorization"),
        requests[2].0.get("authorization")
    );
}

#[test]
fn stream_short_retry_uses_budget_once_then_recovers_or_fails_over() {
    run_http_test(stream_short_retry_impl());
}
async fn stream_short_retry_impl() {
    let server =
        signature_server_with_rejection(true, vec![429, 200], retry_rejection("0.025s")).await;
    let plans = make_plans(true, &server.url, 1);
    let state = test_state(&plans, true, 2_000);
    let started_at = Instant::now();
    assert_ok(run_stream(&plans, &state).await).await;
    assert!(started_at.elapsed() >= Duration::from_millis(20));
    assert_eq!(server.requests.lock().unwrap().len(), 2);

    let server =
        signature_server_with_rejection(true, vec![429, 429, 200], retry_rejection("0.025s")).await;
    let mut plans = make_plans(true, &server.url, 2);
    normalize_request_identity(&mut plans);
    let state = test_state(&plans, true, 2_000);
    assert_ok(run_stream(&plans, &state).await).await;
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 3, "one same-key retry, then one failover");
    assert_eq!(
        requests[0].0.get("authorization"),
        requests[1].0.get("authorization")
    );
    assert_ne!(
        requests[1].0.get("authorization"),
        requests[2].0.get("authorization")
    );
}

async fn assert_sync_not_retried(
    enabled: bool,
    max_delay_ms: u64,
    delay: &str,
    total_ms: Option<u64>,
) {
    let server =
        signature_server_with_rejection(false, vec![429, 200], retry_rejection(delay)).await;
    let mut plans = make_plans(false, &server.url, 1);
    plans[0].timeouts = total_ms.map(|total_ms| ExecutionTimeouts {
        total_ms: Some(total_ms),
        ..Default::default()
    });
    let state = test_state(&plans, enabled, max_delay_ms);
    assert!(
        !super::wait_for_short_retry(
            &state,
            &plans[0],
            report_context(&plans[0], 0).as_ref(),
            429,
            Some(&retry_rejection(delay)),
            None,
            Instant::now(),
            false,
        )
        .await
    );
    let _ = run_sync(&plans, &state).await;
    assert_eq!(
        server.requests.lock().unwrap().len(),
        2,
        "existing candidate retry remains unchanged"
    );
}

async fn assert_stream_not_retried(
    enabled: bool,
    max_delay_ms: u64,
    delay: &str,
    first_byte_ms: Option<u64>,
) {
    let server =
        signature_server_with_rejection(true, vec![429, 200], retry_rejection(delay)).await;
    let mut plans = make_plans(true, &server.url, 1);
    plans[0].timeouts = first_byte_ms.map(|first_byte_ms| ExecutionTimeouts {
        first_byte_ms: Some(first_byte_ms),
        ..Default::default()
    });
    let state = test_state(&plans, enabled, max_delay_ms);
    assert!(
        !super::wait_for_short_retry(
            &state,
            &plans[0],
            report_context(&plans[0], 0).as_ref(),
            429,
            Some(&retry_rejection(delay)),
            None,
            Instant::now(),
            false,
        )
        .await
    );
    let _ = run_stream(&plans, &state).await;
    assert_eq!(
        server.requests.lock().unwrap().len(),
        2,
        "existing candidate retry remains unchanged"
    );
}

#[test]
fn sync_default_off_long_delay_invalid_config_and_deadline_do_not_retry() {
    run_http_test(sync_no_retry_impl());
}
async fn sync_no_retry_impl() {
    assert_sync_not_retried(false, 2_000, "0.025s", None).await;
    assert_sync_not_retried(true, 10, "0.025s", None).await;
    assert_sync_not_retried(true, 7_001, "0.025s", None).await;
    assert_sync_not_retried(true, 2_000, "1.5s", Some(1_000)).await;
}

#[test]
fn stream_default_off_long_delay_invalid_config_and_deadline_do_not_retry() {
    run_http_test(stream_no_retry_impl());
}
async fn stream_no_retry_impl() {
    assert_stream_not_retried(false, 2_000, "0.025s", None).await;
    assert_stream_not_retried(true, 10, "0.025s", None).await;
    assert_stream_not_retried(true, 7_001, "0.025s", None).await;
    assert_stream_not_retried(true, 2_000, "1.5s", Some(1_000)).await;
}

#[tokio::test]
async fn committed_output_zero_budget_and_auth_status_skip_before_waiting() {
    let plan = signature_plan(false, "http://localhost/");
    let mut report = report_context(&plan, 0);
    let body = retry_rejection("0s");
    let state = AppState::new().expect("app state should build");
    assert!(
        !super::wait_for_short_retry(
            &state,
            &plan,
            report.as_ref(),
            429,
            Some(&body),
            None,
            Instant::now(),
            true,
        )
        .await
    );
    report.as_mut().unwrap()["sticky_key_attempts"] = json!(0);
    assert!(
        !super::wait_for_short_retry(
            &state,
            &plan,
            report.as_ref(),
            429,
            Some(&body),
            None,
            Instant::now(),
            false,
        )
        .await
    );
    assert!(
        !super::wait_for_short_retry(
            &state,
            &plan,
            report_context(&plan, 0).as_ref(),
            403,
            Some(&body),
            None,
            Instant::now(),
            false,
        )
        .await
    );
}

#[test]
fn tos_pause_stops_sync_and_stream_and_blocks_later_materialized_requests() {
    run_http_test(tos_pause_http_impl());
}
async fn tos_pause_http_impl() {
    for stream in [false, true] {
        let body = json!({"error": {"code":403,"status":"PERMISSION_DENIED","details":[{
            "@type":"type.googleapis.com/google.rpc.ErrorInfo", "reason":"TOS_VIOLATION"
        }]}});
        let server = signature_server_with_rejection(stream, vec![403, 200], body).await;
        let candidates = make_plans(stream, &server.url, 1);
        let state = test_state_with_pause(&candidates, false, 2000, true);
        let _ = if stream {
            run_stream(&candidates, &state).await
        } else {
            run_sync(&candidates, &state).await
        };
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        assert!(
            crate::orchestration::antigravity_pool_pause_blocks_plan(&state, &candidates[0])
                .await
                .unwrap()
        );
        let _ = if stream {
            run_stream(&candidates, &state).await
        } else {
            run_sync(&candidates, &state).await
        };
        assert_eq!(
            server.requests.lock().unwrap().len(),
            1,
            "paused provider must not send a materialized request"
        );
        crate::orchestration::clear_antigravity_pool_pause(&state, &candidates[0].provider_id)
            .await
            .unwrap();
        assert_ok(if stream {
            run_stream(&candidates, &state).await
        } else {
            run_sync(&candidates, &state).await
        })
        .await;
        assert_eq!(server.requests.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn derived_short_retry_keeps_elapsed_time_in_original_budget() {
    for stream in [false, true] {
        let mut candidates = make_plans(stream, "http://localhost/", 1);
        candidates[0].timeouts = Some(ExecutionTimeouts {
            total_ms: Some(1000),
            first_byte_ms: Some(1000),
            ..Default::default()
        });
        let state = test_state(&candidates, true, 2000);
        let mut context = report_context(&candidates[0], 0).unwrap();
        context["retry_index"] = json!(1);
        let adjusted = super::plan_with_remaining_budget(
            &state,
            &candidates[0],
            Some(&context),
            Duration::from_millis(600),
        )
        .await
        .unwrap();
        let timeouts = adjusted.timeouts.unwrap();
        assert_eq!(
            if stream {
                timeouts.first_byte_ms
            } else {
                timeouts.total_ms
            },
            Some(400)
        );
        assert!(super::plan_with_remaining_budget(
            &state,
            &candidates[0],
            Some(&context),
            Duration::from_millis(1000)
        )
        .await
        .is_none());
    }
}
