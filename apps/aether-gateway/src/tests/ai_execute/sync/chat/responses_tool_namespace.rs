use crate::data::GatewayDataState;

use super::{
    any, build_router_with_state, encrypt_python_fernet_plaintext, json,
    run_async_test_on_large_stack, start_server, to_bytes, AppState, Arc, Digest,
    InMemoryAuthApiKeySnapshotRepository, InMemoryMinimalCandidateSelectionReadRepository,
    InMemoryProviderCatalogReadRepository, InMemoryRequestCandidateRepository, Json, Mutex,
    Request, RequestCandidateReadRepository, Router, Sha256, StatusCode, StoredAuthApiKeySnapshot,
    StoredMinimalCandidateSelectionRow, StoredProviderCatalogEndpoint, StoredProviderCatalogKey,
    StoredProviderCatalogProvider, StoredProviderModelMapping, DEVELOPMENT_ENCRYPTION_KEY,
    EXECUTION_PATH_EXECUTION_RUNTIME_SYNC, EXECUTION_PATH_HEADER,
    LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER, TRACE_ID_HEADER,
};

large_stack_async_test!(
    gateway_restores_discovered_responses_namespace_across_previous_response_id_continuation,
    gateway_restores_discovered_responses_namespace_across_previous_response_id_continuation_impl
);

