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
    run_stream_failover("chat", "error").await;
}

/// HTTP 分块落在错误 JSON 中间时，不能把半个事件认作成功而停止候选切换。
#[tokio::test]
async fn chat_stream_failover_waits_for_split_first_error() {
    run_stream_failover("chat", "split").await;
}

/// 已输出有效内容后只能在当前流报错，不能拼接第三家回答。
#[tokio::test]
async fn chat_stream_failover_stays_on_provider_after_visible_output() {
    run_stream_failover("chat", "visible").await;
}

/// 提供商显式 stop 规则优先于首段错误的默认切换策略。
#[tokio::test]
async fn chat_stream_failover_honors_explicit_stop_rule() {
    run_stream_failover("chat", "stop").await;
}

/// 开场事件和单个空格后的 503 必须继续访问下一家，失败与成功候选均有真实 HTTP 调用证据。
#[tokio::test]
async fn responses_whitespace_failover_reaches_next_provider_after_preamble_error() {
    run_stream_failover("responses", "error").await;
}

/// 空白增量跨 HTTP 分片时也不能提前提交成功。
#[tokio::test]
async fn responses_whitespace_failover_waits_for_fragmented_text_event() {
    run_stream_failover("responses", "split").await;
}

/// 同一供应商成功时原样交付开场空白和未知字段，不能丢失或重复预读字节。
#[tokio::test]
async fn responses_whitespace_success_preserves_original_stream_once() {
    run_stream_failover("responses", "success").await;
}

/// 空白之后出现真实文本即固定当前供应商，后续失败不得拼接另一家回答。
#[tokio::test]
async fn responses_whitespace_failover_stops_after_meaningful_text() {
    run_stream_failover("responses", "visible").await;
}

/// 函数调用建立后同样固定当前供应商，即使参数尚未输出。
#[tokio::test]
async fn responses_whitespace_failover_stops_after_tool_output() {
    run_stream_failover("responses", "tool").await;
}

