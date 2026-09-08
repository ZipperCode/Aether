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
    usage::UsageReadRepository,
};
use axum::{
    body::{Body, Bytes},
    routing::post,
    Router,
};
use http::{Response, StatusCode};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

/// 第二家首段错误必须继续访问第三家，同时保留第一家同 Key 两次尝试。
#[tokio::test]
async fn chat_stream_failover_reaches_third_provider_after_first_event_error() {
    run_chat_stream_failover("error").await;
}

/// HTTP 分块落在错误 JSON 中间时，不能把半个事件认作成功而停止候选切换。
#[tokio::test]
async fn chat_stream_failover_waits_for_split_first_error() {
    run_chat_stream_failover("split").await;
}

/// 已输出有效内容后只能在当前流报错，不能拼接第三家回答。
#[tokio::test]
async fn chat_stream_failover_stays_on_provider_after_visible_output() {
    run_chat_stream_failover("visible").await;
}

/// 提供商显式 stop 规则优先于首段错误的默认切换策略。
#[tokio::test]
async fn chat_stream_failover_honors_explicit_stop_rule() {
    run_chat_stream_failover("stop").await;
}

/// 使用三家真实本地 HTTP 提供商贯穿动态候选、直连传输和 usage 结算；mode 仅选择第二家的故障形态。
async fn run_chat_stream_failover(mode: &'static str) {
    let mut state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
    );
    // 每个场景使用独立目标准入表，以固定测试压力驱动真实 target-select，不依赖进程环境。
    state.upstream_target_admission = Arc::new(
        crate::upstream_admission::UpstreamTargetAdmission::new(Some(16), Duration::from_secs(1)),
    );
    let error = "data: {\"error\":{\"type\":\"server_error\",\"message\":\"NVIDIA returned HTTP 400\",\"code\":400}}\n\n";
    let split_error = "event: error\r\ndata:{\"error\":\r\ndata:{\"type\":\"server_error\",\"message\":\"NVIDIA returned HTTP 400\",\"code\":400}}\r\n\r\n";
    let visible = "data: {\"id\":\"chatcmpl-second\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"second answer\"},\"finish_reason\":null}]}\n\n";
    let success = concat!(
        "data: {\"id\":\"chatcmpl-third\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"opaque\":{\"keep\":true},\"choices\":[{\"index\":0,\"delta\":{\"content\":\"third answer\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-third\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
        "data: [DONE]\n\n",
    );
    let hits: Vec<_> = (0..3).map(|_| Arc::new(AtomicUsize::new(0))).collect();
    let mut servers = Vec::new();
    let mut providers = Vec::new();
    let mut endpoints = Vec::new();
    let mut keys = Vec::new();
    let mut candidates = Vec::new();
    for (index, counter) in hits.iter().enumerate() {
        let counter = Arc::clone(counter);
        let app = Router::new().route("/chat/completions", post(move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if index == 0 {
                    return Response::builder().status(400).header("content-type", "application/json")
                        .body(Body::from("{\"error\":{\"type\":\"server_error\",\"message\":\"first provider failed\"}}"))
                        .unwrap();
                }
                let chunks = if index == 2 {
                    vec![success]
                } else if mode == "split" {
                    vec![": ping\r\n\r\nid: first\r\nretry: 1000\r\n\r\n", &split_error[..55], &split_error[55..]]
                } else if mode == "visible" {
                    vec![visible, error]
                } else {
                    vec![error]
                };
                let stream = async_stream::stream! {
                    for (part, chunk) in chunks.into_iter().enumerate() {
                        if part > 0 {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                        }
                        yield Ok::<_, Infallible>(Bytes::from_static(chunk.as_bytes()));
                    }
                };
                Response::builder().status(200).header("content-type", "text/event-stream")
                    .body(Body::from_stream(stream)).unwrap()
            }
        }));
        let (url, server) = start_server(app).await;
        servers.push(server);
        // fixed_order 只决定候选排名；后续窗口还按目标压力选取。设置 A < B < C，保证真实三家顺序可复现。
        let target = crate::upstream_admission::upstream_target_key_from_url(&url, None).unwrap();
        for _ in 0..index * 4 {
            state
                .upstream_target_admission
                .record_preselect_for_target_key(&target);
        }
        // Provider 在途计数按 ID 存在进程级表中，每个并行场景必须使用独立身份。
        let provider_id = format!("provider-chat-{mode}-{index}");
        let endpoint_id = format!("endpoint-chat-{mode}-{index}");
        let key_id = format!("key-chat-{mode}-{index}");
        let priority = index as i32 + 1;
        // 仅第二家 stop 场景覆盖既有规则，其余场景使用默认故障转移与粘性预算。
        let config = (index == 1 && mode == "stop")
            .then(|| json!({"failover_rules":{"stop_status_codes":[400]}}));
        providers.push(
            StoredProviderCatalogProvider::new(
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
                config,
            )
            .with_routing_fields(priority),
        );
        endpoints.push(
            StoredProviderCatalogEndpoint::new(
                endpoint_id.clone(),
                provider_id.clone(),
                "openai:chat".into(),
                Some("openai".into()),
                Some("chat".into()),
                true,
            )
            .unwrap()
            .with_transport_fields(url, None, None, Some(2), None, None, None, None)
            .unwrap(),
        );
        keys.push(
            StoredProviderCatalogKey::new(
                key_id.clone(),
                provider_id.clone(),
                "local".into(),
                "api_key".into(),
                None,
                true,
            )
            .unwrap()
            .with_transport_fields(
                Some(json!(["openai:chat"])),
                state
                    .seal_provider_catalog_key_api_key(&provider_id, &key_id, "local-test-key")
                    .unwrap(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );
        candidates.push(StoredMinimalCandidateSelectionRow {
            provider_id,
            provider_name: "openai".into(),
            provider_type: "custom".into(),
            provider_priority: priority,
            provider_is_active: true,
            endpoint_id,
            endpoint_api_format: "openai:chat".into(),
            endpoint_api_family: Some("openai".into()),
            endpoint_kind: Some("chat".into()),
            endpoint_is_active: true,
            key_id,
            key_name: "local".into(),
            key_auth_type: "api_key".into(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:chat".into()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 1,
            key_global_priority_by_format: None,
            routing_facts: Default::default(),
            model_id: format!("model-chat-{mode}-{index}"),
            global_model_id: format!("global-chat-{mode}"),
            global_model_name: "gpt-5".into(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-5".into(),
            model_provider_model_mappings: None,
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        });
    }
    let auth = StoredAuthApiKeySnapshot::new(
        format!("user-chat-{mode}"),
        "alice".into(),
        None,
        "user".into(),
        "local".into(),
        true,
        false,
        None,
        None,
        None,
        format!("client-key-chat-{mode}"),
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
    .unwrap();
    let auth = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![(
        Some(format!("{:x}", Sha256::digest(b"local-client-key"))),
        auth,
    )]));
    let candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    let usage_repository = Arc::new(InMemoryUsageReadRepository::default());
    // 路由组 default_policy 是候选排名的权威来源；目标窗口的选择次序由上面的独立测试压力确定。
    let routing_repository = Arc::new(InMemoryRoutingGroupRepository::default());
    routing_repository
        .create_routing_group(CreateRoutingGroupRecord {
            id: format!("routing-chat-{mode}"),
            name: format!("routing-chat-{mode}"),
            description: None,
            enabled: true,
            is_system_default: true,
            sort_order: 0,
            config_json: json!({"default_policy": {
                "priority_mode": "provider",
                "scheduling_mode": "fixed_order",
                "sticky_key_attempts": 2,
            }}),
            version: 1,
            created_at: 1,
            updated_at: 1,
            published_at: Some(1),
        })
        .await
        .unwrap();
    let state = state.with_data_state_for_tests(
        crate::data::GatewayDataState::with_auth_candidate_selection_provider_catalog_request_candidates_and_usage_for_tests(
            auth, Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(candidates)),
            Arc::new(InMemoryProviderCatalogReadRepository::seed(providers, endpoints, keys)),
            Arc::clone(&candidate_repository), Arc::clone(&usage_repository), DEVELOPMENT_ENCRYPTION_KEY,
        ).with_routing_group_repository_for_tests(routing_repository)
        .with_system_config_values_for_tests([
            ("request_record_level".into(), json!("full")),
        ]),
    ).with_usage_runtime_for_tests(super::UsageRuntimeConfig {enabled: true, ..Default::default()});
    let (gateway_url, gateway_server) = start_server(build_router_with_state(state)).await;
    servers.push(gateway_server);
    let request_id = format!("chat-failover-{mode}");
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{gateway_url}/v1/chat/completions"))
        .bearer_auth("local-client-key")
        .header(super::TRACE_ID_HEADER, &request_id)
        .json(&json!({"model":"gpt-5","messages":[],"stream":true}))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = strip_sse_keepalive_comments(&response.text().await.unwrap());
    // 响应已消费完；停止测试拥有的监听任务，不遗留后台服务器。
    for server in servers {
        server.abort();
        let _ = server.await;
    }
    let observed_hits: Vec<_> = hits
        .iter()
        .map(|hits| hits.load(Ordering::SeqCst))
        .collect();
    let candidate_diagnostics = candidate_repository
        .list_by_request_id(&request_id)
        .await
        .unwrap();
    let retries_third = matches!(mode, "error" | "split");
    assert_eq!(
        observed_hits,
        [2, 1, usize::from(retries_third)],
        "provider calls; status={status}, body={body}, candidates={candidate_diagnostics:#?}"
    );
    if retries_third {
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body, success,
            "successful upstream bytes must survive once, including unknown fields"
        );
    } else if mode == "visible" {
        assert_eq!(status, StatusCode::OK);
        assert!(body.starts_with(visible));
        assert!(body.contains("server_error"));
        assert!(!body.contains("third answer"));
    } else {
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let expected_status = if retries_third { "completed" } else { "failed" };
    let usage = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(usage) = usage_repository
                .find_by_request_id(&request_id)
                .await
                .unwrap()
            {
                if usage.status == expected_status {
                    break usage;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("usage must reach exactly the expected terminal state");
    let final_provider_id = format!("provider-chat-{mode}-{}", if retries_third { 2 } else { 1 });
    assert_eq!(
        usage.provider_id.as_deref(),
        Some(final_provider_id.as_str())
    );
    if retries_third {
        assert_eq!(
            (usage.input_tokens, usage.output_tokens, usage.total_tokens),
            (3, 2, 5)
        );
    }
    let stored = candidate_repository
        .list_by_request_id(&request_id)
        .await
        .unwrap();
    let attempted: Vec<_> = stored
        .iter()
        .filter(|candidate| candidate.started_at_unix_ms.is_some())
        .collect();
    assert_eq!(attempted.len(), if retries_third { 4 } else { 3 });
    assert_eq!(
        attempted
            .iter()
            .filter(|candidate| candidate.status == RequestCandidateStatus::Failed)
            .count(),
        3
    );
    assert_eq!(
        attempted
            .iter()
            .filter(|candidate| candidate.status == RequestCandidateStatus::Success)
            .count(),
        usize::from(retries_third)
    );
    assert!(attempted
        .iter()
        .all(|candidate| candidate.finished_at_unix_ms.is_some()));
}
