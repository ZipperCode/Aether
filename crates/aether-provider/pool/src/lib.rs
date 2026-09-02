mod background_limiter;
mod capability;
mod plan;
mod presets;
mod provider;
mod quota;
mod quota_refresh;
mod quota_snapshot;
mod service;

pub mod providers;

pub use background_limiter::OfficialProviderBackgroundLimiter;
pub use capability::{
    ProviderPoolCapabilities, ProviderPoolCapability, ProviderQuotaHealthTransition,
    ProviderQuotaServingPolicy,
};
pub use plan::{derive_oauth_plan_type, derive_plan_tier, normalize_provider_plan_tier};
pub use presets::{
    build_admin_pool_scheduling_presets_payload, normalize_provider_scheduling_presets,
};
pub use provider::{ProviderPoolAdapter, ProviderPoolMemberInput};
pub use providers::{
    build_antigravity_pool_quota_request, build_chatgpt_web_pool_quota_request,
    build_codex_pool_quota_request, build_codex_pool_reset_credit_consume_request,
    build_codex_pool_reset_credits_request, build_deepseek_balance_request,
    build_gemini_cli_pool_quota_request, build_kiro_pool_quota_request,
    build_nous_account_quota_request, build_nous_billing_quota_request,
    build_openrouter_credits_request, build_windsurf_pool_model_configs_request,
    build_windsurf_pool_model_configs_request_with_base_url, build_windsurf_pool_quota_request,
    build_windsurf_pool_quota_request_with_base_url, build_windsurf_pool_rate_limit_request,
    build_windsurf_pool_rate_limit_request_with_base_url,
    clamp_official_balance_execution_timeouts, enrich_chatgpt_web_quota_metadata,
    grok_mode_id_for_model, grok_pool_tier_from_quota_bucket, grok_quota_window_key_for_model,
    grok_supported_quota_windows_for_tier, is_official_deepseek_endpoint,
    is_official_openrouter_endpoint, normalize_chatgpt_web_image_quota_limit,
    parse_deepseek_balance, parse_openrouter_credits, AntigravityProviderPoolAdapter,
    ChatGptWebProviderPoolAdapter, CodexProviderPoolAdapter, DeepSeekProviderPoolAdapter,
    DefaultProviderPoolAdapter, GeminiCliProviderPoolAdapter, GrokProviderPoolAdapter,
    KiroPoolQuotaAuthInput, KiroProviderPoolAdapter, NousProviderPoolAdapter,
    OpenRouterProviderPoolAdapter, UnsupportedQuotaProviderPoolAdapter,
    ANTIGRAVITY_FETCH_AVAILABLE_MODELS_PATH, CHATGPT_WEB_CONVERSATION_INIT_PATH,
    CHATGPT_WEB_DEFAULT_BASE_URL, CODEX_WHAM_RESET_CREDITS_CONSUME_URL,
    CODEX_WHAM_RESET_CREDITS_URL, CODEX_WHAM_USAGE_URL, DEEPSEEK_BALANCE_URL,
    GEMINI_CLI_RETRIEVE_USER_QUOTA_PATH, GEMINI_CLI_USER_AGENT, KIRO_USAGE_LIMITS_PATH,
    KIRO_USAGE_SDK_VERSION, OPENROUTER_CREDITS_URL, WINDSURF_MODEL_CONFIGS_PATH,
    WINDSURF_RATE_LIMIT_PATH, WINDSURF_USER_STATUS_PATH,
};
pub use providers::{
    build_official_api_key_quota_request, build_zhipu_account_balance_request,
    build_zhipu_team_quota_request, is_official_api_key_quota_endpoint,
    parse_official_api_key_quota, parse_zhipu_standard_balance, OfficialApiKeyQuotaProvider,
    OfficialApiKeyQuotaProviderPoolAdapter, ZHIPU_ACCOUNT_REPORT_URL, ZHIPU_TEAM_QUOTA_URL,
};
pub use quota::{
    provider_pool_key_account_quota_exhausted, provider_pool_key_balance_below_minimum,
    provider_pool_key_quota_hard_blocked, provider_pool_key_runtime_quota_blocked,
    provider_pool_key_scheduling_label, provider_pool_member_quota_snapshot,
    provider_pool_quota_metadata_provider_type, provider_pool_quota_metadata_updated_at,
    provider_pool_quota_snapshot_updated_at,
};
pub use quota_refresh::{
    official_balance_backoff_secs, official_balance_backoff_with_jitter_secs,
    ProviderPoolQuotaRequestSpec, OFFICIAL_BALANCE_MAX_BACKOFF_SECS,
    OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
};
pub use quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaRefreshState, ProviderQuotaSnapshotContract,
    ProviderQuotaSnapshotKind, ProviderQuotaValue, ProviderQuotaWindow,
    PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION, ZHIPU_TOKEN_PLAN_SCHEDULING_BLOCKED_FIELD,
    ZHIPU_TOKEN_PLAN_STATUS_FIELD,
};
pub use service::ProviderPoolService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::provider_pool_quota_snapshot_exhausted_decision;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
    use aether_pool_core::PoolSchedulingPreset;
    use serde_json::{json, Value};

    fn sample_key(upstream_metadata: Option<Value>) -> StoredProviderCatalogKey {
        let mut key = StoredProviderCatalogKey::new(
            "key-1".to_string(),
            "provider-1".to_string(),
            "key-1".to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.upstream_metadata = upstream_metadata;
        key
    }

    fn sample_key_with_quota(quota: Value) -> StoredProviderCatalogKey {
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({ "quota": quota }));
        key
    }

    #[test]
    fn quota_snapshot_accepts_current_schema_for_matching_provider() {
        let key = sample_key_with_quota(json!({
            "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            "provider_type": "deepseek",
            "kind": "balance",
            "updated_at": 1_700_000_000u64
        }));

        let snapshot = provider_pool_member_quota_snapshot(&key, "deepseek")
            .expect("matching current snapshot should be accepted");

        assert_eq!(snapshot["kind"], json!("balance"));
    }

    #[test]
    fn quota_snapshot_accepts_supported_legacy_shape_without_provider_type() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        let key = sample_key_with_quota(json!({
            "exhausted": true,
            "reset_seconds": 300,
            "updated_at": now
        }));

        assert_eq!(
            provider_pool_quota_snapshot_exhausted_decision(&key, "codex"),
            Some(true)
        );
    }

    #[test]
    fn quota_snapshot_ignores_snapshot_for_different_provider_type() {
        let key = sample_key_with_quota(json!({
            "provider_type": "deepseek",
            "kind": "balance",
            "updated_at": 1_700_000_000u64
        }));

        assert!(provider_pool_member_quota_snapshot(&key, "codex").is_none());
        assert_eq!(
            provider_pool_quota_snapshot_exhausted_decision(&key, "codex"),
            None
        );
    }

    #[test]
    fn balance_scheduling_requires_fresh_complete_low_balances() {
        // 验证固定边界和多币种“全部不高于阈值”语义。
        let balance_key = |balances: Value| {
            sample_key_with_quota(json!({
                "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
                "provider_type": "deepseek",
                "kind": "balance",
                "freshness": "fresh",
                "balances": balances
            }))
        };

        for available in ["0", "1"] {
            assert!(provider_pool_key_balance_below_minimum(
                &balance_key(json!([{"unit": "USD", "available": available}])),
                "deepseek"
            ));
        }
        assert!(!provider_pool_key_balance_below_minimum(
            &balance_key(json!([{"unit": "USD", "available": "1.0001"}])),
            "deepseek"
        ));
        assert!(provider_pool_key_balance_below_minimum(
            &balance_key(json!([
                {"unit": "USD", "available": "1"},
                {"unit": "CNY", "available": "0.5"}
            ])),
            "deepseek"
        ));
        assert!(!provider_pool_key_balance_below_minimum(
            &balance_key(json!([
                {"unit": "USD", "available": "1"},
                {"unit": "CNY", "available": "1.01"}
            ])),
            "deepseek"
        ));
    }

    #[test]
    fn balance_scheduling_fails_open_for_unknown_or_stale_inputs() {
        // 验证无法确认余额事实时统一 fail-open，避免陈旧状态永久阻断。
        let base = json!({
            "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            "provider_type": "deepseek",
            "kind": "balance",
            "freshness": "fresh",
            "balances": [{"unit": "USD", "available": "1"}]
        });
        let mut invalid_snapshots = Vec::new();

        let mut stale = base.clone();
        stale["freshness"] = json!("stale");
        invalid_snapshots.push(stale);
        let mut empty = base.clone();
        empty["balances"] = json!([]);
        invalid_snapshots.push(empty);
        let mut missing_unit = base.clone();
        missing_unit["balances"][0]
            .as_object_mut()
            .expect("balance")
            .remove("unit");
        invalid_snapshots.push(missing_unit);
        let mut invalid_value = base.clone();
        invalid_value["balances"][0]["available"] = json!("NaN");
        invalid_snapshots.push(invalid_value);
        let mut missing_value = base.clone();
        missing_value["balances"][0]
            .as_object_mut()
            .expect("balance")
            .remove("available");
        invalid_snapshots.push(missing_value);
        let mut empty_unit = base.clone();
        empty_unit["balances"][0]["unit"] = json!("  ");
        invalid_snapshots.push(empty_unit);
        let mut unlimited = base.clone();
        unlimited["unlimited"] = json!(true);
        invalid_snapshots.push(unlimited);
        let mut unknown_unlimited = base.clone();
        unknown_unlimited["unlimited"] = json!("unknown");
        invalid_snapshots.push(unknown_unlimited);
        let mut subscription = base;
        subscription["kind"] = json!("subscription");
        invalid_snapshots.push(subscription);

        for quota in invalid_snapshots {
            assert!(!provider_pool_key_balance_below_minimum(
                &sample_key_with_quota(quota),
                "deepseek"
            ));
        }
    }

    #[test]
    fn zhipu_balance_fallback_with_missing_token_plan_blocks_scheduling() {
        let blocked = sample_key_with_quota(json!({
            "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            "provider_type": "zhipu",
            "kind": "balance",
            "exhausted": true,
            "balances": [{"unit": "CNY", "available": "12.50"}],
            "token_plan_status": "expired",
            "token_plan_scheduling_blocked": true,
            "updated_at": 1_700_000_000u64
        }));
        assert!(provider_pool_key_account_quota_exhausted(&blocked, "zhipu"));

        let informational = sample_key_with_quota(json!({
            "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            "provider_type": "zhipu",
            "kind": "balance",
            "exhausted": false,
            "balances": [{"unit": "CNY", "available": "12.50"}],
            "updated_at": 1_700_000_000u64
        }));
        assert!(!provider_pool_key_account_quota_exhausted(
            &informational,
            "zhipu"
        ));
    }

    #[test]
    fn builtin_service_registers_provider_pool_adapters() {
        let service = ProviderPoolService::with_builtin_adapters();

        assert_eq!(
            service.provider_types().collect::<Vec<_>>(),
            [
                "antigravity",
                "chatgpt_web",
                "claude_code",
                "codex",
                "deepseek",
                "gemini_cli",
                "grok",
                "kimi_coding",
                "kiro",
                "moonshot",
                "nous",
                "openrouter",
                "siliconflow",
                "vertex_ai",
                "windsurf",
                "zai",
                "zhipu"
            ]
        );
        assert!(service
            .adapter("codex")
            .capabilities()
            .supports(ProviderPoolCapability::PlanTier));
        assert_eq!(service.adapter("unknown").provider_type(), "default");
    }

    #[test]
    fn builtin_service_owns_quota_refresh_support_and_endpoint_selection() {
        let service = ProviderPoolService::with_builtin_adapters();

        assert_eq!(
            service.provider_types_for_capability(ProviderPoolCapability::QuotaRefresh),
            [
                "antigravity",
                "chatgpt_web",
                "codex",
                "deepseek",
                "gemini_cli",
                "grok",
                "kimi_coding",
                "kiro",
                "moonshot",
                "nous",
                "openrouter",
                "siliconflow",
                "windsurf",
                "zai",
                "zhipu"
            ]
        );
        assert!(service.supports_quota_refresh("codex"));
        assert!(service.supports_quota_refresh("antigravity"));
        assert!(service.supports_quota_refresh("grok"));
        assert!(service.supports_quota_refresh("gemini_cli"));
        assert!(service.supports_quota_refresh("windsurf"));
        assert!(service.supports_quota_refresh("deepseek"));
        assert!(service.supports_quota_refresh("openrouter"));
        assert!(service.supports_quota_refresh("moonshot"));
        assert!(service.supports_quota_refresh("kimi_coding"));
        assert!(service.supports_quota_refresh("siliconflow"));
        assert!(service.supports_quota_refresh("zhipu"));
        assert!(service.supports_quota_refresh("zai"));
        assert_eq!(
            service.quota_refresh_unsupported_message("claude_code"),
            "Claude Code 暂不支持自动刷新额度：上游没有稳定可用的账号额度查询接口"
        );
        assert_eq!(
            service.quota_refresh_unsupported_message("vertex_ai"),
            "Vertex AI 暂不支持自动刷新额度：额度属于 Google Cloud 项目/区域配额"
        );
    }

    #[test]
    fn official_quota_providers_expose_typed_serving_policy() {
        let service = ProviderPoolService::with_builtin_adapters();

        for provider_type in ["deepseek", "openrouter", "moonshot", "siliconflow"] {
            assert_eq!(
                service.quota_serving_policy(provider_type),
                Some(ProviderQuotaServingPolicy::ObservationOnly)
            );
        }
        for provider_type in ["kimi_coding", "zhipu", "zai"] {
            assert_eq!(
                service.quota_serving_policy(provider_type),
                Some(ProviderQuotaServingPolicy::SubscriptionExhaustionOnly)
            );
        }
        for provider_type in ["codex", "gemini_cli", "grok"] {
            assert_eq!(
                service.quota_serving_policy(provider_type),
                Some(ProviderQuotaServingPolicy::ServingProbe)
            );
        }
        assert_eq!(service.quota_serving_policy("malformed-provider"), None);
    }

    #[test]
    fn subscription_health_transition_preserves_unrelated_state() {
        let policy = ProviderQuotaServingPolicy::SubscriptionExhaustionOnly;

        assert_eq!(
            policy.subscription_transition(false, true, true),
            ProviderQuotaHealthTransition::Preserve
        );
        assert_eq!(
            policy.subscription_transition(true, true, false),
            ProviderQuotaHealthTransition::QuotaExhausted
        );
        assert_eq!(
            policy.subscription_transition(true, false, true),
            ProviderQuotaHealthTransition::Available
        );
        assert_eq!(
            policy.subscription_transition(true, false, false),
            ProviderQuotaHealthTransition::Preserve
        );
        assert_eq!(
            ProviderQuotaServingPolicy::ObservationOnly.subscription_transition(false, true, true),
            ProviderQuotaHealthTransition::Preserve
        );
    }

    #[test]
    fn codex_quota_request_adds_account_header_for_paid_accounts() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some(("authorization".to_string(), "Bearer access".to_string())),
            None,
            Some(&json!({
                "plan_type": "plus",
                "account_id": "acct-1"
            })),
        )
        .expect("spec should build");

        assert_eq!(
            spec.headers.get("chatgpt-account-id").map(String::as_str),
            Some("acct-1")
        );
    }

    #[test]
    fn codex_quota_request_prefers_imported_authorization_header() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some((
                "authorization".to_string(),
                "Bearer jwt-access-token".to_string(),
            )),
            None,
            Some(&json!({
                "headers": {
                    "authorization": "Bearer imported-session"
                }
            })),
        )
        .expect("spec should build");

        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("Bearer imported-session")
        );
    }

    #[test]
    fn codex_agent_identity_quota_request_prefers_dynamic_assertion() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some((
                "authorization".to_string(),
                "AgentAssertion signed-at-request-time".to_string(),
            )),
            None,
            Some(&json!({
                "auth_mode": "agentIdentity",
                "headers": {
                    "authorization": "Bearer stale-imported-session"
                }
            })),
        )
        .expect("spec should build");

        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("AgentAssertion signed-at-request-time")
        );
    }

    #[test]
    fn codex_nested_agent_identity_quota_request_prefers_dynamic_assertion() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some((
                "authorization".to_string(),
                "AgentAssertion signed-at-request-time".to_string(),
            )),
            None,
            Some(&json!({
                "agent_identity": {
                    "agent_runtime_id": "runtime-1",
                    "agent_private_key": "private-key"
                },
                "headers": {
                    "authorization": "Bearer stale-imported-session"
                }
            })),
        )
        .expect("spec should build");

        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("AgentAssertion signed-at-request-time")
        );
    }

    #[test]
    fn gemini_cli_quota_request_uses_v1internal_retrieve_user_quota() {
        let spec = build_gemini_cli_pool_quota_request(
            "key-1",
            "https://cloudcode-pa.googleapis.com/",
            ("authorization".to_string(), "Bearer access".to_string()),
            "project-1",
        );

        assert_eq!(spec.method, "POST");
        assert_eq!(
            spec.url,
            "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"
        );
        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("Bearer access")
        );
        assert_eq!(
            spec.json_body.as_ref().and_then(|body| body.get("project")),
            Some(&json!("project-1"))
        );
        assert_eq!(
            spec.headers.get("user-agent").map(String::as_str),
            Some(GEMINI_CLI_USER_AGENT)
        );
        assert!(spec
            .json_body
            .as_ref()
            .is_some_and(|body| body.get("userAgent").is_none()));
        assert_eq!(spec.client_api_format, "gemini:generate_content");
        assert_eq!(spec.provider_api_format, "gemini_cli:retrieve_user_quota");
    }

    #[test]
    fn codex_quota_request_uses_wham_usage_endpoint() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some(("authorization".to_string(), "Bearer access".to_string())),
            None,
            None,
        )
        .expect("spec should build");

        assert_eq!(spec.method, "GET");
        assert_eq!(spec.url, "https://chatgpt.com/backend-api/wham/usage");
        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("Bearer access")
        );
        assert_eq!(
            spec.headers.get("accept").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(spec.model_name.as_deref(), Some("codex-wham-usage"));
    }

    #[test]
    fn codex_reset_credits_request_uses_wham_detail_endpoint() {
        let spec = build_codex_pool_reset_credits_request(
            "key-1",
            Some(("authorization".to_string(), "Bearer access".to_string())),
            None,
            Some(&json!({
                "plan_type": "plus",
                "account_id": "acct-1"
            })),
        )
        .expect("spec should build");

        assert_eq!(spec.method, "GET");
        assert_eq!(spec.url, CODEX_WHAM_RESET_CREDITS_URL);
        assert_eq!(
            spec.headers.get("authorization").map(String::as_str),
            Some("Bearer access")
        );
        assert_eq!(
            spec.headers.get("chatgpt-account-id").map(String::as_str),
            Some("acct-1")
        );
        assert_eq!(spec.model_name.as_deref(), Some("codex-wham-reset-credits"));
    }

    #[test]
    fn codex_reset_credit_consume_request_posts_redeem_request_id() {
        let spec = build_codex_pool_reset_credit_consume_request(
            "key-1",
            Some(("authorization".to_string(), "Bearer access".to_string())),
            None,
            None,
            "8ae6f1c7-7e9e-4f5d-9b8a-000000000000",
        )
        .expect("spec should build");

        assert_eq!(spec.method, "POST");
        assert_eq!(spec.url, CODEX_WHAM_RESET_CREDITS_CONSUME_URL);
        assert_eq!(spec.content_type.as_deref(), Some("application/json"));
        assert_eq!(
            spec.json_body
                .as_ref()
                .and_then(|body| body.get("redeem_request_id")),
            Some(&json!("8ae6f1c7-7e9e-4f5d-9b8a-000000000000"))
        );
        assert_eq!(
            spec.json_body
                .as_ref()
                .and_then(|body| body.as_object())
                .map(|body| body.len()),
            Some(1)
        );
        assert_eq!(
            spec.model_name.as_deref(),
            Some("codex-wham-reset-credit-consume")
        );
    }

    #[test]
    fn codex_quota_request_skips_account_header_for_free_accounts() {
        let spec = build_codex_pool_quota_request(
            "key-1",
            Some(("authorization".to_string(), "Bearer access".to_string())),
            None,
            Some(&json!({
                "plan_type": "codex:free",
                "account_id": "acct-1"
            })),
        )
        .expect("spec should build");

        assert!(!spec.headers.contains_key("chatgpt-account-id"));
    }

    #[test]
    fn kiro_quota_request_includes_profile_arn_when_present() {
        let spec = build_kiro_pool_quota_request(
            "key-1",
            &KiroPoolQuotaAuthInput {
                authorization_value: "Bearer access".to_string(),
                api_region: "us-west-2".to_string(),
                kiro_version: "0.3.210".to_string(),
                machine_id: "machine".to_string(),
                profile_arn: Some("arn:aws:sso:::profile/p-1".to_string()),
            },
        );

        assert!(spec.url.contains("q.us-west-2.amazonaws.com"));
        assert!(spec
            .url
            .contains("profileArn=arn%3Aaws%3Asso%3A%3A%3Aprofile%2Fp-1"));
    }

    #[test]
    fn chatgpt_web_quota_request_uses_default_base_url_when_empty() {
        let spec = build_chatgpt_web_pool_quota_request(
            "key-1",
            "",
            ("authorization".to_string(), "Bearer access".to_string()),
        );

        assert_eq!(
            spec.url,
            "https://chatgpt.com/backend-api/conversation/init"
        );
        assert_eq!(
            spec.headers.get("origin").map(String::as_str),
            Some("https://chatgpt.com")
        );
        assert!(spec.accept_invalid_certs);
    }

    #[test]
    fn chatgpt_web_quota_metadata_enriches_auth_and_uses_first_remaining_as_limit() {
        let mut metadata = json!({
            "image_quota_remaining": 12,
        });
        enrich_chatgpt_web_quota_metadata(
            &mut metadata,
            Some(&json!({
                "plan": "free",
                "email": "user@example.com",
                "accountId": "acct-1"
            })),
        );
        normalize_chatgpt_web_image_quota_limit(&mut metadata, None);

        assert_eq!(metadata["plan_type"], json!("free"));
        assert_eq!(metadata["email"], json!("user@example.com"));
        assert_eq!(metadata["account_id"], json!("acct-1"));
        assert_eq!(metadata["image_quota_total"], json!(12.0));
        assert_eq!(metadata["image_quota_used"], json!(0.0));
    }

    #[test]
    fn chatgpt_web_quota_metadata_preserves_existing_paid_limit() {
        let mut metadata = json!({
            "plan_type": "plus",
            "image_quota_remaining": 7,
        });
        normalize_chatgpt_web_image_quota_limit(
            &mut metadata,
            Some(&json!({
                "chatgpt_web": {
                    "image_quota_total": 40
                }
            })),
        );

        assert_eq!(metadata["image_quota_total"], json!(40.0));
        assert_eq!(metadata["image_quota_used"], json!(33.0));
    }

    #[test]
    fn chatgpt_web_quota_metadata_does_not_preserve_legacy_free_25_limit() {
        let mut metadata = json!({
            "plan_type": "free",
            "image_quota_remaining": 19,
        });
        normalize_chatgpt_web_image_quota_limit(
            &mut metadata,
            Some(&json!({
                "chatgpt_web": {
                    "plan_type": "free",
                    "image_quota_total": 25
                }
            })),
        );

        assert_eq!(metadata["image_quota_total"], json!(19.0));
        assert_eq!(metadata["image_quota_used"], json!(0.0));
        assert_eq!(
            metadata["image_quota_limit_source"],
            json!("first_remaining")
        );
    }

    #[test]
    fn chatgpt_web_quota_metadata_ignores_upstream_free_25_default() {
        let mut metadata = json!({
            "plan_type": "free",
            "image_quota_remaining": 19,
            "image_quota_total": 25,
        });
        normalize_chatgpt_web_image_quota_limit(&mut metadata, None);

        assert_eq!(metadata["image_quota_total"], json!(19.0));
        assert_eq!(metadata["image_quota_used"], json!(0.0));
        assert_eq!(
            metadata["image_quota_limit_source"],
            json!("first_remaining")
        );
    }

    #[test]
    fn chatgpt_web_quota_metadata_preserves_marked_free_first_limit() {
        let mut metadata = json!({
            "plan_type": "free",
            "image_quota_remaining": 18,
        });
        normalize_chatgpt_web_image_quota_limit(
            &mut metadata,
            Some(&json!({
                "chatgpt_web": {
                    "plan_type": "free",
                    "image_quota_total": 19,
                    "image_quota_limit_source": "first_remaining"
                }
            })),
        );

        assert_eq!(metadata["image_quota_total"], json!(19.0));
        assert_eq!(metadata["image_quota_used"], json!(1.0));
        assert_eq!(
            metadata["image_quota_limit_source"],
            json!("first_remaining")
        );
    }

    #[test]
    fn windsurf_quota_request_uses_user_status_connect_rpc() {
        let spec = build_windsurf_pool_quota_request("key-ws", "session-token-123");

        assert_eq!(spec.request_id, "windsurf-quota:key-ws");
        assert_eq!(spec.method, "POST");
        assert_eq!(
            spec.url,
            format!("https://server.codeium.com{WINDSURF_USER_STATUS_PATH}")
        );
        assert_eq!(spec.content_type.as_deref(), Some("application/json"));
        assert_eq!(
            spec.headers
                .get("connect-protocol-version")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            spec.json_body
                .as_ref()
                .and_then(|body| body.pointer("/metadata/apiKey"))
                .and_then(Value::as_str),
            Some("session-token-123")
        );
        assert_eq!(spec.provider_api_format, "windsurf:user_status");
    }

    #[test]
    fn windsurf_model_and_rate_limit_requests_use_connect_rpc_metadata() {
        let models = build_windsurf_pool_model_configs_request("key-ws", "api-key-123");
        let rate_limit = build_windsurf_pool_rate_limit_request("key-ws", "api-key-123");

        assert_eq!(
            models.url,
            format!("https://server.codeium.com{WINDSURF_MODEL_CONFIGS_PATH}")
        );
        assert_eq!(
            rate_limit.url,
            format!("https://server.codeium.com{WINDSURF_RATE_LIMIT_PATH}")
        );
        for spec in [models, rate_limit] {
            assert_eq!(spec.method, "POST");
            assert_eq!(
                spec.headers
                    .get("connect-protocol-version")
                    .map(String::as_str),
                Some("1")
            );
            assert_eq!(
                spec.json_body
                    .as_ref()
                    .and_then(|body| body.pointer("/metadata/apiKey"))
                    .and_then(Value::as_str),
                Some("api-key-123")
            );
            assert_eq!(spec.client_api_format, "openai:chat");
        }
    }

    /// 验证新增的模型上下文为空时不改变 Windsurf 限速元数据的可调度语义。
    #[test]
    fn windsurf_rate_limit_metadata_keeps_member_schedulable() {
        let service = ProviderPoolService::with_builtin_adapters();
        let key = sample_key(Some(json!({
            "windsurf": {
                "updated_at": 1_700_000_000u64,
                "rate_limit": {
                    "limited": true,
                    "retry_after_ms": 60_000
                }
            }
        })));

        let signals = service.member_signals("windsurf", &key, None, None);

        assert!(!signals.quota_exhausted);
    }

    /// 验证新增的模型上下文为空时仍保留 Windsurf 封禁快照的账号级耗尽语义。
    #[test]
    fn windsurf_status_snapshot_ban_marks_member_exhausted() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(Some(json!({
            "windsurf": {
                "updated_at": 1_700_000_000u64,
                "daily_remaining_percent": 100.0
            }
        })));
        key.status_snapshot = Some(json!({
            "quota": {
                "provider_type": "windsurf",
                "code": "banned",
                "exhausted": false,
                "windows": [{
                    "code": "daily",
                    "used_ratio": 0.0,
                    "remaining_ratio": 1.0
                }]
            }
        }));

        let signals = service.member_signals("windsurf", &key, None, None);

        assert!(signals.quota_exhausted);
    }

    #[test]
    fn preset_payload_derives_provider_support_from_capabilities() {
        let payload = build_admin_pool_scheduling_presets_payload();
        let items = payload.as_array().expect("payload should be array");
        let free_first = items
            .iter()
            .find(|item| item["name"] == "free_first")
            .expect("free_first should exist");
        let recent_refresh = items
            .iter()
            .find(|item| item["name"] == "recent_refresh")
            .expect("recent_refresh should exist");
        let legacy_free_team = items
            .iter()
            .find(|item| item["name"] == "free_team_first")
            .expect("legacy free_team_first should remain configurable");

        assert_eq!(
            free_first["providers"],
            json!(["codex", "grok", "kiro", "nous", "windsurf"])
        );
        assert_eq!(
            recent_refresh["providers"],
            json!(["codex", "grok", "kiro", "nous", "windsurf"])
        );
        assert_eq!(free_first["default_enabled"], json!(false));
        assert_eq!(recent_refresh["default_enabled"], json!(false));
        assert_eq!(
            recent_refresh["default_enabled_providers"],
            json!(["codex", "windsurf"])
        );
        assert_eq!(legacy_free_team["default_mode"], json!("both"));
        assert_eq!(legacy_free_team["modes"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn quota_metadata_provider_type_comes_from_pool_registry() {
        assert_eq!(
            provider_pool_quota_metadata_provider_type(&json!({
                "gemini_cli": {
                    "updated_at": 1_700_000_000u64
                }
            }))
            .as_deref(),
            Some("gemini_cli")
        );
        assert_eq!(
            provider_pool_quota_metadata_provider_type(&json!({
                "custom_provider": {
                    "updated_at": 1_700_000_000u64
                }
            }))
            .as_deref(),
            Some("custom_provider")
        );
    }

    #[test]
    fn codex_adapter_injects_recent_refresh_and_filters_by_capability() {
        let service = ProviderPoolService::with_builtin_adapters();
        let normalized = service.normalize_scheduling_presets(
            "codex",
            &[PoolSchedulingPreset {
                preset: "cache_affinity".to_string(),
                enabled: true,
                mode: None,
            }],
        );

        assert_eq!(
            normalized
                .iter()
                .map(|preset| preset.preset.as_str())
                .collect::<Vec<_>>(),
            ["cache_affinity", "recent_refresh"]
        );

        let legacy_free_team = service.normalize_scheduling_presets(
            "codex",
            &[PoolSchedulingPreset {
                preset: "free_team_first".to_string(),
                enabled: true,
                mode: Some("team_only".to_string()),
            }],
        );
        assert_eq!(legacy_free_team[0].preset, "free_team_first");
        assert_eq!(legacy_free_team[0].mode.as_deref(), Some("team_only"));

        let unsupported = service.normalize_scheduling_presets(
            "chatgpt_web",
            &[PoolSchedulingPreset {
                preset: "plus_first".to_string(),
                enabled: true,
                mode: None,
            }],
        );
        assert!(unsupported.is_empty());
    }

    #[test]
    fn provider_quota_exhaustion_is_adapter_owned() {
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "allowed": false,
                    "limit_reached": true
                }
            }))),
            "codex",
        ));
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "has_credits": false,
                    "credits_unlimited": false
                }
            }))),
            "codex",
        ));
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "kiro": {
                    "remaining": 0
                }
            }))),
            "kiro",
        ));
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "chatgpt_web": {
                    "image_quota_blocked": true
                }
            }))),
            "chatgpt_web",
        ));
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "grok": {
                    "quota_by_model": {
                        "quota_fast": {
                            "is_exhausted": true,
                            "remaining": 0.0
                        }
                    }
                }
            }))),
            "grok",
        ));
        assert!(!provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "grok": {
                    "pool_tier": "basic",
                    "quota_by_model": {
                        "quota_fast": {
                            "is_exhausted": false,
                            "remaining": 1.0
                        },
                        "quota_heavy": {
                            "is_exhausted": true,
                            "remaining": 0.0
                        }
                    }
                }
            }))),
            "grok",
        ));
        assert!(!provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "has_credits": false,
                    "credits_unlimited": true
                }
            }))),
            "codex",
        ));
        assert!(!provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "allowed": true,
                    "primary_used_percent": 100.0
                }
            }))),
            "codex",
        ));

        let mut explicit_codex_limit = sample_key(None);
        explicit_codex_limit.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "allowed": false,
                "limit_reached": true,
                "usage_ratio": 0.91,
                "windows": [{
                    "code": "weekly",
                    "used_ratio": 0.91
                }]
            }
        }));
        assert!(provider_pool_key_account_quota_exhausted(
            &explicit_codex_limit,
            "codex",
        ));
    }

    #[test]
    fn provider_quota_exhaustion_snapshot_expires_after_reset_at() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();

        let mut expired = sample_key(None);
        expired.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "code": "exhausted",
                "exhausted": true,
                "updated_at": now.saturating_sub(600),
                "windows": [{
                    "code": "5h",
                    "used_ratio": 1.0,
                    "reset_at": now.saturating_sub(60),
                    "is_exhausted": true
                }]
            }
        }));
        assert!(!provider_pool_key_account_quota_exhausted(
            &expired, "codex"
        ));

        let mut active = sample_key(None);
        active.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "code": "exhausted",
                "exhausted": true,
                "updated_at": now,
                "windows": [{
                    "code": "5h",
                    "used_ratio": 1.0,
                    "reset_at": now.saturating_add(3600),
                    "is_exhausted": true
                }]
            }
        }));
        assert!(provider_pool_key_account_quota_exhausted(&active, "codex"));
    }

    #[test]
    fn balance_snapshot_is_observation_only_even_with_legacy_exhausted_flag() {
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "schema_version": 1,
                "kind": "balance",
                "provider_type": "codex",
                "code": "exhausted",
                "exhausted": true,
                "balances": [{"unit": "USD", "available": "0"}],
                "refresh_state": {"last_success_at": 1_700_000_000u64}
            }
        }));

        assert!(!provider_pool_key_account_quota_exhausted(&key, "codex"));
    }

    /// 验证一个 Antigravity 模型耗尽不会阻断同一 Key 上仍有额度的其他模型。
    #[test]
    fn antigravity_model_quota_exhaustion_does_not_block_other_models() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [
                    {
                        "code": "model:gemini-3.1-pro-high",
                        "scope": "model",
                        "model": "gemini-3.1-pro-high",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    },
                    {
                        "code": "model:gemini-3-flash-agent",
                        "scope": "model",
                        "model": "gemini-3-flash-agent",
                        "used_ratio": 0.1,
                        "is_exhausted": false
                    }
                ]
            }
        }));

        let exhausted =
            service.member_signals("antigravity", &key, None, Some("gemini-3.1-pro-high"));
        let available =
            service.member_signals("antigravity", &key, None, Some("gemini-3-flash-agent"));

        assert!(exhausted.quota_exhausted);
        assert!(!available.quota_exhausted);
    }

    /// 验证 Codex 标准与 Spark 模型族只消费各自匹配的额度窗口。
    #[test]
    fn codex_standard_and_spark_quota_families_are_independent() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut standard_exhausted = sample_key(None);
        standard_exhausted.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "allowed": false,
                "limit_reached": true,
                "windows": [
                    { "code": "weekly", "used_ratio": 1.0, "is_exhausted": true },
                    { "code": "5h", "used_ratio": 0.5, "is_exhausted": false },
                    { "code": "spark_weekly", "used_ratio": 0.2, "is_exhausted": false },
                    { "code": "spark_5h", "used_ratio": 0.1, "is_exhausted": false }
                ]
            }
        }));

        let standard =
            service.member_signals("codex", &standard_exhausted, None, Some("gpt-5.3-codex"));
        let spark = service.member_signals(
            "codex",
            &standard_exhausted,
            None,
            Some("gpt-5.3-codex-spark"),
        );
        assert!(standard.quota_exhausted);
        assert!(!standard.quota_hard_blocked);
        assert!(!spark.quota_exhausted);
        assert!(!spark.quota_hard_blocked);

        let mut spark_exhausted = sample_key(None);
        spark_exhausted.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": false,
                "windows": [
                    { "code": "weekly", "used_ratio": 0.2, "is_exhausted": false },
                    { "code": "5h", "used_ratio": 0.1, "is_exhausted": false },
                    { "code": "spark_weekly", "used_ratio": 1.0, "is_exhausted": true },
                    { "code": "spark_5h", "used_ratio": 0.4, "is_exhausted": false }
                ]
            }
        }));

        let standard =
            service.member_signals("codex", &spark_exhausted, None, Some("gpt-5.3-codex"));
        let spark =
            service.member_signals("codex", &spark_exhausted, None, Some("gpt-5.3-codex-spark"));
        assert!(!standard.quota_exhausted);
        assert!(spark.quota_exhausted);
    }

    #[test]
    fn provider_quota_exhaustion_metadata_expires_after_reset_at() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();

        assert!(!provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "updated_at": now.saturating_sub(600),
                    "allowed": false,
                    "limit_reached": true,
                    "primary_used_percent": 100.0,
                    "primary_reset_at": now.saturating_sub(60)
                }
            }))),
            "codex",
        ));
        assert!(provider_pool_key_account_quota_exhausted(
            &sample_key(Some(json!({
                "codex": {
                    "updated_at": now,
                    "primary_used_percent": 100.0,
                    "primary_reset_at": now.saturating_add(3600)
                }
            }))),
            "codex",
        ));
    }

    #[test]
    fn codex_explicit_quota_block_is_hard_until_reset() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();

        assert!(provider_pool_key_quota_hard_blocked(
            &sample_key(Some(json!({
                "codex": {
                    "updated_at": now,
                    "allowed": false,
                    "limit_reached": true,
                    "primary_reset_at": now.saturating_add(3600)
                }
            }))),
            "codex",
        ));
        assert!(!provider_pool_key_quota_hard_blocked(
            &sample_key(Some(json!({
                "codex": {
                    "updated_at": now,
                    "primary_used_percent": 100.0,
                    "primary_reset_at": now.saturating_add(3600)
                }
            }))),
            "codex",
        ));
        assert!(!provider_pool_key_quota_hard_blocked(
            &sample_key(Some(json!({
                "codex": {
                    "updated_at": now.saturating_sub(600),
                    "allowed": false,
                    "limit_reached": true,
                    "primary_reset_at": now.saturating_sub(60)
                }
            }))),
            "codex",
        ));

        let mut snapshot_blocked = sample_key(None);
        snapshot_blocked.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "allowed": false,
                "limit_reached": true,
                "usage_ratio": 0.91,
                "reset_at": now.saturating_add(3600)
            }
        }));
        assert!(provider_pool_key_quota_hard_blocked(
            &snapshot_blocked,
            "codex",
        ));
        snapshot_blocked.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "allowed": false,
                "limit_reached": true,
                "usage_ratio": 0.91,
                "reset_at": now.saturating_sub(60)
            }
        }));
        assert!(!provider_pool_key_quota_hard_blocked(
            &snapshot_blocked,
            "codex",
        ));
    }

    #[test]
    fn grok_quota_tier_boundaries_match_pool_modes() {
        assert_eq!(
            grok_supported_quota_windows_for_tier(Some("basic")),
            [("quota_fast", "fast")]
        );
        assert_eq!(
            grok_supported_quota_windows_for_tier(Some("super")),
            [
                ("quota_auto", "auto"),
                ("quota_fast", "fast"),
                ("quota_expert", "expert"),
                ("quota_grok_4_3", "grok-420-computer-use-sa")
            ]
        );
        assert_eq!(
            grok_supported_quota_windows_for_tier(Some("heavy")),
            [
                ("quota_auto", "auto"),
                ("quota_fast", "fast"),
                ("quota_expert", "expert"),
                ("quota_heavy", "heavy"),
                ("quota_grok_4_3", "grok-420-computer-use-sa")
            ]
        );
    }

    #[test]
    fn grok_pool_tier_infers_from_live_quota_totals() {
        let bucket = json!({
            "quota_by_model": {
                "quota_fast": {
                    "remaining": 20.0,
                    "total": 30.0
                },
                "quota_auto": {
                    "remaining": 7.0,
                    "total": 7.0
                }
            }
        });
        let bucket = bucket.as_object().expect("bucket should be object");

        assert_eq!(grok_pool_tier_from_quota_bucket(bucket), Some("basic"));
    }

    #[test]
    fn grok_model_name_maps_to_quota_window() {
        assert_eq!(
            grok_quota_window_key_for_model(Some("grok-4.20-fast")),
            Some("quota_fast")
        );
        assert_eq!(
            grok_quota_window_key_for_model(Some("grok-4.20-multi-agent-0309")),
            Some("quota_heavy")
        );
        assert_eq!(
            grok_quota_window_key_for_model(Some("grok-4.3-beta")),
            Some("quota_grok_4_3")
        );
    }

    #[test]
    fn plan_tier_derivation_normalizes_provider_prefix() {
        let key = sample_key(Some(json!({
            "codex": {
                "plan_type": "codex:Plus"
            }
        })));

        assert_eq!(
            derive_oauth_plan_type("codex", &key, None).as_deref(),
            Some("plus")
        );
    }

    #[test]
    fn plan_tier_derivation_reads_quota_snapshot() {
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "plan_type": "team"
            }
        }));

        assert_eq!(
            derive_oauth_plan_type("codex", &key, None).as_deref(),
            Some("team")
        );
    }
}
mod singleflight;
pub use singleflight::AsyncSingleflight;