/// End-to-end Responses -> Chat continuation:
///
/// 1. Turn one declares a client-executed `tool_search` tool and carries the
///    client's `tool_search_output` result (namespace `crm` / function
///    `lookup`) in its input. The gateway records the scoped response history.
/// 2. Turn two continues with `previous_response_id` and no repeated discovery
///    output. The planner expands the hydrated history, offers the discovered
///    child to the Chat provider as `lookup`, and the response conversion must
///    put the Responses namespace back on the consumer-visible function call
///    from the pinned tool identity snapshot.
///
/// The same continuation replayed under a different API key must fail closed:
/// the scoped history is not visible to another key and no second upstream
/// request may be issued.
async fn gateway_restores_discovered_responses_namespace_across_previous_response_id_continuation_impl(
) {
    fn hash_api_key(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn sample_auth_snapshot(api_key_id: &str, user_id: &str) -> StoredAuthApiKeySnapshot {
        StoredAuthApiKeySnapshot::new(
            user_id.to_string(),
            "alice".to_string(),
            Some("alice@example.com".to_string()),
            "user".to_string(),
            "local".to_string(),
            true,
            false,
            Some(json!(["openai"])),
            Some(json!(["openai:responses"])),
            Some(json!(["gpt-5"])),
            api_key_id.to_string(),
            Some("default".to_string()),
            true,
            false,
            false,
            Some(60),
            Some(5),
            Some(4_102_444_800_i64),
            Some(json!(["openai"])),
            Some(json!(["openai:responses"])),
            Some(json!(["gpt-5"])),
        )
        .expect("auth snapshot should build")
    }

    fn sample_candidate_row() -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: "provider-responses-namespace-1".to_string(),
            provider_name: "openai".to_string(),
            provider_type: "custom".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: "endpoint-responses-namespace-1".to_string(),
            endpoint_api_format: "openai:chat".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("chat".to_string()),
            endpoint_is_active: true,
            key_id: "key-responses-namespace-1".to_string(),
            key_name: "prod".to_string(),
            key_auth_type: "api_key".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:chat".to_string()]),
            key_allowed_models: None,
            key_capabilities: None,
            key_internal_priority: 5,
            key_global_priority_by_format: Some(json!({"openai:chat": 1})),
            routing_facts: Default::default(),
            model_id: "model-responses-namespace-1".to_string(),
            global_model_id: "global-model-responses-namespace-1".to_string(),
            global_model_name: "gpt-5".to_string(),
            global_model_mappings: None,
            global_model_supports_streaming: Some(true),
            model_provider_model_name: "gpt-5-upstream".to_string(),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: "gpt-5-upstream".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:chat".to_string()]),
                endpoint_ids: None,
                operations: None,
            }]),
            model_supports_streaming: Some(true),
            model_is_active: true,
            model_is_available: true,
        }
    }

    fn sample_provider_catalog_provider() -> StoredProviderCatalogProvider {
        StoredProviderCatalogProvider::new(
            "provider-responses-namespace-1".to_string(),
            "openai".to_string(),
            Some("https://example.com".to_string()),
            "custom".to_string(),
        )
        .expect("provider should build")
        .with_transport_fields(
            true,
            false,
            true,
            None,
            Some(2),
            None,
            Some(20.0),
            None,
            None,
        )
    }

    fn sample_provider_catalog_endpoint(base_url: String) -> StoredProviderCatalogEndpoint {
        StoredProviderCatalogEndpoint::new(
            "endpoint-responses-namespace-1".to_string(),
            "provider-responses-namespace-1".to_string(),
            "openai:chat".to_string(),
            Some("openai".to_string()),
            Some("chat".to_string()),
            true,
        )
        .expect("endpoint should build")
        .with_transport_fields(base_url, None, None, Some(2), None, None, None, None)
        .expect("endpoint transport should build")
    }

    fn sample_provider_catalog_key() -> StoredProviderCatalogKey {
        StoredProviderCatalogKey::new(
            "key-responses-namespace-1".to_string(),
            "provider-responses-namespace-1".to_string(),
            "prod".to_string(),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("key should build")
        .with_transport_fields(
            Some(json!(["openai:chat"])),
            encrypt_python_fernet_plaintext(
                DEVELOPMENT_ENCRYPTION_KEY,
                "sk-upstream-responses-namespace",
            )
            .expect("api key should encrypt"),
            None,
            None,
            Some(json!({"openai:chat": 1})),
            None,
            None,
            None,
            None,
        )
        .expect("key transport should build")
    }

    fn tool_search_tools() -> serde_json::Value {
        json!([{
            "type": "tool_search",
            "execution": "client",
            "description": "Find tools",
            "parameters": {"type": "object", "properties": {"goal": {"type": "string"}}}
        }])
    }

    fn discovery_items() -> serde_json::Value {
        json!([
            {
                "type": "tool_search_call",
                "call_id": "call_search_resp_namespace_t1",
                "execution": "client",
                "arguments": {"goal": "crm"}
            },
            {
                "type": "tool_search_output",
                "call_id": "call_search_resp_namespace_t1",
                "execution": "client",
                "tools": [{
                    "type": "namespace",
                    "name": "crm",
                    "description": "CRM tools",
                    "tools": [{
                        "type": "function",
                        "name": "lookup",
                        "parameters": {
                            "type": "object",
                            "properties": {"q": {"type": "string"}}
                        }
                    }]
                }]
            }
        ])
    }

    let seen_chat_requests = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen_chat_requests_clone = Arc::clone(&seen_chat_requests);

    let provider = Router::new().route(
        "/v1/chat/completions",
        any(move |request: Request| {
            let seen_chat_requests_inner = Arc::clone(&seen_chat_requests_clone);
            async move {
                let (_parts, body) = request.into_parts();
                let raw_body = to_bytes(body, usize::MAX).await.expect("body should read");
                let payload: serde_json::Value =
                    serde_json::from_slice(&raw_body).expect("upstream payload should parse");
                let continuation = payload.to_string().contains("look up the customer");
                seen_chat_requests_inner
                    .lock()
                    .expect("mutex should lock")
                    .push(payload);
                let body = if continuation {
                    json!({
                        "id": "chatcmpl-resp-namespace-continuation-t2",
                        "object": "chat.completion",
                        "model": "gpt-5-upstream",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": null,
                                "tool_calls": [{
                                    "id": "call_lookup_resp_namespace_t2",
                                    "type": "function",
                                    "function": {
                                        "name": "lookup",
                                        "arguments": "{\"q\":\"42\"}"
                                    }
                                }]
                            },
                            "finish_reason": "tool_calls"
                        }],
                        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
                    })
                } else {
                    json!({
                        "id": "chatcmpl-resp-namespace-continuation-t1",
                        "object": "chat.completion",
                        "model": "gpt-5-upstream",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "crm tools discovered"
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
                    })
                };
                Json(body)
            }
        }),
    );

    let (provider_url, provider_handle) = start_server(provider).await;

    let auth_repository = Arc::new(InMemoryAuthApiKeySnapshotRepository::seed(vec![
        (
            Some(hash_api_key("sk-client-responses-namespace")),
            sample_auth_snapshot(
                "api-key-responses-namespace-1",
                "user-responses-namespace-1",
            ),
        ),
        (
            Some(hash_api_key("sk-client-responses-namespace-other")),
            sample_auth_snapshot(
                "api-key-responses-namespace-other",
                "user-responses-namespace-other",
            ),
        ),
    ]));
    let candidate_selection_repository =
        Arc::new(InMemoryMinimalCandidateSelectionReadRepository::seed(vec![
            sample_candidate_row(),
        ]));
    let provider_catalog_repository = Arc::new(InMemoryProviderCatalogReadRepository::seed(
        vec![sample_provider_catalog_provider()],
        vec![sample_provider_catalog_endpoint(format!(
            "{provider_url}/v1"
        ))],
        vec![sample_provider_catalog_key()],
    ));
    let request_candidate_repository = Arc::new(InMemoryRequestCandidateRepository::default());
    // Responses local planning resolves request routing policy before candidate
    // preselection (the Chat-only local path does not), so the fixture must
    // publish a system default routing group or the planner fails closed with
    // `NoDefault` before any candidate is considered.
    let gateway_state = AppState::new()
        .expect("gateway state should build")
        .with_data_state_for_tests(
            GatewayDataState::with_auth_candidate_selection_provider_catalog_and_request_candidate_repository_for_tests(
                auth_repository,
                candidate_selection_repository,
                provider_catalog_repository,
                Arc::clone(&request_candidate_repository),
                DEVELOPMENT_ENCRYPTION_KEY,
            )
            .with_system_default_routing_group_for_tests(),
        );
    let gateway = build_router_with_state(gateway_state);
    let (gateway_url, gateway_handle) = start_server(gateway).await;
    let client = reqwest::Client::new();

    // Turn one: the client already fed the `tool_search_output` discovery result
    // (namespace `crm` / function `lookup`) back with its request, and the model
    // answers with a plain message. The gateway records the scoped history that
    // turn two must replay.
    let discovery = discovery_items();
    let turn_one_body = json!({
        "model": "gpt-5",
        "tools": tool_search_tools(),
        "input": [
            {"role": "user", "content": "find crm tools"},
            discovery[0].clone(),
            discovery[1].clone(),
        ]
    });
    let turn_one = client
        .post(format!("{gateway_url}/v1/responses"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            "Bearer sk-client-responses-namespace",
        )
        .header(TRACE_ID_HEADER, "trace-responses-namespace-t1")
        .body(turn_one_body.to_string())
        .send()
        .await
        .expect("turn one should succeed");
    let turn_one_status = turn_one.status();
    let turn_one_execution_path = turn_one
        .headers()
        .get(EXECUTION_PATH_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let turn_one_miss_reason = turn_one
        .headers()
        .get(LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let turn_one_text = turn_one.text().await.expect("turn one body should read");
    assert_eq!(
        turn_one_status,
        StatusCode::OK,
        "turn one failed before the continuation could be planned: execution_path={turn_one_execution_path:?} \
         miss_reason={turn_one_miss_reason:?} upstream_requests={} body={turn_one_text}",
        seen_chat_requests.lock().expect("mutex should lock").len()
    );
    assert_eq!(
        turn_one_execution_path.as_deref(),
        Some(EXECUTION_PATH_EXECUTION_RUNTIME_SYNC)
    );
    let turn_one_json: serde_json::Value =
        serde_json::from_str(&turn_one_text).expect("turn one body should parse");
    assert_eq!(turn_one_json["status"], "completed");
    let previous_response_id = turn_one_json["id"]
        .as_str()
        .expect("turn one response id")
        .to_string();

    // Turn two: the continuation relies on the recorded scoped history and does
    // not repeat the discovery output. The planner expands the hydrated
    // history, offers the discovered child to the Chat provider, and must
    // restore the namespace on the returned function call.
    let continuation_body = json!({
        "model": "gpt-5",
        "previous_response_id": previous_response_id,
        "tools": tool_search_tools(),
        "input": [{"role": "user", "content": "look up the customer"}]
    });
    let continuation = client
        .post(format!("{gateway_url}/v1/responses"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            "Bearer sk-client-responses-namespace",
        )
        .header(TRACE_ID_HEADER, "trace-responses-namespace-t2")
        .body(continuation_body.to_string())
        .send()
        .await
        .expect("continuation should succeed");
    let continuation_status = continuation.status();
    let continuation_execution_path = continuation
        .headers()
        .get(EXECUTION_PATH_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let continuation_miss_reason = continuation
        .headers()
        .get(LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let continuation_text = continuation
        .text()
        .await
        .expect("continuation body should read");
    if continuation_status != StatusCode::OK {
        // Skipped-candidate diagnostics carry the converter/contract field and
        // reason, which the client-facing error deliberately hides.
        let candidates = request_candidate_repository
            .list_by_request_id("trace-responses-namespace-t2")
            .await
            .expect("continuation candidate trace should read");
        let candidate_details = candidates
            .iter()
            .map(|candidate| {
                format!(
                    "status={:?} skip_reason={:?} error_type={:?} error_message={:?} \
                     extra_data={}",
                    candidate.status,
                    candidate.skip_reason,
                    candidate.error_type,
                    candidate.error_message,
                    candidate
                        .extra_data
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        panic!(
            "the scoped continuation must be served locally: execution_path={continuation_execution_path:?} \
             miss_reason={continuation_miss_reason:?} previous_response_id={previous_response_id} \
             upstream_requests={} candidates=[{candidate_details}] body={continuation_text}",
            seen_chat_requests.lock().expect("mutex should lock").len()
        );
    }
    assert_eq!(
        continuation_status,
        StatusCode::OK,
        "the scoped continuation must be served locally: execution_path={continuation_execution_path:?} \
         miss_reason={continuation_miss_reason:?} previous_response_id={previous_response_id} \
         upstream_requests={} body={continuation_text}",
        seen_chat_requests.lock().expect("mutex should lock").len()
    );
    let continuation_json: serde_json::Value =
        serde_json::from_str(&continuation_text).expect("continuation body should parse");
    assert_eq!(
        continuation_execution_path.as_deref(),
        Some(EXECUTION_PATH_EXECUTION_RUNTIME_SYNC)
    );

    let restored_call = continuation_json["output"]
        .as_array()
        .expect("continuation output")
        .iter()
        .find(|item| item["type"] == "function_call")
        .cloned()
        .expect("continuation must return the executed function call");
    assert_eq!(restored_call["call_id"], "call_lookup_resp_namespace_t2");
    assert_eq!(restored_call["name"], "lookup");
    assert_eq!(
        restored_call["namespace"], "crm",
        "the consumer-visible function call must keep its discovered Responses namespace: {continuation_json}"
    );

    let continuation_requests = seen_chat_requests
        .lock()
        .expect("mutex should lock")
        .iter()
        .filter(|payload| payload.to_string().contains("look up the customer"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        continuation_requests.len(),
        1,
        "the continuation must hit the Chat provider exactly once"
    );
    let offered_tool_names = continuation_requests[0]["tools"]
        .as_array()
        .expect("upstream chat tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(
        offered_tool_names.contains(&"lookup"),
        "the Chat provider must be offered the discovered child, saw {offered_tool_names:?}"
    );

    // The same continuation under another API key cannot see the scoped
    // history: it must fail closed without a namespace guess or an upstream
    // call that would replay the provider alias.
    let foreign_scope = client
        .post(format!("{gateway_url}/v1/responses"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            "Bearer sk-client-responses-namespace-other",
        )
        .header(TRACE_ID_HEADER, "trace-responses-namespace-foreign")
        .body(continuation_body.to_string())
        .send()
        .await
        .expect("foreign-scope continuation should be answered");
    let foreign_scope_status = foreign_scope.status();
    let foreign_scope_miss_reason = foreign_scope
        .headers()
        .get(LOCAL_EXECUTION_RUNTIME_MISS_REASON_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let foreign_scope_body = foreign_scope
        .text()
        .await
        .expect("foreign-scope body should read");
    assert!(
        !foreign_scope_status.is_success(),
        "a continuation must not resolve history recorded under another API key: \
         miss_reason={foreign_scope_miss_reason:?} body={foreign_scope_body}"
    );
    assert_ne!(
        foreign_scope_status,
        StatusCode::UNAUTHORIZED,
        "the foreign API key must authenticate; only the scoped history lookup may fail: {foreign_scope_body}"
    );
    assert!(
        !foreign_scope_body.contains("crm"),
        "a foreign API key must never receive the discovered namespace: {foreign_scope_body}"
    );
    assert_eq!(
        seen_chat_requests
            .lock()
            .expect("mutex should lock")
            .iter()
            .filter(|payload| payload.to_string().contains("look up the customer"))
            .count(),
        1,
        "a foreign-scope continuation must not reach the Chat provider"
    );

    gateway_handle.abort();
    provider_handle.abort();
}