/// 共用真实 HTTP、动态候选与 usage 结算夹具；Chat 保留三家粘性重试，Responses 使用两家隔离开场空白。
async fn run_stream_failover(api_kind: &'static str, mode: &'static str) {
    let responses = api_kind == "responses";
    let api_format = if responses {
        "openai:responses"
    } else {
        "openai:chat"
    };
    let endpoint_kind = if responses { "cli" } else { "chat" };
    let upstream_path = if responses {
        "/responses"
    } else {
        "/chat/completions"
    };
    let last_provider = if responses { 1 } else { 2 };
    let mut state = AppState::new().unwrap().with_data_state_for_tests(
        crate::data::GatewayDataState::disabled()
            .with_encryption_key_for_tests(DEVELOPMENT_ENCRYPTION_KEY),
    );
    // 每个场景使用独立目标准入表，以固定测试压力驱动真实 target-select，不依赖进程环境。
    state.upstream_target_admission = Arc::new(
        crate::upstream_admission::UpstreamTargetAdmission::new(Some(16), Duration::from_secs(1)),
    );
    let error = if responses {
        "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp-whitespace\",\"status\":\"failed\",\"error\":{\"type\":\"server_error\",\"message\":\"capacity exhausted\",\"code\":503}}}\n\n"
    } else {
        "data: {\"error\":{\"type\":\"server_error\",\"message\":\"NVIDIA returned HTTP 400\",\"code\":400}}\n\n"
    };
    let split_error = "event: error\r\ndata:{\"error\":\r\ndata:{\"type\":\"server_error\",\"message\":\"NVIDIA returned HTTP 400\",\"code\":400}}\r\n\r\n";
    let visible = if responses {
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg-whitespace\",\"output_index\":0,\"content_index\":0,\"delta\":\"first answer\"}\n\n"
    } else {
        "data: {\"id\":\"chatcmpl-second\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"second answer\"},\"finish_reason\":null}]}\n\n"
    };
    let chat_success = concat!(
        "data: {\"id\":\"chatcmpl-third\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"opaque\":{\"keep\":true},\"choices\":[{\"index\":0,\"delta\":{\"content\":\"third answer\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-third\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-5\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
        "data: [DONE]\n\n",
    );
    let setup = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-whitespace\",\"object\":\"response\",\"model\":\"gpt-5\",\"status\":\"in_progress\",\"output\":[]},\"opaque\":{\"keep\":true}}\n\n",
        "event: response.in_progress\ndata: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"resp-whitespace\",\"status\":\"in_progress\"}}\n\n",
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"msg-whitespace\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[]}}\n\n",
        "event: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"item_id\":\"msg-whitespace\",\"output_index\":0,\"content_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"\",\"annotations\":[]}}\n\n",
    );
    let whitespace = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg-whitespace\",\"output_index\":0,\"content_index\":0,\"delta\":\" \"}\n\n";
    // 分片切在增量字段前，供应商身份标记保持完整，仍覆盖跨分片的空白事件。
    let whitespace_split = whitespace.find("\"delta\":").unwrap();
    let tool = "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"fc-1\",\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"lookup\",\"arguments\":\"\"}}\n\n";
    // 完整原生终态不需要兼容层补字段，因此可以严格比较上游与客户端的原始字节。
    let responses_success = concat!(
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg-whitespace\",\"output_index\":0,\"content_index\":0,\"delta\":\"successful answer\",\"opaque\":{\"keep\":true}}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-whitespace\",\"object\":\"response\",\"model\":\"gpt-5\",\"status\":\"completed\",\"created_at\":123,\"completed_at\":123,\"output_text\":\" successful answer\",\"output\":[{\"id\":\"msg-whitespace\",\"type\":\"message\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\" successful answer\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"total_tokens\":5}}}\n\n",
    );
    let success_chunks = if responses {
        vec![setup, whitespace, responses_success]
    } else {
        vec![chat_success]
    };
    let success = success_chunks.concat();
    // 故障场景只改变语义事件的位置，保留同一传输和路由配置以比较提交边界。
    let failure_chunks = match (responses, mode) {
        (true, "split") => vec![
            setup,
            &whitespace[..whitespace_split],
            &whitespace[whitespace_split..],
            error,
        ],
        (true, "visible") => vec![setup, whitespace, visible, error],
        (true, "tool") => vec![setup, whitespace, tool, error],
        (true, _) => vec![setup, whitespace, error],
        (false, "split") => vec![
            ": ping\r\n\r\nid: first\r\nretry: 1000\r\n\r\n",
            &split_error[..55],
            &split_error[55..],
        ],
        (false, "visible") => vec![visible, error],
        (false, _) => vec![error],
    };
    let hits: Vec<_> = (0..=last_provider)
        .map(|_| Arc::new(AtomicUsize::new(0)))
        .collect();
    let mut servers = Vec::new();
    let mut providers = Vec::new();
    let mut endpoints = Vec::new();
    let mut keys = Vec::new();
    let mut candidates = Vec::new();
    for (index, counter) in hits.iter().enumerate() {
        let counter = Arc::clone(counter);
        let chunks = if index == last_provider || mode == "success" {
            success_chunks.clone()
        } else {
            failure_chunks.clone()
        };
        // 两家响应使用不同 response/item ID，不能把 A 的开场误认为 B 成功流的一部分。
        let chunks: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                if responses {
                    Bytes::from(chunk.replace("whitespace", &format!("whitespace-{index}")))
                } else {
                    Bytes::from_static(chunk.as_bytes())
                }
            })
            .collect();
        let app = Router::new().route(upstream_path, post(move || {
            let counter = Arc::clone(&counter);
            let chunks = chunks.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if !responses && index == 0 {
                    return Response::builder().status(400).header("content-type", "application/json")
                        .body(Body::from("{\"error\":{\"type\":\"server_error\",\"message\":\"first provider failed\"}}"))
                        .unwrap();
                }
                let stream = async_stream::stream! {
                    for (part, chunk) in chunks.into_iter().enumerate() {
                        if part > 0 {
                            tokio::time::sleep(Duration::from_millis(30)).await;
                        }
                        yield Ok::<_, Infallible>(chunk);
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
        let provider_id = format!("provider-{api_kind}-{mode}-{index}");
        let endpoint_id = format!("endpoint-{api_kind}-{mode}-{index}");
        let key_id = format!("key-{api_kind}-{mode}-{index}");
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
                api_format.into(),
                Some("openai".into()),
                Some(endpoint_kind.into()),
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
                Some(json!([api_format])),
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
            endpoint_api_format: api_format.into(),
            endpoint_api_family: Some("openai".into()),
            endpoint_kind: Some(endpoint_kind.into()),
            endpoint_is_active: true,
            key_id,
            key_name: "local".into(),
            key_auth_type: "api_key".into(),
            key_is_active: true,
            key_api_formats: Some(vec![api_format.into()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 1,
            key_global_priority_by_format: None,
            routing_facts: Default::default(),
            model_id: format!("model-{api_kind}-{mode}-{index}"),
            global_model_id: format!("global-{api_kind}-{mode}"),
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
        format!("user-{api_kind}-{mode}"),
        "alice".into(),
        None,
        "user".into(),
        "local".into(),
        true,
        false,
        None,
        None,
        None,
        format!("client-key-{api_kind}-{mode}"),
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
            id: format!("routing-{api_kind}-{mode}"),
            name: format!("routing-{api_kind}-{mode}"),
            description: None,
            enabled: true,
            is_system_default: true,
            sort_order: 0,
            config_json: json!({"default_policy": {
                "priority_mode": "provider",
                "scheduling_mode": "fixed_order",
                "sticky_key_attempts": if responses { 1 } else { 2 },
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
    let request_id = format!("{api_kind}-failover-{mode}");
    let request_body = if responses {
        json!({"model":"gpt-5","input":[],"stream":true})
    } else {
        json!({"model":"gpt-5","messages":[],"stream":true})
    };
    let mut response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{gateway_url}/v1{upstream_path}"))
        .bearer_auth("local-client-key")
        .header(super::TRACE_ID_HEADER, &request_id)
        .json(&request_body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    // 只收集测试所需的有界响应，避免断言错误时无限积累流数据。
    let mut body_bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.unwrap() {
        assert!(body_bytes.len() + chunk.len() <= 64 * 1024);
        body_bytes.extend_from_slice(&chunk);
    }
    let body = strip_sse_keepalive_comments(&String::from_utf8(body_bytes).unwrap());
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
    let retries_next = matches!(mode, "error" | "split");
    let completes = retries_next || mode == "success";
    let final_provider = if retries_next {
        last_provider
    } else {
        last_provider - 1
    };
    let expected_hits = if responses {
        vec![1, usize::from(retries_next)]
    } else {
        vec![2, 1, usize::from(retries_next)]
    };
    assert_eq!(
        observed_hits, expected_hits,
        "provider calls; status={status}, body={body}, candidates={candidate_diagnostics:#?}"
    );
    if completes {
        assert_eq!(status, StatusCode::OK);
        let success = if responses {
            success.replace("whitespace", &format!("whitespace-{final_provider}"))
        } else {
            success
        };
        assert_eq!(
            body, success,
            "successful upstream bytes must survive once, including unknown fields"
        );
        if responses && retries_next {
            assert!(
                !body.contains("whitespace-0"),
                "failed provider preamble leaked downstream"
            );
        }
    } else if matches!(mode, "visible" | "tool") {
        assert_eq!(status, StatusCode::OK);
        let prefix = if responses {
            format!(
                "{setup}{whitespace}{}",
                if mode == "tool" { tool } else { visible }
            )
            .replace("whitespace", "whitespace-0")
        } else {
            visible.to_owned()
        };
        assert!(body.starts_with(&prefix));
        assert!(body.contains("server_error"));
        assert!(!body.contains("third answer"));
        assert!(!body.contains("successful answer"));
    } else {
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let expected_status = if completes { "completed" } else { "failed" };
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
    let final_provider_id = format!("provider-{api_kind}-{mode}-{final_provider}");
    assert_eq!(
        usage.provider_id.as_deref(),
        Some(final_provider_id.as_str())
    );
    if completes {
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
    let failed_attempts = if responses {
        usize::from(mode != "success")
    } else {
        3
    };
    assert_eq!(attempted.len(), failed_attempts + usize::from(completes));
    assert_eq!(
        attempted
            .iter()
            .filter(|candidate| candidate.status == RequestCandidateStatus::Failed)
            .count(),
        failed_attempts
    );
    assert_eq!(
        attempted
            .iter()
            .filter(|candidate| candidate.status == RequestCandidateStatus::Success)
            .count(),
        usize::from(completes)
    );
    assert!(attempted
        .iter()
        .all(|candidate| candidate.finished_at_unix_ms.is_some()));
    // 除总数外逐一验证真实供应商归属，确保 A 的 503 与 B 的成功没有写反或丢失。
    if responses {
        for index in 0..=final_provider {
            let provider_id = format!("provider-{api_kind}-{mode}-{index}");
            let candidate = attempted
                .iter()
                .find(|candidate| candidate.provider_id.as_deref() == Some(provider_id.as_str()))
                .expect("each called provider must have its own terminal candidate");
            let succeeded = completes && index == final_provider;
            assert_eq!(
                candidate.status,
                if succeeded {
                    RequestCandidateStatus::Success
                } else {
                    RequestCandidateStatus::Failed
                }
            );
            assert_eq!(
                candidate.status_code,
                Some(if succeeded { 200 } else { 503 })
            );
        }
    }
}
