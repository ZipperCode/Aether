use super::{build_router_with_state, start_server, strip_sse_keepalive_comments, AppState};
use aether_crypto::DEVELOPMENT_ENCRYPTION_KEY;
use aether_data::repository::{
    auth::{InMemoryAuthApiKeySnapshotRepository, StoredAuthApiKeySnapshot},
    candidate_selection::InMemoryMinimalCandidateSelectionReadRepository,
    candidates::InMemoryRequestCandidateRepository,
    provider_catalog::InMemoryProviderCatalogReadRepository,
    routing_profiles::InMemoryRoutingGroupRepository,
    usage::InMemoryUsageReadRepository,
};
use aether_data_contracts::repository::{
    candidate_selection::StoredMinimalCandidateSelectionRow,
    candidates::{RequestCandidateReadRepository, RequestCandidateStatus},
    provider_catalog::{
        StoredProviderCatalogEndpoint, StoredProviderCatalogKey, StoredProviderCatalogProvider,
    },
    routing_profiles::{CreateRoutingGroupRecord, RoutingGroupWriteRepository},
};
use axum::{
    body::{Body, Bytes},
    extract::Request,
    routing::post,
    Router,
};
use http::{header::AUTHORIZATION, Response, StatusCode};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

/// 号池首个 Key 被上游以真实 401 拒绝（含同 Key 重试共 2 次尝试）后，必须继续
/// 尝试同池下一个 Key 并成功交付；两个 Key 各自留下具体候选记录。
#[tokio::test]
async fn responses_pool_fails_over_to_next_key_after_invalid_auth_rejection() {
    run_pool_responses_scenario("failover").await;
}

/// 号池全部 Key 都被真实 401 拒绝时，必须留下每个 Key 的具体失败候选记录，
/// 且请求级 reason 是 execution_runtime_candidates_exhausted，而不是 no_local_stream_plans。
#[tokio::test]
async fn responses_pool_all_invalid_auth_keys_surface_concrete_exhausted_diagnostics() {
    run_pool_responses_scenario("all_bad").await;
}

/// 两个 Key 并发额度都被占用时，第三个请求必须返回容量语义的 429（纯容量
/// 耗尽按设计映射为 429，非 503），以 pool_group_exhausted +
/// provider_key_concurrency_limit_reached 的具体原因落库，且与真实执行耗尽
/// (execution_runtime_candidates_exhausted) 保持可区分。
#[tokio::test]
async fn responses_pool_key_concurrency_skip_stays_distinct_from_execution_exhaustion() {
    run_pool_responses_scenario("concurrency").await;
}

const FIRST_KEY_SECRET: &str = "pool-first-upstream-secret-1008";
const SECOND_KEY_SECRET: &str = "pool-second-upstream-secret-1008";
const POOL_CONCURRENCY_SKIP_REASON: &str = "provider_key_concurrency_limit_reached";

