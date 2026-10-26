pub mod capability;
pub mod enumeration;
pub mod selectability;
pub mod types;

pub use capability::{
    candidate_supports_required_capability, requested_capability_priority_for_candidate,
};
pub use enumeration::{
    collect_global_model_names_for_required_capability, enumerate_minimal_candidate_selection,
    enumerate_minimal_candidate_selection_with_model_directives,
};
pub use selectability::{
    auth_api_key_concurrency_limit_reached, candidate_is_selectable_with_runtime_state,
    candidate_runtime_skip_reason_with_state, CandidateRuntimeSelectabilityInput,
};
pub use types::{
    EnumerateMinimalCandidateSelectionInput, SchedulerMinimalCandidateSelectionCandidate,
    SchedulerPriorityMode,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use aether_data_contracts::repository::candidate_selection::{
        StoredMinimalCandidateRoutingFacts, StoredMinimalCandidateSelectionRow,
        StoredProviderModelMapping,
    };
    use aether_data_contracts::repository::candidates::{
        RequestCandidateStatus, StoredRequestCandidate,
    };
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;

    use super::{
        auth_api_key_concurrency_limit_reached, candidate_is_selectable_with_runtime_state,
        candidate_runtime_skip_reason_with_state, candidate_supports_required_capability,
        collect_global_model_names_for_required_capability, CandidateRuntimeSelectabilityInput,
        EnumerateMinimalCandidateSelectionInput, SchedulerMinimalCandidateSelectionCandidate,
    };
    use crate::SchedulerAuthConstraints;

    fn sample_row(id: &str) -> StoredMinimalCandidateSelectionRow {
        StoredMinimalCandidateSelectionRow {
            provider_id: format!("provider-{id}"),
            provider_name: format!("Provider {id}"),
            provider_type: "custom".to_string(),
            provider_priority: 10,
            provider_is_active: true,
            endpoint_id: format!("endpoint-{id}"),
            endpoint_api_format: "openai:chat".to_string(),
            endpoint_api_family: Some("openai".to_string()),
            endpoint_kind: Some("chat".to_string()),
            endpoint_is_active: true,
            key_id: format!("key-{id}"),
            key_name: format!("prod-{id}"),
            key_auth_type: "api_key".to_string(),
            key_is_active: true,
            key_api_formats: Some(vec!["openai:chat".to_string()]),
            key_allowed_models: None,
            key_capabilities: Some(serde_json::json!({"cache_1h": true})),
            key_internal_priority: 50,
            key_global_priority_by_format: Some(serde_json::json!({"openai:chat": 2})),
            routing_facts: Default::default(),
            model_id: format!("model-{id}"),
            global_model_id: format!("global-model-{id}"),
            global_model_name: "gpt-5".to_string(),
            global_model_mappings: Some(vec!["gpt-5(?:\\.\\d+)?".to_string()]),
            global_model_supports_streaming: Some(true),
            model_provider_model_name: format!("gpt-5-upstream-{id}"),
            model_provider_model_mappings: Some(vec![StoredProviderModelMapping {
                name: format!("gpt-5-canary-{id}"),
                priority: 1,
                api_formats: Some(vec!["openai:chat".to_string()]),
                endpoint_ids: None,
                operations: None,
            }]),
            model_supports_streaming: None,
            model_is_active: true,
            model_is_available: true,
        }
    }
    fn sample_candidate(
        id: &str,
        capabilities: Option<serde_json::Value>,
    ) -> SchedulerMinimalCandidateSelectionCandidate {
        SchedulerMinimalCandidateSelectionCandidate {
            provider_id: format!("provider-{id}"),
            provider_name: format!("Provider {id}"),
            provider_type: "openai".to_string(),
            provider_priority: 0,
            endpoint_id: format!("endpoint-{id}"),
            endpoint_api_format: "openai:chat".to_string(),
            key_id: format!("key-{id}"),
            key_name: format!("key-{id}"),
            key_auth_type: "bearer".to_string(),
            key_internal_priority: 0,
            key_global_priority_for_format: None,
            key_capabilities: capabilities,
            routing_facts: Default::default(),
            model_id: format!("model-{id}"),
            global_model_id: format!("global-model-{id}"),
            global_model_name: "gpt-5".to_string(),
            selected_provider_model_name: "gpt-5".to_string(),
            supports_streaming: true,
            mapping_matched_model: None,
        }
    }

    fn sample_key(id: &str, health_score: f64) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            format!("key-{id}"),
            format!("provider-{id}"),
            format!("key-{id}"),
            "api_key".to_string(),
            None,
            true,
        )
        .expect("provider key should build");
        key.health_by_format = Some(serde_json::json!({
            "openai:chat": {
                "health_score": health_score
            }
        }));
        key
    }

    fn sample_key_with_concurrent_limit(
        id: &str,
        concurrent_limit: Option<i32>,
    ) -> StoredProviderCatalogKey {
        let mut key = sample_key(id, 1.0);
        key.concurrent_limit = concurrent_limit;
        key
    }

    fn stored_candidate(
        id: &str,
        status: RequestCandidateStatus,
        created_at_unix_ms: i64,
    ) -> StoredRequestCandidate {
        let finished_at_unix_ms = match status {
            RequestCandidateStatus::Pending | RequestCandidateStatus::Streaming => None,
            _ => Some(created_at_unix_ms),
        };
        StoredRequestCandidate::new(
            id.to_string(),
            format!("req-{id}"),
            None,
            None,
            None,
            None,
            0,
            0,
            Some("provider-1".to_string()),
            Some("endpoint-1".to_string()),
            Some("key-1".to_string()),
            status,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            created_at_unix_ms,
            Some(created_at_unix_ms),
            finished_at_unix_ms,
        )
        .expect("candidate should build")
    }

    #[test]
    fn reads_required_capability_from_object_and_array_forms() {
        assert!(candidate_supports_required_capability(
            &sample_candidate("1", Some(serde_json::json!({"vision": true}))),
            "vision"
        ));
        assert!(candidate_supports_required_capability(
            &sample_candidate("1", Some(serde_json::json!(["vision", "tools"]))),
            "tools"
        ));
        assert!(!candidate_supports_required_capability(
            &sample_candidate("1", Some(serde_json::json!({"vision": false}))),
            "vision"
        ));
    }

    #[test]
    fn enumerates_minimal_candidate_selection_with_auth_constraints() {
        let mut disallowed = sample_row("2");
        disallowed.provider_id = "provider-blocked".to_string();
        disallowed.provider_name = "Blocked".to_string();

        let constraints = SchedulerAuthConstraints {
            allowed_providers: Some(vec!["provider-1".to_string()]),
            allowed_api_formats: Some(vec!["OPENAI:CHAT".to_string()]),
            allowed_models: Some(vec!["gpt-5".to_string()]),
        };
        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![sample_row("1"), disallowed],
                normalized_api_format: "openai:chat",
                request_operation: None,
                requested_model_name: "gpt-5",
                resolved_global_model_name: "gpt-5",
                require_streaming: false,
                required_capabilities: None,
                auth_constraints: Some(&constraints),
            })
            .expect("candidate selection should build");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].provider_id, "provider-1");
        assert_eq!(candidates[0].selected_provider_model_name, "gpt-5-canary-1");
    }

    #[test]
    fn pool_representative_model_list_denial_does_not_reject_logical_group() {
        // 逻辑号池组在枚举列表里只带一个代表 Key 行；代表账号的显式模型清单
        // （与请求不兼容或显式为空）不得在成员展开存在之前否决整个组。
        for allowed_models in [Some(vec!["gpt-6".to_string()]), Some(Vec::new())] {
            let mut representative = sample_row("pool");
            representative.routing_facts = StoredMinimalCandidateRoutingFacts {
                provider_pool_enabled: true,
                ..Default::default()
            };
            representative.key_allowed_models = allowed_models;

            let candidates = super::enumerate_minimal_candidate_selection(
                EnumerateMinimalCandidateSelectionInput {
                    rows: vec![representative],
                    normalized_api_format: "openai:chat",
                    request_operation: None,
                    requested_model_name: "gpt-5",
                    resolved_global_model_name: "gpt-5",
                    require_streaming: false,
                    required_capabilities: None,
                    auth_constraints: None,
                },
            )
            .expect("pool group enumeration should build");

            assert_eq!(candidates.len(), 1);
            // 回退只忽略代表的模型清单限制；上游目标仍是该行作用域内的
            // 配置/默认模型名，不产生任何新的模型名。
            assert_eq!(
                candidates[0].selected_provider_model_name, "gpt-5-canary-pool",
                "fallback must keep the scoped configured/default upstream target"
            );
            assert_eq!(candidates[0].mapping_matched_model, None);
        }
    }

    #[test]
    fn non_pool_account_model_list_denial_stays_strict() {
        for allowed_models in [Some(vec!["gpt-6".to_string()]), Some(Vec::new())] {
            let mut row = sample_row("strict");
            row.key_allowed_models = allowed_models;

            let candidates = super::enumerate_minimal_candidate_selection(
                EnumerateMinimalCandidateSelectionInput {
                    rows: vec![row],
                    normalized_api_format: "openai:chat",
                    request_operation: None,
                    requested_model_name: "gpt-5",
                    resolved_global_model_name: "gpt-5",
                    require_streaming: false,
                    required_capabilities: None,
                    auth_constraints: None,
                },
            )
            .expect("candidate enumeration should build");

            assert!(
                candidates.is_empty(),
                "non-pool account model denial must stay strict"
            );
        }

        // NULL 清单保持不受限。
        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![sample_row("null")],
                normalized_api_format: "openai:chat",
                request_operation: None,
                requested_model_name: "gpt-5",
                resolved_global_model_name: "gpt-5",
                require_streaming: false,
                required_capabilities: None,
                auth_constraints: None,
            })
            .expect("candidate enumeration should build");
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn pool_representative_mapping_scope_mismatch_stays_denied() {
        // 代表回退只忽略模型清单限制，不允许越过 Endpoint / API 格式作用域。
        for mapping in [
            StoredProviderModelMapping {
                name: "gpt-5-elsewhere".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:chat".to_string()]),
                endpoint_ids: Some(vec!["endpoint-elsewhere".to_string()]),
                operations: None,
            },
            StoredProviderModelMapping {
                name: "gpt-5-elsewhere".to_string(),
                priority: 1,
                api_formats: Some(vec!["openai:responses".to_string()]),
                endpoint_ids: None,
                operations: None,
            },
        ] {
            let mut representative = sample_row("scope");
            representative.routing_facts = StoredMinimalCandidateRoutingFacts {
                provider_pool_enabled: true,
                ..Default::default()
            };
            representative.key_allowed_models = Some(vec!["gpt-6".to_string()]);
            representative.model_provider_model_mappings = Some(vec![mapping]);

            let candidates = super::enumerate_minimal_candidate_selection(
                EnumerateMinimalCandidateSelectionInput {
                    rows: vec![representative],
                    normalized_api_format: "openai:chat",
                    request_operation: None,
                    requested_model_name: "gpt-5",
                    resolved_global_model_name: "gpt-5",
                    require_streaming: false,
                    required_capabilities: None,
                    auth_constraints: None,
                },
            )
            .expect("candidate enumeration should build");

            assert!(
                candidates.is_empty(),
                "pool representative fallback must respect mapping endpoint/format scope"
            );
        }
    }

    #[test]
    fn generic_model_resolution_stays_strict_for_pool_member_rows() {
        // 成员行展开后仍带 pool 标志，通用解析器必须保持严格：显式清单
        // （含空清单）与请求无交集即拒绝，绝不能命中代表回退。
        for allowed_models in [Some(vec!["gpt-6".to_string()]), Some(Vec::new())] {
            let mut member = sample_row("member");
            member.routing_facts = StoredMinimalCandidateRoutingFacts {
                provider_pool_enabled: true,
                ..Default::default()
            };
            member.key_allowed_models = allowed_models;

            assert!(
                crate::model::resolve_provider_model_name_with_model_directives_and_request_operation(
                    &member,
                    "gpt-5",
                    "openai:chat",
                    false,
                    None,
                )
                .is_none(),
                "generic resolver must keep strict account model denial for pool member rows"
            );
        }
    }

    #[test]
    fn codex_live_enumeration_reuses_responses_mapping_without_widening_provider_scope() {
        let mut codex = sample_row("codex-live");
        codex.provider_name = "Codex".to_string();
        codex.provider_type = "codex".to_string();
        codex.endpoint_api_format = "codex:live".to_string();
        codex.endpoint_api_family = Some("codex".to_string());
        codex.endpoint_kind = Some("live".to_string());
        codex.key_auth_type = "oauth".to_string();
        codex.key_api_formats = Some(vec!["codex:live".to_string()]);
        codex.key_allowed_models = Some(vec!["gpt-future-live".to_string()]);
        codex.global_model_name = "live-future-alias".to_string();
        codex.model_provider_model_name = "gpt-future-live".to_string();
        codex.model_provider_model_mappings = Some(vec![StoredProviderModelMapping {
            name: "gpt-future-live".to_string(),
            priority: 1,
            api_formats: Some(vec!["openai:responses".to_string()]),
            endpoint_ids: Some(vec![codex.endpoint_id.clone()]),
            operations: None,
        }]);

        let constraints = SchedulerAuthConstraints {
            allowed_providers: Some(vec!["codex".to_string()]),
            allowed_api_formats: Some(vec!["codex:live".to_string()]),
            allowed_models: Some(vec!["live-future-alias".to_string()]),
        };
        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![codex.clone()],
                normalized_api_format: "codex:live",
                request_operation: None,
                requested_model_name: "live-future-alias",
                resolved_global_model_name: "live-future-alias",
                require_streaming: true,
                required_capabilities: None,
                auth_constraints: Some(&constraints),
            })
            .expect("Codex Live candidate enumeration should build");

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].selected_provider_model_name,
            "gpt-future-live"
        );

        for provider_type in ["openai", "custom"] {
            let mut non_codex = codex.clone();
            non_codex.provider_type = provider_type.to_string();
            let candidates = super::enumerate_minimal_candidate_selection(
                EnumerateMinimalCandidateSelectionInput {
                    rows: vec![non_codex],
                    normalized_api_format: "codex:live",
                    request_operation: None,
                    requested_model_name: "live-future-alias",
                    resolved_global_model_name: "live-future-alias",
                    require_streaming: true,
                    required_capabilities: None,
                    auth_constraints: None,
                },
            )
            .expect("non-Codex Live candidate enumeration should build");
            assert!(candidates.is_empty());
        }
    }

    #[test]
    fn enumeration_preserves_effective_streaming_capability() {
        let mut row = sample_row("1");
        row.model_supports_streaming = Some(false);

        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![row],
                normalized_api_format: "openai:chat",
                request_operation: None,
                requested_model_name: "gpt-5",
                resolved_global_model_name: "gpt-5",
                require_streaming: false,
                required_capabilities: None,
                auth_constraints: None,
            })
            .expect("candidate selection should build");

        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].supports_streaming);
    }

    #[test]
    fn enumeration_preserves_theoretical_candidate_order_without_final_sorting() {
        let mut later_priority = sample_row("1");
        later_priority.provider_priority = 10;
        let mut earlier_priority = sample_row("2");
        earlier_priority.provider_priority = 0;

        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![later_priority, earlier_priority],
                normalized_api_format: "openai:chat",
                request_operation: None,
                requested_model_name: "gpt-5",
                resolved_global_model_name: "gpt-5",
                require_streaming: false,
                required_capabilities: None,
                auth_constraints: None,
            })
            .expect("candidate enumeration should build");

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].provider_id, "provider-1");
        assert_eq!(candidates[1].provider_id, "provider-2");
    }

    #[test]
    fn collects_global_model_names_for_required_capability_with_auth_constraints() {
        let mut disallowed = sample_row("2");
        disallowed.global_model_name = "gpt-4.1".to_string();
        disallowed.provider_id = "provider-blocked".to_string();
        disallowed.provider_name = "Blocked".to_string();

        let constraints = SchedulerAuthConstraints {
            allowed_providers: Some(vec!["provider-1".to_string()]),
            allowed_api_formats: Some(vec!["openai:chat".to_string()]),
            allowed_models: Some(vec!["gpt-5".to_string()]),
        };
        let model_names = collect_global_model_names_for_required_capability(
            vec![sample_row("1"), disallowed],
            "openai:chat",
            "cache_1h",
            false,
            Some(&constraints),
        );

        assert_eq!(model_names, vec!["gpt-5".to_string()]);
    }

    #[test]
    fn requested_capability_priority_counts_missing_compatible_capabilities() {
        let mut missing_capability = sample_row("1");
        missing_capability.key_capabilities = Some(serde_json::json!({"cache_1h": false}));
        missing_capability.provider_priority = 0;

        let mut matching_capability = sample_row("2");
        matching_capability.key_capabilities = Some(serde_json::json!({"cache_1h": true}));
        matching_capability.provider_priority = 10;

        let required_capabilities = serde_json::json!({"cache_1h": true});
        let candidates =
            super::enumerate_minimal_candidate_selection(EnumerateMinimalCandidateSelectionInput {
                rows: vec![missing_capability, matching_capability],
                normalized_api_format: "openai:chat",
                request_operation: None,
                requested_model_name: "gpt-5",
                resolved_global_model_name: "gpt-5",
                require_streaming: false,
                required_capabilities: Some(&required_capabilities),
                auth_constraints: None,
            })
            .expect("candidate selection should build");
        let priority = candidates
            .iter()
            .map(|candidate| {
                super::requested_capability_priority_for_candidate(
                    Some(&required_capabilities),
                    candidate,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(candidates.len(), 2);
        assert_eq!(priority, vec![(0, 1), (0, 0)]);
    }

    #[test]
    fn candidate_selectability_respects_provider_concurrency_limit() {
        let recent_candidates = vec![stored_candidate("one", RequestCandidateStatus::Pending, 95)];
        let provider_concurrent_limits = BTreeMap::from([("provider-1".to_string(), 1)]);

        assert!(!candidate_is_selectable_with_runtime_state(
            CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &provider_concurrent_limits,
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            },
        ));
    }

    #[test]
    fn provider_key_concurrency_limit_unset_or_zero_is_unlimited() {
        let recent_candidates = vec![stored_candidate("one", RequestCandidateStatus::Pending, 95)];
        for concurrent_limit in [None, Some(0)] {
            let provider_key_rpm_states = BTreeMap::from([(
                "key-1".to_string(),
                sample_key_with_concurrent_limit("1", concurrent_limit),
            )]);

            assert_eq!(
                candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                    candidate: &sample_candidate("1", None),
                    recent_candidates: &recent_candidates,
                    provider_concurrent_limits: &BTreeMap::new(),
                    provider_key_rpm_states: &provider_key_rpm_states,
                    now_unix_secs: 100,
                    provider_quota_blocks_requests: false,
                    quota_hard_blocked: false,
                    account_quota_exhausted: false,
                    balance_below_minimum: false,
                    oauth_invalid: false,
                    enforce_key_circuit_breaker: true,
                    rpm_reset_at: None,
                }),
                None
            );
        }
    }

    #[test]
    fn provider_key_concurrency_limit_rejects_pending_active_with_exact_skip_reason() {
        let recent_candidates = vec![stored_candidate("one", RequestCandidateStatus::Pending, 95)];
        let provider_key_rpm_states = BTreeMap::from([(
            "key-1".to_string(),
            sample_key_with_concurrent_limit("1", Some(1)),
        )]);

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("provider_key_concurrency_limit_reached")
        );
    }

    #[test]
    fn provider_key_concurrency_limit_rejects_streaming_active() {
        let recent_candidates = vec![stored_candidate(
            "streaming",
            RequestCandidateStatus::Streaming,
            95,
        )];
        let provider_key_rpm_states = BTreeMap::from([(
            "key-1".to_string(),
            sample_key_with_concurrent_limit("1", Some(1)),
        )]);

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("provider_key_concurrency_limit_reached")
        );
    }

    #[test]
    fn provider_key_concurrency_limit_ignores_finished_and_stale_active_requests() {
        let recent_candidates = vec![
            stored_candidate("finished", RequestCandidateStatus::Success, 95),
            stored_candidate("failed", RequestCandidateStatus::Failed, 96),
            stored_candidate("cancelled", RequestCandidateStatus::Cancelled, 97),
            stored_candidate("stale", RequestCandidateStatus::Pending, 699_000),
        ];
        let provider_key_rpm_states = BTreeMap::from([(
            "key-1".to_string(),
            sample_key_with_concurrent_limit("1", Some(1)),
        )]);

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 1_000,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            None
        );
    }

    #[test]
    fn provider_key_concurrency_limit_missing_state_does_not_skip() {
        let recent_candidates = vec![stored_candidate("one", RequestCandidateStatus::Pending, 95)];

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            None
        );
    }

    #[test]
    fn recent_failures_do_not_skip_without_persisted_circuit() {
        let recent_candidates = vec![
            stored_candidate("failed", RequestCandidateStatus::Failed, 95_000),
            stored_candidate("cancelled", RequestCandidateStatus::Cancelled, 99_000),
        ];

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            None
        );
    }

    #[test]
    fn provider_key_concurrency_limit_preserves_key_circuit_and_rpm_checks() {
        let mut circuit_open_key = sample_key_with_concurrent_limit("1", Some(2));
        circuit_open_key.circuit_breaker_by_format = Some(serde_json::json!({
            "openai:chat": {"open": true}
        }));
        let provider_key_rpm_states = BTreeMap::from([("key-1".to_string(), circuit_open_key)]);
        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("key_circuit_open")
        );

        let recent_candidates = vec![stored_candidate(
            "one",
            RequestCandidateStatus::Pending,
            95_000,
        )];
        let provider_key_rpm_states = BTreeMap::from([(
            "key-1".to_string(),
            sample_key_with_concurrent_limit("1", Some(2)).with_rate_limit_fields(
                Some(1),
                Some(2),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        )]);

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &recent_candidates,
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("key_rpm_exhausted")
        );
    }

    #[test]
    fn key_circuit_allows_probe_after_next_probe_time() {
        let mut circuit_open_key = sample_key_with_concurrent_limit("1", Some(2));
        circuit_open_key.circuit_breaker_by_format = Some(serde_json::json!({
            "openai:chat": {
                "open": true,
                "next_probe_at_unix_secs": 100
            }
        }));
        let provider_key_rpm_states = BTreeMap::from([("key-1".to_string(), circuit_open_key)]);

        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 99,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("key_circuit_open")
        );
        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            None
        );
    }

    #[test]
    fn candidate_selectability_rejects_quota_or_zero_health() {
        let provider_key_rpm_states = BTreeMap::from([("key-1".to_string(), sample_key("1", 0.0))]);

        assert!(!candidate_is_selectable_with_runtime_state(
            CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &provider_key_rpm_states,
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            },
        ));
        assert!(!candidate_is_selectable_with_runtime_state(
            CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: true,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            },
        ));
    }

    #[test]
    fn candidate_selectability_rejects_exhausted_account_quota() {
        assert!(!candidate_is_selectable_with_runtime_state(
            CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: true,
                balance_below_minimum: false,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            },
        ));
    }

    #[test]
    fn candidate_selectability_reports_low_balance_reason() {
        // 验证余额不足使用独立诊断原因，不与订阅额度耗尽混淆。
        assert_eq!(
            candidate_runtime_skip_reason_with_state(CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: true,
                oauth_invalid: false,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            }),
            Some("key_balance_below_minimum")
        );
    }

    #[test]
    fn candidate_selectability_rejects_oauth_invalid_keys() {
        assert!(!candidate_is_selectable_with_runtime_state(
            CandidateRuntimeSelectabilityInput {
                candidate: &sample_candidate("1", None),
                recent_candidates: &[],
                provider_concurrent_limits: &BTreeMap::new(),
                provider_key_rpm_states: &BTreeMap::new(),
                now_unix_secs: 100,
                provider_quota_blocks_requests: false,
                quota_hard_blocked: false,
                account_quota_exhausted: false,
                balance_below_minimum: false,
                oauth_invalid: true,
                enforce_key_circuit_breaker: true,
                rpm_reset_at: None,
            },
        ));
    }

    #[test]
    fn detects_auth_api_key_concurrency_limit_from_recent_active_requests() {
        let recent_candidates = vec![StoredRequestCandidate::new(
            "one".to_string(),
            "req-one".to_string(),
            None,
            Some("api-key-1".to_string()),
            None,
            None,
            0,
            0,
            Some("provider-1".to_string()),
            Some("endpoint-1".to_string()),
            Some("key-1".to_string()),
            RequestCandidateStatus::Pending,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            95,
            Some(95),
            None,
        )
        .expect("candidate should build")];

        assert!(auth_api_key_concurrency_limit_reached(
            &recent_candidates,
            100,
            "api-key-1",
            1,
        ));
        assert!(!auth_api_key_concurrency_limit_reached(
            &recent_candidates,
            100,
            "api-key-1",
            2,
        ));
    }
}