/// 共用真实 HTTP 上游的号池回归夹具。
///
/// 单个 Provider 开启 `pool_advanced`，一个 Responses Endpoint 下挂两个 Key：
/// - failover：第一个 Key 的凭证被上游以 401 拒绝，第二个 Key 成功；
/// - all_bad：两个 Key 都被 401 拒绝，请求耗尽；
/// - concurrency：两个 Key 的并发额度被前两个请求占满，第三个请求必须在
///   未执行任何 Key 的情况下给出具体的并发跳过诊断。
async fn run_pool_responses_scenario(mode: &'static str) {
    let all_bad = mode == "all_bad";
    let concurrency = mode == "concurrency";
    let mut state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
    );
    // 单一上游目标，容量按测试压力固定，不依赖进程环境。
    state.upstream_target_admission = Arc::new(
        crate::upstream_admission::UpstreamTargetAdmission::new(Some(16), Duration::from_secs(1)),
    );

    let setup = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-pool\",\"object\":\"response\",\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
        "event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"resp-pool\",\"status\":\"in_progress\"}}\n\n",
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"msg-pool\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\n",
        "event: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"item_id\":\"msg-pool\",\"output_index\":0,\"content_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"\",\"annotations\":[]}}\n\n",
    );
    let whitespace = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg-pool\",\"output_index\":0,\"content_index\":0,\"delta\":\" \"}\n\n";
    let success_tail = concat!(
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg-pool\",\"output_index\":0,\"content_index\":0,\"delta\":\"pool answer\"}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-pool\",\"object\":\"response\",\"model\":\"gpt-5\",\"status\":\"completed\",\"created_at\":123,\"completed_at\":123,\"output_text\":\" pool answer\",\"output\":[{\"id\":\"msg-pool\",\"type\":\"message\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\" pool answer\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
    );
    let success_chunks: Vec<Bytes> = vec![setup, whitespace, success_tail]
        .into_iter()
        .map(|chunk| Bytes::from_static(chunk.as_bytes()))
        .collect();
    let expected_success_body = format!("{setup}{whitespace}{success_tail}");

    let first_hits = Arc::new(AtomicUsize::new(0));
    let second_hits = Arc::new(AtomicUsize::new(0));
    let released = Arc::new(AtomicBool::new(false));
    // 在闭包捕获边界克隆：move 闭包只能拿走副本，测试尾部仍可读取计数器并释放。
    let upstream_first_hits = Arc::clone(&first_hits);
    let upstream_second_hits = Arc::clone(&second_hits);
    let upstream_released = Arc::clone(&released);
    let upstream_success_chunks = success_chunks.clone();
    let app = Router::new().route(
        "/responses",
        post(move |request: Request| {
            let first_hits = Arc::clone(&upstream_first_hits);
            let second_hits = Arc::clone(&upstream_second_hits);
            let released = Arc::clone(&upstream_released);
            let success_chunks = upstream_success_chunks.clone();
            async move {
                let bearer = request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let is_first = bearer.contains(FIRST_KEY_SECRET);
                if is_first {
                    first_hits.fetch_add(1, Ordering::SeqCst);
                } else {
                    second_hits.fetch_add(1, Ordering::SeqCst);
                }
                // 未能识别的凭证按第二个 Key 处理；若网关不是以 Bearer 传递 Key，
                // failover 场景会因 first_hits 断言失败而立即暴露。
                if !concurrency && (is_first || all_bad) {
                    return Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .header("content-type", "application/json")
                        .body(Body::from(
                            "{\"error\":{\"message\":\"invalid pool api key\",\"type\":\"invalid_request_error\",\"code\":\"invalid_api_key\"}}",
                        ))
                        .unwrap();
                }
                let stream = async_stream::stream! {
                    for (part, chunk) in success_chunks.iter().enumerate() {
                        if part > 0 {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                        }
                        if concurrency && part == 1 {
                            // 首块已发出，first-byte 门槛已满足；在此占住该 Key
                            // 的并发额度直到测试释放（默认 idle 门槛 300s，远大于
                            // 本测试的占用时长）。
                            while !released.load(Ordering::SeqCst) {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        }
                        yield Ok::<_, Infallible>(chunk.clone());
                    }
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        }),
    );
    let (upstream_url, upstream_server) = start_server(app).await;

    let provider_id = format!("provider-pool-responses-{mode}");
    let endpoint_id = format!("endpoint-pool-responses-{mode}");
    let model_id = format!("model-pool-responses-{mode}");
    let first_key_id = format!("key-pool-first-{mode}");
    let second_key_id = format!("key-pool-second-{mode}");

    let providers = vec![StoredProviderCatalogProvider::new(
        provider_id.clone(),
        "openai".into(),
        None,
        "custom".into(),
    )
    .unwrap()
    .with_transport_fields(
        true,
        false,
        false,
        None,
        Some(2),
        None,
        Some(20.0),
        None,
        // pool_advanced 使候选解析把该 Provider 识别为号池组。
        Some(json!({"pool_advanced": {}})),
    )];
    let endpoints = vec![StoredProviderCatalogEndpoint::new(
        endpoint_id.clone(),
        provider_id.clone(),
        "openai:responses".into(),
        Some("openai".into()),
        Some("cli".into()),
        true,
    )
    .unwrap()
    .with_transport_fields(upstream_url, None, None, Some(2), None, None, None, None)
    .unwrap()];
    let mut first_key = StoredProviderCatalogKey::new(
        first_key_id.clone(),
        provider_id.clone(),
        "first".into(),
        "api_key".into(),
        None,
        true,
    )
    .unwrap()
    .with_transport_fields(
        Some(json!(["openai:responses"])),
        state
            .seal_provider_catalog_key_api_key(&provider_id, &first_key_id, FIRST_KEY_SECRET)
            .unwrap(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    first_key.internal_priority = 0;
    let mut second_key = StoredProviderCatalogKey::new(
        second_key_id.clone(),
        provider_id.clone(),
        "second".into(),
        "api_key".into(),
        None,
        true,
    )
    .unwrap()
    .with_transport_fields(
        Some(json!(["openai:responses"])),
        state
            .seal_provider_catalog_key_api_key(&provider_id, &second_key_id, SECOND_KEY_SECRET)
            .unwrap(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    second_key.internal_priority = 1;
    if concurrency {
        // 并发场景需要每个 Key 只有 1 个在途额度，第三个请求才会被真实跳过。
        first_key.concurrent_limit = Some(1);
        second_key.concurrent_limit = Some(1);
    }
    let keys = vec![first_key, second_key];

    let pool_row = |key_id: String, key_name: &str, key_internal_priority: i32| {
        StoredMinimalCandidateSelectionRow {
            provider_id: provider_id.clone(),
            provider_name: "openai".into(),
            provider_type: "custom".into(),
            provider_priority: 1,
            provider_is_active: true,
            endpoint_id: endpoint_id.clone(),
            endpoint_api_format: "openai:responses".into(),
            endpoint_api_family: Some("openai".into()),
            endpoint_kind: Some("cli".into()),
            endpoint_is_active: true,
            key_id,
            key_name: key_name.into(),
            key_auth_type: "api_key".into(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:responses".into()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority,
            key_global_priority_by_format: None,
            routing_facts: Default::default(),
            model_id: model_id.clone(),
            global_model_id: "global-pool-responses".into(),
            global_model_name: "gpt-5".into(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-5".into(),
            model_provider_model_mappings: None,
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    };
    let rows = vec![
        pool_row(first_key_id.clone(), "first", 0),
        pool_row(second_key_id.clone(), "second", 1),
    ];

    let auth_snapshot_for = |suffix: &str| {
        StoredAuthApiKeySnapshot::new(
            format!("user-pool-{mode}-{suffix}"),
            "alice".into(),
            None,
            "user".into(),
            "local".into(),
            true,
            false,
            None,
            None,
            None,
            format!("client-key-pool-{mode}-{suffix}"),
            None,
            true,
            false,
            false,
            None,
            None,
            Some(4_102_444_800),
            None,
            None,
            None,
        )
        .unwrap()
    };
    // 并发场景的三个请求使用互不相同的客户端 Key，避免共享客户端并发准入
    // 干扰号池 Key 级别的并发跳过判定。
    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![
        (
            Some(format!("{:x}", Sha256::digest(b"local-client-key"))),
            auth_snapshot_for("main"),
        ),
        (
            Some(format!("{:x}", Sha256::digest(b"local-client-key-2"))),
            auth_snapshot_for("second"),
        ),
        (
            Some(format!("{:x}", Sha256::digest(b"local-client-key-3"))),
            auth_snapshot_for("third"),
        ),
    ]));
    let candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    // 固定排序先 first 后 second；号池 Key 的同 Key 重试跟随 Provider 重试策略
    // （首个 Key 共 2 次尝试），sticky_key_attempts 只影响非号池候选路径。
    let routing_repository = Arc::new(InMemoryRoutingGroupRepository::default());
    routing_repository
        .create_routing_group(CreateRoutingGroupRecord {
            id: format!("routing-pool-{mode}"),
            name: format!("routing-pool-{mode}"),
            description: None,
            enabled: true,
            is_system_default: true,
            sort_order: 0,
            config_json: json!({"default_policy": {
                "priority_mode": "provider",
                "scheduling_mode": "fixed_order",
                "sticky_key_attempts": 1,
            }}),
            version: 1,
            created_at: 1,
            updated_at: 1,
            published_at: Some(1),
        })
        .await
        .unwrap();
    let state = state
        .with_data_state_for_tests(
            crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_request_candidates_and_usage_for_tests(
                auth_repository,
                Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(rows)),
                Arc::new(InMemoryProviderCatalogReadRepository::seed(providers, endpoints, keys)),
                Arc::clone(&candidate_repository),
                Arc::new(InMemoryUsageReadRepository::default()),
                DEVELOPMENT_ENCRYPTION_KEY,
            )
            .with_routing_group_repository_for_tests(routing_repository)
            .with_system_config_values_for_tests([
                ("request_record_level".into(), json!("full")),
            ]),
        )
        .with_usage_runtime_for_tests(super::UsageRuntimeConfig {
            enabled: true,
            ..Default::default()
        });
    let (gateway_url, gateway_server) = start_server(build_router_with_state(state)).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let send_request = |trace_id: String, bearer: &'static str| {
        let client = client.clone();
        let url = format!("{gateway_url}/v1/responses");
        async move {
            client
                .post(url)
                .bearer_auth(bearer)
                .header(super::TRACE_ID_HEADER, &trace_id)
                .json(&json!({"model":"gpt-5","input":[],"stream":true}))
                .send()
                .await
                .unwrap()
        }
    };

    if concurrency {
        // 网关会先预读语义内容再向客户端提交响应；前两个请求必须作为后台任务
        // 发出（不能等待响应头，否则会在语义门处超时），用上游计数器确认它们
        // 已分别占住 first/second 两个 Key 的并发额度。
        let send_and_read = |trace_id: String, bearer: &'static str| {
            let client = client.clone();
            let url = format!("{gateway_url}/v1/responses");
            async move {
                let response = client
                    .post(url)
                    .bearer_auth(bearer)
                    .header(super::TRACE_ID_HEADER, &trace_id)
                    .json(&json!({"model":"gpt-5","input":[],"stream":true}))
                    .send()
                    .await
                    .unwrap();
                read_bounded(response).await
            }
        };
        let first_trace = format!("responses-pool-{mode}-1");
        let first_task = tokio::spawn(send_and_read(first_trace.clone(), "local-client-key"));
        wait_for_counter(&first_hits, 1).await;
        let second_trace = format!("responses-pool-{mode}-2");
        let second_task = tokio::spawn(send_and_read(second_trace.clone(), "local-client-key-2"));
        wait_for_counter(&second_hits, 1).await;
        let third_trace = format!("responses-pool-{mode}-3");
        let third_response = send_request(third_trace.clone(), "local-client-key-3").await;
        let (third_status, third_body, third_miss_reason) = read_bounded(third_response).await;
        // 纯容量耗尽按设计映射为 429（handlers/proxy 的容量分类器：
        // 全部跳过原因都属于并发/RPM 容量集合时返回 429，其他候选缺失保持 503）。
        assert_eq!(third_status, StatusCode::TOO_MANY_REQUESTS, "{third_body}");
        // 未执行任何 Key 时不能落成真实执行耗尽：runtime Exhausted 只在有候选
        // 被真正尝试后出现，NoPath 诊断不允许被覆盖。
        assert_ne!(
            third_miss_reason.as_deref(),
            Some("execution_runtime_candidates_exhausted"),
            "pre-execution pool skip must not be classified as execution exhaustion"
        );
        let third_candidates = candidate_repository
            .list_by_request_id(&third_trace)
            .await
            .unwrap();
        assert!(
            third_candidates.iter().all(|candidate| {
                candidate.status != RequestCandidateStatus::Failed
                    && candidate.status != RequestCandidateStatus::Success
            }),
            "no pool key may be executed while both concurrency slots are held: {third_candidates:#?}"
        );
        let diagnostics = format!("{third_candidates:?}");
        assert!(
            diagnostics.contains(POOL_CONCURRENCY_SKIP_REASON),
            "concurrency skip must persist a concrete per-key reason: {diagnostics}"
        );
        assert!(
            diagnostics.contains("pool_group_exhaustion"),
            "pool group exhaustion must keep its aggregated diagnostic payload: {diagnostics}"
        );
        // 释放流并汇合前两个请求，确认它们各自成功。
        released.store(true, Ordering::SeqCst);
        let (first_final_status, first_final_body, _) = first_task.await.unwrap();
        assert_eq!(first_final_status, StatusCode::OK, "{first_final_body}");
        let (second_final_status, second_final_body, _) = second_task.await.unwrap();
        assert_eq!(second_final_status, StatusCode::OK, "{second_final_body}");
        assert_eq!(
            strip_sse_keepalive_comments(&first_final_body),
            expected_success_body
        );
        assert_eq!(
            strip_sse_keepalive_comments(&second_final_body),
            expected_success_body
        );
    } else {
        let trace_id = format!("responses-pool-{mode}");
        let response = send_request(trace_id.clone(), "local-client-key").await;
        let (status, body, miss_reason) = read_bounded(response).await;
        let body = strip_sse_keepalive_comments(&body);
        let stored = candidate_repository
            .list_by_request_id(&trace_id)
            .await
            .unwrap();
        let attempted: Vec<_> = stored
            .iter()
            .filter(|candidate| {
                candidate.status == RequestCandidateStatus::Failed
                    || candidate.status == RequestCandidateStatus::Success
            })
            .collect();
        let attempted_key_ids: Vec<&str> = attempted
            .iter()
            .map(|candidate| candidate.key_id.as_deref().unwrap_or_default())
            .collect();
        if all_bad {
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
            // 真实候选被尝试且全部失败后，不能再用 no_local_stream_plans 掩盖运行时耗尽。
            assert_eq!(
                miss_reason.as_deref(),
                Some("execution_runtime_candidates_exhausted"),
                "status={status}, body={body}, candidates={stored:#?}"
            );
            assert!(
                attempted_key_ids.contains(&first_key_id.as_str()),
                "first pool key must leave a concrete failure candidate: {stored:#?}"
            );
            assert!(
                attempted_key_ids.contains(&second_key_id.as_str()),
                "second pool key must leave a concrete failure candidate: {stored:#?}"
            );
            assert!(
                attempted.iter().all(|candidate| candidate.status
                    == RequestCandidateStatus::Failed
                    && candidate.status_code == Some(401)
                    && candidate.finished_at_unix_ms.is_some()),
                "every attempted pool key must fail with the real upstream 401: {stored:#?}"
            );
            assert!(
                first_hits.load(Ordering::SeqCst) >= 1 && second_hits.load(Ordering::SeqCst) >= 1,
                "both pool keys must be exercised over real HTTP: first={}, second={}",
                first_hits.load(Ordering::SeqCst),
                second_hits.load(Ordering::SeqCst)
            );
        } else {
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body, expected_success_body);
            // 号池 Key 的同 Key 重试跟随 Provider 重试策略（首个 Key 共 2 次
            // 401 尝试，含 1 条重试行），仍失败才切换到第二个 Key。
            assert_eq!(first_hits.load(Ordering::SeqCst), 2);
            assert_eq!(second_hits.load(Ordering::SeqCst), 1);
            let first_key_attempts: Vec<_> = attempted
                .iter()
                .filter(|candidate| candidate.key_id.as_deref() == Some(first_key_id.as_str()))
                .collect();
            assert_eq!(
                first_key_attempts.len(),
                2,
                "first key must leave its initial attempt plus one same-key retry row: {stored:#?}"
            );
            assert!(first_key_attempts.iter().all(|candidate| {
                candidate.status == RequestCandidateStatus::Failed
                    && candidate.status_code == Some(401)
                    && candidate.finished_at_unix_ms.is_some()
            }));
            let second_key_attempts: Vec<_> = attempted
                .iter()
                .filter(|candidate| candidate.key_id.as_deref() == Some(second_key_id.as_str()))
                .collect();
            assert_eq!(
                second_key_attempts.len(),
                1,
                "second key must succeed on its first attempt: {stored:#?}"
            );
            assert_eq!(
                second_key_attempts[0].status,
                RequestCandidateStatus::Success
            );
            assert_eq!(second_key_attempts[0].status_code, Some(200));
        }
    }

    gateway_server.abort();
    let _ = gateway_server.await;
    upstream_server.abort();
    let _ = upstream_server.await;
}

async fn read_bounded(mut response: reqwest::Response) -> (StatusCode, String, Option<String>) {
    let status = response.status();
    let miss_reason = response
        .headers()
        .get(super::LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    // 只收集测试所需的有界响应，避免断言错误时无限积累流数据。
    let mut body_bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.unwrap() {
        assert!(body_bytes.len() + chunk.len() <= 64 * 1024);
        body_bytes.extend_from_slice(&chunk);
    }
    (status, String::from_utf8(body_bytes).unwrap(), miss_reason)
}

async fn wait_for_counter(counter: &AtomicUsize, target: usize) {
    for _ in 0..600 {
        if counter.load(Ordering::SeqCst) >= target {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!(
        "counter did not reach {target} within 6s: {}",
        counter.load(Ordering::SeqCst)
    );
}
