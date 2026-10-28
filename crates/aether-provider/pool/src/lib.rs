mod background_limiter;
mod capability;
mod plan;
mod presets;
mod provider;
mod quota;
mod quota_refresh;
mod quota_snapshot;
mod quota_sources;
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
    build_antigravity_pool_quota_request, build_antigravity_pool_quota_summary_request,
    build_chatgpt_web_pool_quota_request, build_claude_code_pool_quota_request,
    build_codex_pool_quota_request, build_codex_pool_reset_credit_consume_request,
    build_codex_pool_reset_credits_request, build_deepseek_balance_request,
    build_gemini_cli_pool_quota_request, build_kiro_pool_quota_request,
    build_nous_account_quota_request, build_nous_billing_quota_request,
    build_openrouter_credits_request, build_windsurf_pool_model_configs_request,
    build_windsurf_pool_model_configs_request_with_base_url, build_windsurf_pool_quota_request,
    build_windsurf_pool_quota_request_with_base_url, build_windsurf_pool_rate_limit_request,
    build_windsurf_pool_rate_limit_request_with_base_url, build_xai_pool_billing_request,
    build_xai_pool_user_request, clamp_official_balance_execution_timeouts,
    deepseek_quota_url_host_is_allowed, enrich_chatgpt_web_quota_metadata, grok_mode_id_for_model,
    grok_pool_tier_from_quota_bucket, grok_quota_window_key_for_model,
    grok_supported_quota_windows_for_tier, is_official_deepseek_endpoint,
    is_official_openrouter_endpoint, normalize_chatgpt_web_image_quota_limit,
    openrouter_quota_url_host_is_allowed, parse_deepseek_balance, parse_openrouter_credits,
    AntigravityProviderPoolAdapter, ChatGptWebProviderPoolAdapter, ClaudeCodeProviderPoolAdapter,
    CodexProviderPoolAdapter, DeepSeekProviderPoolAdapter, DefaultProviderPoolAdapter,
    GeminiCliProviderPoolAdapter, GrokProviderPoolAdapter, KiroPoolQuotaAuthInput,
    KiroProviderPoolAdapter, NousProviderPoolAdapter, OpenRouterProviderPoolAdapter,
    UnsupportedQuotaProviderPoolAdapter, XaiProviderPoolAdapter,
    ANTIGRAVITY_FETCH_AVAILABLE_MODELS_PATH, ANTIGRAVITY_RETRIEVE_USER_QUOTA_SUMMARY_PATH,
    CHATGPT_WEB_CONVERSATION_INIT_PATH, CHATGPT_WEB_DEFAULT_BASE_URL,
    CODEX_WHAM_RESET_CREDITS_CONSUME_URL, CODEX_WHAM_RESET_CREDITS_URL, CODEX_WHAM_USAGE_URL,
    DEEPSEEK_BALANCE_URL, GEMINI_CLI_RETRIEVE_USER_QUOTA_PATH, GEMINI_CLI_USER_AGENT,
    KIRO_USAGE_LIMITS_PATH, KIRO_USAGE_SDK_VERSION, OPENROUTER_CREDITS_URL,
    WINDSURF_MODEL_CONFIGS_PATH, WINDSURF_RATE_LIMIT_PATH, WINDSURF_USER_STATUS_PATH,
    XAI_BILLING_PATH, XAI_USER_PATH,
};
pub use providers::{
    build_minimax_balance_request, build_minimax_token_plan_request,
    build_official_api_key_quota_request, build_zhipu_account_balance_request,
    build_zhipu_team_quota_request, is_official_api_key_quota_endpoint,
    is_retired_official_api_key_quota_endpoint, official_api_key_quota_sources,
    official_api_key_quota_url_host_is_allowed, parse_official_api_key_quota,
    parse_official_api_key_quota_for_endpoint, parse_zhipu_standard_balance,
    OfficialApiKeyQuotaProvider, OfficialApiKeyQuotaProviderPoolAdapter, ZHIPU_ACCOUNT_REPORT_URL,
    ZHIPU_TEAM_QUOTA_URL,
};
pub use quota::{
    provider_pool_codex_metadata_has_account_quota, provider_pool_key_account_quota_exhausted,
    provider_pool_key_balance_below_minimum, provider_pool_key_minimum_quota_reached,
    provider_pool_key_model_quota_exhausted, provider_pool_key_model_quota_hard_blocked,
    provider_pool_key_quota_hard_blocked, provider_pool_key_runtime_quota_blocked,
    provider_pool_key_scheduling_label, provider_pool_key_supports_discovered_model,
    provider_pool_member_quota_snapshot, provider_pool_quota_metadata_provider_type,
    provider_pool_quota_metadata_updated_at, provider_pool_quota_snapshot_updated_at,
    provider_pool_reset_deadline_elapsed, resolve_provider_pool_key_thinking_model,
};
pub use quota_refresh::{
    official_balance_backoff_secs, official_balance_backoff_with_jitter_secs,
    ProviderPoolQuotaRequestSpec, OFFICIAL_BALANCE_MAX_BACKOFF_SECS,
    OFFICIAL_BALANCE_MIN_BACKOFF_SECS,
};
pub use quota_snapshot::{
    ProviderQuotaBalance, ProviderQuotaQueryStatus, ProviderQuotaRefreshState,
    ProviderQuotaSnapshotContract, ProviderQuotaSnapshotKind, ProviderQuotaSource,
    ProviderQuotaValue, ProviderQuotaWindow, PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
};
pub use quota_sources::provider_quota_snapshot_exhausted;
pub use service::ProviderPoolService;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::provider_pool_quota_snapshot_exhausted_decision;
    use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
    use aether_pool_core::PoolSchedulingPreset;
    use aether_provider_transport::antigravity::AntigravityThinkingLevel;
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

    fn openrouter_snapshot(available: &str, free_window: Value) -> Value {
        json!({
            "schema_version": PROVIDER_QUOTA_SNAPSHOT_SCHEMA_VERSION,
            "provider_type": "openrouter",
            "kind": "balance",
            "sources": [
                {
                    "id": "key_limit",
                    "label": "Key 消费限额",
                    "product": "key_spending_limit",
                    "scope": "key",
                    "query_status": "ok",
                    "freshness": "fresh"
                },
                {
                    "id": "free_requests",
                    "label": "免费模型每日请求",
                    "product": "model_requests",
                    "scope": "model",
                    "query_status": "ok",
                    "freshness": "fresh"
                }
            ],
            "balances": [
                {"source_id": "key_limit", "unit": "USD", "available": available}
            ],
            "windows": [free_window]
        })
    }

    #[test]
    fn openrouter_free_requests_source_does_not_cancel_key_limit_threshold() {
        // 免费模型请求数与 Key 消费限额并存时，金额阈值事实必须独立成立。
        let low = sample_key_with_quota(openrouter_snapshot(
            "0.5",
            json!({
                "source_id": "free_requests",
                "code": "free_model_daily_requests",
                "scope": "model",
                "unit": "requests",
                "is_exhausted": false
            }),
        ));
        assert!(provider_pool_key_balance_below_minimum(&low, "openrouter"));

        let healthy = sample_key_with_quota(openrouter_snapshot(
            "100",
            json!({
                "source_id": "free_requests",
                "code": "free_model_daily_requests",
                "scope": "model",
                "unit": "requests",
                "is_exhausted": false
            }),
        ));
        assert!(!provider_pool_key_balance_below_minimum(
            &healthy,
            "openrouter"
        ));
    }

    #[test]
    fn exhausted_free_requests_window_never_blocks_paid_account_scheduling() {
        // 免费次数耗尽是模型级观察事实，不能形成账号级耗尽或金额阻断。
        let key = sample_key_with_quota(openrouter_snapshot(
            "100",
            json!({
                "source_id": "free_requests",
                "code": "free_model_daily_requests",
                "scope": "model",
                "unit": "requests",
                "is_exhausted": true,
                "reset_at": 4_102_444_800u64
            }),
        ));
        assert!(!provider_pool_key_balance_below_minimum(&key, "openrouter"));
        assert!(!provider_pool_key_account_quota_exhausted(
            &key,
            "openrouter"
        ));
        assert_eq!(
            provider_pool_quota_snapshot_exhausted_decision(&key, "openrouter"),
            Some(false)
        );
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
    fn legacy_zhipu_balance_fallback_does_not_block_scheduling() {
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
        assert!(!provider_pool_key_account_quota_exhausted(
            &blocked, "zhipu"
        ));

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
                "minimax",
                "moonshot",
                "nous",
                "openrouter",
                "siliconflow",
                "vertex_ai",
                "windsurf",
                "xai",
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
                "claude_code",
                "codex",
                "deepseek",
                "gemini_cli",
                "grok",
                "kimi_coding",
                "kiro",
                "minimax",
                "moonshot",
                "nous",
                "openrouter",
                "siliconflow",
                "windsurf",
                "xai",
                "zai",
                "zhipu"
            ]
        );
        assert!(service.supports_quota_refresh("claude_code"));
        assert!(service.supports_quota_refresh("codex"));
        assert!(service.supports_quota_refresh("antigravity"));
        assert!(service.supports_quota_refresh("grok"));
        assert!(service.supports_quota_refresh("gemini_cli"));
        assert!(service.supports_quota_refresh("windsurf"));
        assert!(service.supports_quota_refresh("deepseek"));
        assert!(service.supports_quota_refresh("openrouter"));
        assert!(service.supports_quota_refresh("moonshot"));
        assert!(service.supports_quota_refresh("minimax"));
        assert!(service.supports_quota_refresh("kimi_coding"));
        assert!(service.supports_quota_refresh("siliconflow"));
        assert!(service.supports_quota_refresh("zhipu"));
        assert!(service.supports_quota_refresh("zai"));
        assert!(service.supports_quota_refresh("xai"));
        let claude_spec = build_claude_code_pool_quota_request(
            "key-1",
            ("authorization".to_string(), "Bearer access".to_string()),
        );
        assert_eq!(claude_spec.method, "GET");
        assert_eq!(
            claude_spec.url,
            "https://api.anthropic.com/api/oauth/usage?cedar_ember=1&skip_spend=1"
        );
        assert_eq!(
            claude_spec
                .headers
                .get("anthropic-beta")
                .map(String::as_str),
            Some("oauth-2025-04-20")
        );
        assert_eq!(
            service.quota_refresh_unsupported_message("vertex_ai"),
            "Vertex AI 暂不支持自动刷新额度：额度属于 Google Cloud 项目/区域配额"
        );
    }

    #[test]
    fn official_quota_providers_expose_typed_serving_policy() {
        let service = ProviderPoolService::with_builtin_adapters();

        for provider_type in [
            "deepseek",
            "openrouter",
            "moonshot",
            "siliconflow",
            "minimax",
        ] {
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
    fn kiro_quota_request_rejects_region_url_injection() {
        let spec = build_kiro_pool_quota_request(
            "key-1",
            &KiroPoolQuotaAuthInput {
                authorization_value: "Bearer sensitive-access-token".to_string(),
                api_region: "attacker.example/evil".to_string(),
                kiro_version: "0.3.210".to_string(),
                machine_id: "machine".to_string(),
                profile_arn: None,
            },
        );

        assert_eq!(
            spec.url,
            "https://q.us-east-1.amazonaws.com/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST&isEmailRequired=true"
        );
        assert_eq!(
            spec.headers.get("host").map(String::as_str),
            Some("q.us-east-1.amazonaws.com")
        );
        assert!(!spec.url.contains("attacker.example"));
        assert!(!spec
            .headers
            .get("host")
            .is_some_and(|value| value.contains("attacker.example")));
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
            json!(["codex", "grok", "kiro", "nous", "windsurf", "xai"])
        );
        assert_eq!(
            recent_refresh["providers"],
            json!(["codex", "grok", "kiro", "nous", "windsurf", "xai"])
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
    fn codex_minimum_quota_respects_boundary_and_provider() {
        for (used_percent, expected) in [
            (98.9, false),
            (98.99999, false),
            (99.0, true),
            (99.5, true),
            (100.0, true),
        ] {
            let key = sample_key(Some(json!({
                "codex": { "primary_used_percent": used_percent }
            })));
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", None),
                expected,
                "used_percent={used_percent}"
            );
            assert!(!provider_pool_key_minimum_quota_reached(&key, "kiro", None));
        }
        assert!(!provider_pool_key_minimum_quota_reached(
            &sample_key(None),
            "codex",
            None
        ));
    }

    #[test]
    fn codex_minimum_quota_short_model_names_still_use_account_windows() {
        for (used_percent, expected) in [(98.0, false), (99.0, true)] {
            let key = sample_key(Some(json!({
                "codex": { "primary_used_percent": used_percent }
            })));
            for model in ["o1", "o3", "", "   "] {
                assert_eq!(
                    provider_pool_key_minimum_quota_reached(&key, "codex", Some(model)),
                    expected,
                    "model={model:?}, used_percent={used_percent}"
                );
            }
        }
    }

    #[test]
    fn codex_minimum_quota_respects_windows_and_reset() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        for (reset_at, expected) in [(now + 3600, true), (now - 60, false)] {
            let mut key = sample_key(None);
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "updated_at": now - 600,
                    "windows": [
                        { "code": "5h", "used_ratio": 0.2 },
                        { "code": "weekly", "used_ratio": 0.99, "reset_at": reset_at }
                    ]
                }
            }));
            for model in [None, Some("gpt-5.4")] {
                assert_eq!(
                    provider_pool_key_minimum_quota_reached(&key, "codex", model),
                    expected
                );
            }
        }
    }

    #[test]
    fn codex_minimum_quota_checks_either_window_and_relative_resets() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        for prefix in ["primary", "secondary"] {
            for (reset_seconds, expected) in [(3600, true), (60, false)] {
                let key = sample_key(Some(json!({
                    "codex": {
                        "updated_at": now - 600,
                        "allowed": true,
                        "limit_reached": false,
                        format!("{prefix}_used_percent"): 99.0,
                        format!("{prefix}_reset_after_seconds"): reset_seconds
                    }
                })));
                for model in [None, Some("gpt-5.4")] {
                    assert_eq!(
                        provider_pool_key_minimum_quota_reached(&key, "codex", model),
                        expected,
                        "prefix={prefix}, reset_seconds={reset_seconds}, model={model:?}"
                    );
                }
                assert!(!provider_pool_key_account_quota_exhausted(&key, "codex"));
            }
        }
    }

    #[test]
    fn codex_minimum_quota_uses_newest_source_with_or_without_model() {
        for (snapshot_at, metadata_at, metadata_wins) in [
            (Some(200), Some(100), false),
            (Some(100), Some(200), true),
            (None, None, false),
            (Some(100), None, false),
            (None, Some(100), true),
        ] {
            for snapshot_reached in [false, true] {
                let mut key = sample_key(Some(json!({
                    "codex": {
                        "updated_at": metadata_at,
                        "primary_used_percent": if snapshot_reached { 20.0 } else { 99.0 }
                    }
                })));
                key.status_snapshot = Some(json!({
                    "quota": {
                        "provider_type": "codex",
                        "observed_at": snapshot_at,
                        "windows": [{
                            "code": "5h",
                            "used_ratio": if snapshot_reached { 0.99 } else { 0.2 },
                            "is_exhausted": false
                        }]
                    }
                }));
                for model in [None, Some("gpt-5.4")] {
                    assert_eq!(
                        provider_pool_key_minimum_quota_reached(&key, "codex", model),
                        snapshot_reached != metadata_wins,
                        "snapshot_at={snapshot_at:?}, metadata_at={metadata_at:?}, model={model:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn codex_minimum_quota_isolates_model_buckets_and_checks_each_model_window() {
        for explicit_model in [false, true] {
            let mut key = sample_key(None);
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "windows": [
                        { "code": "5h", "used_ratio": 0.2 },
                        {
                            "code": "spark_5h", "used_ratio": 0.99,
                            "model": if explicit_model { Some("spark") } else { None },
                            "is_exhausted": false
                        },
                        {
                            "code": "spark_weekly", "used_ratio": 0.2,
                            "model": if explicit_model { Some("spark") } else { None },
                            "is_exhausted": false
                        }
                    ]
                }
            }));
            assert!(provider_pool_key_minimum_quota_reached(
                &key,
                "codex",
                Some("gpt-5.3-codex-spark")
            ));
            for model in [None, Some("gpt-5.4")] {
                assert!(!provider_pool_key_minimum_quota_reached(
                    &key, "codex", model
                ));
            }
            assert_eq!(
                provider_pool_key_model_quota_exhausted(&key, "codex", "gpt-5.3-codex-spark"),
                Some(false)
            );
        }
        for (account_percent, spark_percent) in [(99.0, 20.0), (20.0, 99.0)] {
            let key = sample_key(Some(json!({
                "codex": {
                    "primary_used_percent": account_percent,
                    "spark_primary_used_percent": spark_percent,
                    "spark_secondary_used_percent": 20.0
                }
            })));
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.3-codex-spark")),
                spark_percent == 99.0
            );
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.4")),
                account_percent == 99.0
            );
        }
    }

    #[test]
    fn codex_minimum_quota_keeps_model_bucket_when_account_metadata_is_newer() {
        for (snapshot_ratio, account_percent) in [(0.2, 99.0), (0.99, 20.0)] {
            let mut key = sample_key(Some(json!({
                "codex": { "updated_at": 200, "primary_used_percent": account_percent }
            })));
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "observed_at": 100,
                    "windows": [{ "code": "spark_5h", "used_ratio": snapshot_ratio }]
                }
            }));
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.3-codex-spark")),
                snapshot_ratio == 0.99
            );
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.4")),
                account_percent == 99.0
            );

            key.upstream_metadata.as_mut().unwrap()["codex"]["spark_primary_used_percent"] =
                json!(account_percent);
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.3-codex-spark")),
                account_percent == 99.0,
                "the newer observation for the same model bucket must win"
            );
        }
    }

    #[test]
    fn codex_minimum_quota_supports_remaining_values_and_ignores_unknown_data() {
        for (metrics, expected) in [
            (json!({ "remaining_ratio": 0.01 }), true),
            (json!({ "remaining_percent": "1" }), true),
            (json!({ "remaining": 1, "limit": 100 }), true),
            (json!({ "used_percent": "99" }), true),
            (json!({ "used_ratio": null }), false),
            (json!({ "used_ratio": "NaN" }), false),
            (json!({ "remaining": 0, "limit": 0 }), false),
            (json!({ "remaining_ratio": 0.01001 }), false),
        ] {
            let key = sample_key(Some(json!({
                "codex": { "quota_by_model": { "spark": metrics } }
            })));
            assert_eq!(
                provider_pool_key_minimum_quota_reached(&key, "codex", Some("gpt-5.3-codex-spark")),
                expected,
                "metrics={metrics}"
            );
            assert!(!provider_pool_key_minimum_quota_reached(
                &key, "codex", None
            ));
        }
        let disabled = sample_key(Some(json!({
            "codex": { "primary_used_percent": 99.0, "primary_window_minutes": 0 }
        })));
        assert!(!provider_pool_key_minimum_quota_reached(
            &disabled, "codex", None
        ));

        let mut mismatched = sample_key(None);
        mismatched.status_snapshot = Some(json!({
            "quota": {
                "provider_type": "kiro",
                "windows": [{ "code": "weekly", "used_ratio": 0.99 }]
            }
        }));
        assert!(!provider_pool_key_minimum_quota_reached(
            &mismatched,
            "codex",
            None
        ));
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

    /// 验证模型级硬阻断也消费需人工恢复的 Key 级事实，不能被可用模型窗口解除。
    #[test]
    fn runtime_quota_block_survives_model_specific_available_quota() {
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "scheduling": {"code": "quota_exhausted", "blocked": true},
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [{
                    "code": "model:gemini-3.7-flash-tiered",
                    "scope": "model",
                    "model": "gemini-3.7-flash-tiered",
                    "is_exhausted": false
                }]
            }
        }));
        assert_eq!(
            provider_pool_key_model_quota_exhausted(&key, "antigravity", "gemini-3.7-flash-tiered"),
            Some(false)
        );
        assert!(provider_pool_key_model_quota_hard_blocked(
            &key,
            "antigravity",
            "gemini-3.7-flash-tiered"
        ));
        assert!(provider_pool_key_quota_hard_blocked(&key, "antigravity"));
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
        assert!(exhausted.quota_hard_blocked);
        assert!(!exhausted.runtime_quota_hard_blocked);
        assert!(!available.quota_exhausted);
        assert!(!available.quota_hard_blocked);
    }

    #[test]
    fn antigravity_tiered_model_quota_wins_over_display_only_group_windows() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": true,
                "windows": [{
                    "code": "model:gemini-3.7-flash-tiered",
                    "scope": "model",
                    "model": "gemini-3.7-flash-tiered",
                    "remaining_ratio": 0.906,
                    "used_ratio": 0.094,
                    "is_exhausted": false
                }, {
                    "code": "group:0:3p-5h",
                    "scope": "quota_group",
                    "quota_group": "group:0",
                    "bucket_id": "3p-5h",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals =
            service.member_signals("antigravity", &key, None, Some("gemini-3.7-flash-tiered"));

        assert!(!signals.quota_exhausted);
        assert!(!signals.quota_hard_blocked);
    }

    #[test]
    fn antigravity_tiered_variant_does_not_match_another_model_family() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [{
                    "code": "model:gemini-3.7-pro-tiered",
                    "scope": "model",
                    "model": "gemini-3.7-pro-tiered",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals =
            service.member_signals("antigravity", &key, None, Some("gemini-3.7-flash-tiered"));

        assert!(!signals.quota_exhausted);
        assert!(!signals.quota_hard_blocked);
    }

    /// 验证同系列可用兄弟模型不会掩盖目标 Antigravity 模型的耗尽状态，且正反方向均成立。
    #[test]
    fn antigravity_sibling_model_does_not_mask_exhausted_target_model() {
        let service = ProviderPoolService::with_builtin_adapters();
        for (first_model, second_model) in [
            ("claude-sonnet-4-5", "claude-sonnet-4-6"),
            ("claude-sonnet-4-5", "claude-sonnet-4-5-thinking"),
            ("gemini-3.7-pro-tiered", "gemini-3.7-flash-tiered"),
        ] {
            for (first_remaining, second_remaining) in [(0.0_f64, 0.8_f64), (0.8_f64, 0.0_f64)] {
                let mut snapshot_key = sample_key(None);
                snapshot_key.status_snapshot = Some(json!({
                    "quota": {
                        "version": 2,
                        "provider_type": "antigravity",
                        "exhausted": false,
                        "windows": [
                            {
                                "code": format!("model:{first_model}"),
                                "scope": "model",
                                "model": first_model,
                                "remaining_ratio": first_remaining,
                                "used_ratio": 1.0 - first_remaining,
                                "is_exhausted": first_remaining <= 0.0
                            },
                            {
                                "code": format!("model:{second_model}"),
                                "scope": "model",
                                "model": second_model,
                                "remaining_ratio": second_remaining,
                                "used_ratio": 1.0 - second_remaining,
                                "is_exhausted": second_remaining <= 0.0
                            }
                        ]
                    }
                }));
                let first_signals =
                    service.member_signals("antigravity", &snapshot_key, None, Some(first_model));
                let second_signals =
                    service.member_signals("antigravity", &snapshot_key, None, Some(second_model));
                assert_eq!(first_signals.quota_exhausted, first_remaining <= 0.0);
                assert_eq!(first_signals.quota_hard_blocked, first_remaining <= 0.0);
                assert!(!first_signals.runtime_quota_hard_blocked);
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &snapshot_key,
                        "antigravity",
                        first_model
                    ),
                    first_remaining <= 0.0
                );
                assert_eq!(second_signals.quota_exhausted, second_remaining <= 0.0);
                assert_eq!(second_signals.quota_hard_blocked, second_remaining <= 0.0);
                assert!(!second_signals.runtime_quota_hard_blocked);
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &snapshot_key,
                        "antigravity",
                        second_model
                    ),
                    second_remaining <= 0.0
                );

                let metadata_key = sample_key(Some(json!({
                    "antigravity": {
                        "updated_at": 1_700_000_000_u64,
                        "quota_by_model": {
                            (first_model): {
                                "remaining_fraction": first_remaining,
                                "used_percent": (1.0 - first_remaining) * 100.0
                            },
                            (second_model): {
                                "remaining_fraction": second_remaining,
                                "used_percent": (1.0 - second_remaining) * 100.0
                            }
                        }
                    }
                })));
                let first_meta =
                    service.member_signals("antigravity", &metadata_key, None, Some(first_model));
                let second_meta =
                    service.member_signals("antigravity", &metadata_key, None, Some(second_model));
                assert_eq!(first_meta.quota_exhausted, first_remaining <= 0.0);
                assert_eq!(first_meta.quota_hard_blocked, first_remaining <= 0.0);
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &metadata_key,
                        "antigravity",
                        first_model
                    ),
                    first_remaining <= 0.0
                );
                assert_eq!(second_meta.quota_exhausted, second_remaining <= 0.0);
                assert_eq!(second_meta.quota_hard_blocked, second_remaining <= 0.0);
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &metadata_key,
                        "antigravity",
                        second_model
                    ),
                    second_remaining <= 0.0
                );
            }
        }

        // Trimmed case-insensitive exact match succeeds; punctuation-stripped or substring forms do not.
        let exact_key = sample_key(Some(json!({
            "antigravity": {
                "updated_at": 1_700_000_000_u64,
                "quota_by_model": {
                    "  Claude-Sonnet-4-5  ": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                }
            }
        })));
        let exact_signals = service.member_signals(
            "antigravity",
            &exact_key,
            None,
            Some("  claude-sonnet-4-5 "),
        );
        assert!(exact_signals.quota_exhausted);
        assert!(exact_signals.quota_hard_blocked);
        for non_exact in ["claudesonnet45", "sonnet-4-5", "claude-sonnet"] {
            assert_eq!(
                provider_pool_key_model_quota_exhausted(&exact_key, "antigravity", non_exact),
                None,
                "non-exact identifier {non_exact} must not match"
            );
            let non_exact_signals =
                service.member_signals("antigravity", &exact_key, None, Some(non_exact));
            assert!(!non_exact_signals.quota_exhausted);
            assert!(!non_exact_signals.quota_hard_blocked);
        }

        // Every explicit model alias supports both scalar strings and string arrays in exact mode.
        for field in [
            "model",
            "model_name",
            "model_id",
            "quota_model",
            "quota_model_name",
            "target_model",
            "limit_name",
            "models",
            "model_ids",
        ] {
            for field_value in [
                json!(" Claude-Sonnet-4-5 "),
                json!(["other-model", " Claude-Sonnet-4-5 "]),
            ] {
                let mut window = serde_json::Map::new();
                window.insert("code".to_string(), json!("custom_window"));
                window.insert("scope".to_string(), json!("model"));
                window.insert(field.to_string(), field_value);
                window.insert("used_ratio".to_string(), json!(1.0));
                window.insert("is_exhausted".to_string(), json!(true));

                let mut alias_key = sample_key(None);
                alias_key.status_snapshot = Some(json!({
                    "quota": {
                        "version": 2,
                        "provider_type": "antigravity",
                        "exhausted": false,
                        "windows": [Value::Object(window)]
                    }
                }));
                assert_eq!(
                    provider_pool_key_model_quota_exhausted(
                        &alias_key,
                        "antigravity",
                        "claude-sonnet-4-5"
                    ),
                    Some(true),
                    "field={field}"
                );
                assert_eq!(
                    provider_pool_key_model_quota_exhausted(
                        &alias_key,
                        "antigravity",
                        "claude-sonnet-4-6"
                    ),
                    None,
                    "field={field}"
                );
                assert!(
                    provider_pool_key_model_quota_hard_blocked(
                        &alias_key,
                        "antigravity",
                        "claude-sonnet-4-5"
                    ),
                    "field={field}"
                );
                assert!(
                    !provider_pool_key_model_quota_hard_blocked(
                        &alias_key,
                        "antigravity",
                        "claude-sonnet-4-6"
                    ),
                    "field={field}"
                );
            }
        }

        // Multiple exact windows for the same model preserve existing `all` semantics.
        let mut multi_window_key = sample_key(None);
        multi_window_key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": false,
                "windows": [
                    {
                        "code": "short",
                        "scope": "model",
                        "model": "claude-sonnet-4-5",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    },
                    {
                        "code": "weekly",
                        "scope": "model",
                        "model": "claude-sonnet-4-5",
                        "used_ratio": 0.4,
                        "is_exhausted": false
                    }
                ]
            }
        }));
        let partial_signals = service.member_signals(
            "antigravity",
            &multi_window_key,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(!partial_signals.quota_exhausted);
        assert!(!partial_signals.quota_hard_blocked);
        multi_window_key.status_snapshot.as_mut().unwrap()["quota"]["windows"][1]["used_ratio"] =
            json!(1.0);
        multi_window_key.status_snapshot.as_mut().unwrap()["quota"]["windows"][1]["is_exhausted"] =
            json!(true);
        let all_exhausted_signals = service.member_signals(
            "antigravity",
            &multi_window_key,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(all_exhausted_signals.quota_exhausted);
        assert!(all_exhausted_signals.quota_hard_blocked);
    }

    /// 验证同一精确模型同时存在已知 0 额度（未来 reset）与未知/格式错误窗口时保持 `all`/fail-open 放行，而全未知窗口保持 `None`。
    #[test]
    fn antigravity_known_exhausted_plus_unknown_exact_window_preserves_fail_open_all_semantics() {
        let service = ProviderPoolService::with_builtin_adapters();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        let future_unix = now.saturating_add(3600);
        let future_rfc3339 = chrono::DateTime::<chrono::Utc>::from_timestamp(future_unix as i64, 0)
            .expect("future unix timestamp should be valid")
            .to_rfc3339();

        let known_snapshot_window = json!({
            "code": "model:claude-sonnet-4-5:short",
            "scope": "model",
            "model": "claude-sonnet-4-5",
            "remaining_ratio": 0.0,
            "used_ratio": 1.0,
            "reset_at": future_unix,
            "is_exhausted": true
        });
        let known_raw_entry = json!({
            "remaining_fraction": 0.0,
            "used_percent": 100.0,
            "reset_time": future_rfc3339.as_str()
        });
        let known_raw_window = json!({
            "code": "extra:claude-sonnet-4-5",
            "scope": "model",
            "model": "claude-sonnet-4-5",
            "remaining_fraction": 0.0,
            "used_percent": 100.0,
            "reset_time": future_rfc3339.as_str()
        });

        let unknown_payload_shapes = [
            ("empty", json!({})),
            (
                "display_and_reset_only",
                json!({
                    "display_name": "Claude Sonnet 4.5",
                    "reset_time": future_rfc3339
                }),
            ),
            (
                "malformed_bool_and_ratios",
                json!({
                    "is_exhausted": "maybe",
                    "exhausted": "unknown",
                    "used_ratio": "not-a-number",
                    "usage_ratio": "bad",
                    "used_percent": "NaN",
                    "remaining_ratio": "bad",
                    "remaining_fraction": "not-a-number",
                    "remaining_percent": "bad"
                }),
            ),
            ("missing_limit", json!({ "remaining": 0.0 })),
            ("missing_total", json!({ "remaining_value": 0.0 })),
            ("zero_limit", json!({ "remaining": 0.0, "limit": 0.0 })),
            (
                "negative_total",
                json!({ "remaining_value": 0.0, "total": -10.0 }),
            ),
        ];

        let assert_fail_open = |key: &StoredProviderCatalogKey, context: &str| {
            assert_eq!(
                provider_pool_key_model_quota_exhausted(key, "antigravity", "claude-sonnet-4-5"),
                Some(false),
                "{context}"
            );
            let signals =
                service.member_signals("antigravity", key, None, Some("claude-sonnet-4-5"));
            assert!(!signals.quota_exhausted, "{context}");
            assert!(!signals.quota_hard_blocked, "{context}");
            assert!(
                !provider_pool_key_model_quota_hard_blocked(
                    key,
                    "antigravity",
                    "claude-sonnet-4-5"
                ),
                "{context}"
            );
        };

        for (shape_name, unknown_payload) in &unknown_payload_shapes {
            let mut unknown_window_map = unknown_payload
                .as_object()
                .expect("unknown payload should be object")
                .clone();
            unknown_window_map.insert("code".to_string(), json!("model:claude-sonnet-4-5:weekly"));
            unknown_window_map.insert("scope".to_string(), json!("model"));
            unknown_window_map.insert("model".to_string(), json!("claude-sonnet-4-5"));
            let unknown_window = Value::Object(unknown_window_map);

            // Materialized snapshot `windows` in both orders.
            for (order_label, windows) in [
                (
                    "known_then_unknown",
                    json!([known_snapshot_window.clone(), unknown_window.clone()]),
                ),
                (
                    "unknown_then_known",
                    json!([unknown_window.clone(), known_snapshot_window.clone()]),
                ),
            ] {
                let mut snapshot_key = sample_key(None);
                snapshot_key.status_snapshot = Some(json!({
                    "quota": {
                        "version": 2,
                        "provider_type": "antigravity",
                        "observed_at": now,
                        "exhausted": false,
                        "windows": windows
                    }
                }));
                assert_fail_open(
                    &snapshot_key,
                    &format!("snapshot windows shape={shape_name} order={order_label}"),
                );
            }

            // Raw metadata: known `quota_by_model` + unknown `windows` or `additional_quota_windows`.
            for array_field in ["windows", "additional_quota_windows"] {
                let raw_key = sample_key(Some(json!({
                    "antigravity": {
                        "updated_at": now,
                        "quota_by_model": {
                            "claude-sonnet-4-5": known_raw_entry.clone()
                        },
                        (array_field): [unknown_window.clone()]
                    }
                })));
                assert_fail_open(
                    &raw_key,
                    &format!("raw quota_by_model + {array_field} shape={shape_name}"),
                );
            }

            // Raw metadata: unknown `quota_by_model` + known `additional_quota_windows`.
            let reverse_raw_key = sample_key(Some(json!({
                "antigravity": {
                    "updated_at": now,
                    "quota_by_model": {
                        "claude-sonnet-4-5": unknown_payload.clone()
                    },
                    "additional_quota_windows": [known_raw_window.clone()]
                }
            })));
            assert_fail_open(
                &reverse_raw_key,
                &format!("raw unknown quota_by_model + known additional shape={shape_name}"),
            );

            // Raw metadata: case/whitespace-distinct exact keys in `quota_by_model`.
            let case_variant_key = sample_key(Some(json!({
                "antigravity": {
                    "updated_at": now,
                    "quota_by_model": {
                        "claude-sonnet-4-5": known_raw_entry.clone(),
                        " Claude-Sonnet-4-5 ": unknown_payload.clone()
                    }
                }
            })));
            assert_fail_open(
                &case_variant_key,
                &format!("raw case-variant quota_by_model shape={shape_name}"),
            );

            // Legacy `models` map + unknown `additional_quota_windows`.
            let legacy_raw_key = sample_key(Some(json!({
                "antigravity": {
                    "updated_at": now,
                    "models": {
                        "claude-sonnet-4-5": known_raw_entry.clone()
                    },
                    "additional_quota_windows": [unknown_window.clone()]
                }
            })));
            assert_fail_open(
                &legacy_raw_key,
                &format!("legacy models + unknown additional shape={shape_name}"),
            );

            // All-unknown multi-window combinations must retain `None` rather than inventing `Some(false)`.
            let mut all_unknown_snapshot = sample_key(None);
            all_unknown_snapshot.status_snapshot = Some(json!({
                "quota": {
                    "version": 2,
                    "provider_type": "antigravity",
                    "observed_at": now,
                    "exhausted": false,
                    "windows": [
                        unknown_window.clone(),
                        {
                            "code": "account",
                            "scope": "account"
                        }
                    ],
                    "quota_by_model": {
                        "claude-sonnet-4-5": unknown_payload.clone()
                    }
                }
            }));
            assert_eq!(
                provider_pool_key_model_quota_exhausted(
                    &all_unknown_snapshot,
                    "antigravity",
                    "claude-sonnet-4-5"
                ),
                None,
                "all-unknown snapshot shape={shape_name}"
            );
            let all_unknown_signals = service.member_signals(
                "antigravity",
                &all_unknown_snapshot,
                None,
                Some("claude-sonnet-4-5"),
            );
            assert!(
                !all_unknown_signals.quota_exhausted,
                "all-unknown snapshot shape={shape_name}"
            );
            assert!(
                !all_unknown_signals.quota_hard_blocked,
                "all-unknown snapshot shape={shape_name}"
            );
        }

        // Unknown explicit-alias windows (scalar and array) paired with a known exhausted window also fail open.
        for field in [
            "model",
            "model_name",
            "model_id",
            "quota_model",
            "quota_model_name",
            "target_model",
            "limit_name",
            "models",
            "model_ids",
        ] {
            for field_value in [
                json!(" Claude-Sonnet-4-5 "),
                json!(["other-model", " Claude-Sonnet-4-5 "]),
            ] {
                let mut alias_unknown = serde_json::Map::new();
                alias_unknown.insert("code".to_string(), json!("alias_unknown"));
                alias_unknown.insert("scope".to_string(), json!("model"));
                alias_unknown.insert(field.to_string(), field_value);

                let mut alias_key = sample_key(None);
                alias_key.status_snapshot = Some(json!({
                    "quota": {
                        "version": 2,
                        "provider_type": "antigravity",
                        "observed_at": now,
                        "exhausted": false,
                        "windows": [
                            known_snapshot_window.clone(),
                            Value::Object(alias_unknown)
                        ]
                    }
                }));
                assert_fail_open(&alias_key, &format!("alias unknown field={field}"));
            }
        }
    }

    /// 验证缺失、空模型名、无效观测保持未知放行，且展示分组不会反向推断模型或账号耗尽。
    #[test]
    fn antigravity_missing_malformed_and_group_windows_do_not_guess_or_block_models() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(Some(json!({
            "antigravity": {
                "updated_at": 1_700_000_000_u64,
                "quota_by_model": {
                    "gemini-3-flash": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    },
                    "empty-entry": {},
                    "display-only": {
                        "display_name": "Display Only Model"
                    },
                    "malformed-fraction": {
                        "remaining_fraction": "not-a-number"
                    },
                    "missing-limit": {
                        "remaining": 0.0
                    },
                    "zero-limit": {
                        "remaining": 0.0,
                        "limit": 0.0
                    }
                }
            }
        })));
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "observed_at": 1_700_000_000_u64,
                "updated_at": 1_700_000_000_u64,
                "exhausted": true,
                "windows": [
                    {
                        "code": "model:gemini-3-flash",
                        "scope": "model",
                        "model": "gemini-3-flash",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    },
                    {
                        "code": "group:0:3p-5h",
                        "scope": "quota_group",
                        "quota_group": "claude",
                        "quota_group_label": "Claude and GPT models",
                        "bucket_id": "3p-5h",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    }
                ]
            }
        }));

        let flash_signals =
            service.member_signals("antigravity", &key, None, Some("gemini-3-flash"));
        assert!(flash_signals.quota_exhausted);
        assert!(flash_signals.quota_hard_blocked);
        for unknown_model in [
            "",
            "   ",
            "claude-sonnet-4-5",
            "empty-entry",
            "display-only",
            "malformed-fraction",
            "missing-limit",
            "zero-limit",
        ] {
            assert_eq!(
                provider_pool_key_model_quota_exhausted(&key, "antigravity", unknown_model),
                None,
                "model={unknown_model:?}"
            );
            let unknown_signals =
                service.member_signals("antigravity", &key, None, Some(unknown_model));
            assert!(!unknown_signals.quota_exhausted, "model={unknown_model:?}");
            assert!(
                !unknown_signals.quota_hard_blocked,
                "model={unknown_model:?}"
            );
        }
        assert!(!provider_pool_key_account_quota_exhausted(
            &key,
            "antigravity"
        ));
        assert!(!provider_pool_key_quota_hard_blocked(&key, "antigravity"));

        // Explicit structural account windows and windowless account snapshots still apply.
        let mut generic_account_key = sample_key(None);
        generic_account_key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": true,
                "windows": [{
                    "code": "account",
                    "scope": "account",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));
        let generic_signals = service.member_signals(
            "antigravity",
            &generic_account_key,
            None,
            Some("unlisted-model"),
        );
        assert!(generic_signals.quota_exhausted);
        assert!(generic_signals.quota_hard_blocked);
        assert!(provider_pool_key_account_quota_exhausted(
            &generic_account_key,
            "antigravity"
        ));
        assert!(provider_pool_key_quota_hard_blocked(
            &generic_account_key,
            "antigravity"
        ));

        let mut snapshot_fallback_key = sample_key(None);
        snapshot_fallback_key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "exhausted": true
            }
        }));
        let fallback_signals = service.member_signals(
            "antigravity",
            &snapshot_fallback_key,
            None,
            Some("unlisted-model"),
        );
        assert!(fallback_signals.quota_exhausted);
        assert!(fallback_signals.quota_hard_blocked);
    }

    /// 验证权威 `quota_by_model`（含空对象）优先于历史 `models`/`model_quotas`，同时保留仅含历史字段的兼容读取。
    #[test]
    fn antigravity_authoritative_quota_by_model_precedes_legacy_maps_without_resurrection() {
        let service = ProviderPoolService::with_builtin_adapters();
        let conflicting = sample_key(Some(json!({
            "antigravity": {
                "updated_at": 1_700_000_000_u64,
                "quota_by_model": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    },
                    "claude-sonnet-4-6": {
                        "remaining_fraction": 0.8,
                        "used_percent": 20.0
                    }
                },
                "models": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.9,
                        "used_percent": 10.0
                    },
                    "claude-sonnet-4-6": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    },
                    "legacy-only-model": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                },
                "model_quotas": {
                    "claude-sonnet-4-6": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                },
                "additional_quota_windows": [{
                    "code": "extra:gemini-2.5-pro",
                    "scope": "model",
                    "model": "gemini-2.5-pro",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        })));
        let conflicting_target =
            service.member_signals("antigravity", &conflicting, None, Some("claude-sonnet-4-5"));
        assert!(conflicting_target.quota_exhausted);
        assert!(conflicting_target.quota_hard_blocked);
        let conflicting_available =
            service.member_signals("antigravity", &conflicting, None, Some("claude-sonnet-4-6"));
        assert!(!conflicting_available.quota_exhausted);
        assert!(!conflicting_available.quota_hard_blocked);
        assert_eq!(
            provider_pool_key_model_quota_exhausted(
                &conflicting,
                "antigravity",
                "legacy-only-model"
            ),
            None
        );
        let conflicting_legacy =
            service.member_signals("antigravity", &conflicting, None, Some("legacy-only-model"));
        assert!(!conflicting_legacy.quota_exhausted);
        assert!(!conflicting_legacy.quota_hard_blocked);
        let conflicting_extra =
            service.member_signals("antigravity", &conflicting, None, Some("gemini-2.5-pro"));
        assert!(conflicting_extra.quota_exhausted);
        assert!(conflicting_extra.quota_hard_blocked);

        // An empty authoritative `quota_by_model` object must not resurrect legacy maps.
        let mut empty_authoritative = sample_key(Some(json!({
            "antigravity": {
                "updated_at": 1_700_000_000_u64,
                "quota_by_model": {},
                "models": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                },
                "model_quotas": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                }
            }
        })));
        empty_authoritative.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "antigravity",
                "observed_at": 1_700_000_000_u64,
                "exhausted": false,
                "quota_by_model": {},
                "models": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    }
                }
            }
        }));
        assert_eq!(
            provider_pool_key_model_quota_exhausted(
                &empty_authoritative,
                "antigravity",
                "claude-sonnet-4-5"
            ),
            None
        );
        let empty_signals = service.member_signals(
            "antigravity",
            &empty_authoritative,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(!empty_signals.quota_exhausted);
        assert!(!empty_signals.quota_hard_blocked);

        // Legacy `models`-only input (or non-object `quota_by_model`) remains supported.
        let legacy_models_only = sample_key(Some(json!({
            "antigravity": {
                "updated_at": 1_700_000_000_u64,
                "quota_by_model": Value::Null,
                "models": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0
                    },
                    "claude-sonnet-4-6": {
                        "remaining_fraction": 0.8,
                        "used_percent": 20.0
                    }
                }
            }
        })));
        let legacy_exhausted = service.member_signals(
            "antigravity",
            &legacy_models_only,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(legacy_exhausted.quota_exhausted);
        assert!(legacy_exhausted.quota_hard_blocked);
        let legacy_available = service.member_signals(
            "antigravity",
            &legacy_models_only,
            None,
            Some("claude-sonnet-4-6"),
        );
        assert!(!legacy_available.quota_exhausted);
        assert!(!legacy_available.quota_hard_blocked);

        // Non-Antigravity providers retain merged legacy model maps.
        let mut non_antigravity = sample_key(None);
        non_antigravity.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "grok",
                "exhausted": false,
                "quota_by_model": {
                    "vendor-alpha": { "remaining": 5.0, "total": 10.0 }
                },
                "models": {
                    "vendor-beta": { "remaining": 0.0, "total": 10.0 }
                }
            }
        }));
        assert!(
            !service
                .member_signals("grok", &non_antigravity, None, Some("vendor-alpha"))
                .quota_exhausted
        );
        assert!(
            service
                .member_signals("grok", &non_antigravity, None, Some("vendor-beta"))
                .quota_exhausted
        );
    }

    /// 逐账号模型准入只消费该账号自身的发现事实：权威映射精确匹配，缺失映射保留发现前行为。
    #[test]
    fn antigravity_discovered_model_admission_uses_exact_account_evidence() {
        // 事故形态：自动抓取关闭、无账号 allowlist，发现映射只包含一个无关/禁用占位模型。
        let incident_key = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-disabled": {
                        "remaining_fraction": 1.0
                    }
                }
            }
        })));
        assert!(!incident_key.auto_fetch_models);
        assert_eq!(incident_key.allowed_models, None);
        assert!(!provider_pool_key_supports_discovered_model(
            &incident_key,
            "antigravity",
            "gemini-4-argon-high"
        ));
        assert!(!provider_pool_key_supports_discovered_model(
            &incident_key,
            "Antigravity",
            "gemini-4-argon"
        ));

        let exact_key = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-high": {},
                    "gemini-4-flash": {}
                }
            }
        })));
        assert!(provider_pool_key_supports_discovered_model(
            &exact_key,
            "antigravity",
            " Gemini-4-Argon-High "
        ));
        // 兄弟模型、前缀和后缀变体都不得由精确身份推断出来。
        assert!(!provider_pool_key_supports_discovered_model(
            &exact_key,
            "antigravity",
            "gemini-4-argon"
        ));
        assert!(!provider_pool_key_supports_discovered_model(
            &exact_key,
            "antigravity",
            "gemini-4-argon-high-thinking"
        ));

        // 显式 disabled:true 的同名条目不算可用。
        let disabled_key = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": {
                    "gemini-4-argon-high": { "disabled": true }
                }
            }
        })));
        assert!(!provider_pool_key_supports_discovered_model(
            &disabled_key,
            "antigravity",
            "gemini-4-argon-high"
        ));

        // 权威空对象优先于历史回退映射，不得复活旧模型列表。
        let empty_authoritative = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": {},
                "models": {
                    "gemini-4-argon-high": {}
                }
            }
        })));
        assert!(!provider_pool_key_supports_discovered_model(
            &empty_authoritative,
            "antigravity",
            "gemini-4-argon-high"
        ));

        // 缺失或格式不正确的模型映射保留发现前的既有行为。
        assert!(provider_pool_key_supports_discovered_model(
            &sample_key(None),
            "antigravity",
            "gemini-4-argon-high"
        ));
        let malformed_maps = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": [],
                "models": "gemini-4-argon-high",
                "model_quotas": 1
            }
        })));
        assert!(provider_pool_key_supports_discovered_model(
            &malformed_maps,
            "antigravity",
            "gemini-4-argon-high"
        ));
        let legacy_models = sample_key(Some(json!({
            "antigravity": {
                "models": {
                    "gemini-4-argon-high": {}
                }
            }
        })));
        assert!(provider_pool_key_supports_discovered_model(
            &legacy_models,
            "antigravity",
            "gemini-4-argon-high"
        ));
        let legacy_model_quotas = sample_key(Some(json!({
            "antigravity": {
                "models": [],
                "model_quotas": {
                    "gemini-4-argon-high": {}
                }
            }
        })));
        assert!(provider_pool_key_supports_discovered_model(
            &legacy_model_quotas,
            "antigravity",
            "gemini-4-argon-high"
        ));

        // 其他 Provider 类型不使用该发现约束。
        let codex_key = sample_key(Some(json!({
            "codex": {
                "quota_by_model": {
                    "gpt-5": {}
                }
            }
        })));
        assert!(provider_pool_key_supports_discovered_model(
            &codex_key, "codex", "gpt-5.4"
        ));
        assert!(provider_pool_key_supports_discovered_model(
            &codex_key,
            "antigravity",
            "gemini-4-argon-high"
        ));
    }

    fn antigravity_thinking_key(models: Value) -> StoredProviderCatalogKey {
        sample_key(Some(json!({
            "antigravity": { "quota_by_model": models }
        })))
    }

    fn resolve_antigravity_thinking<'a>(
        key: &'a StoredProviderCatalogKey,
        selected: &'a str,
        requested: &str,
        level: Option<AntigravityThinkingLevel>,
    ) -> Option<&'a str> {
        resolve_provider_pool_key_thinking_model(key, "antigravity", requested, selected, level)
    }

    /// 账号隔离、权威首映射、空映射与禁用条目都由解析器直接消费。
    #[test]
    fn antigravity_thinking_model_resolution_uses_only_authoritative_account_map() {
        let selected = "gemini-4-argon-high";
        let low_account = antigravity_thinking_key(json!({
            "gemini-4-argon-low": {},
            "gemini-4-argon-high": {}
        }));
        let medium_account = antigravity_thinking_key(json!({
            "gemini-4-argon-medium": {},
            "gemini-4-argon-high": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &low_account,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some("gemini-4-argon-low")
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &medium_account,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Medium)
            ),
            Some("gemini-4-argon-medium")
        );
        // 另一账号没有的档位必须拒绝，不能借用本账号模型。
        assert_eq!(
            resolve_antigravity_thinking(
                &low_account,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Medium)
            ),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &medium_account,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            None
        );

        // 首个对象映射权威：空对象优先于后续 models，且空映射拒绝一切模型。
        let authoritative_empty = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": {},
                "models": { "gemini-4-argon-low": {} }
            }
        })));
        for level in [None, Some(AntigravityThinkingLevel::Low)] {
            assert_eq!(
                resolve_antigravity_thinking(
                    &authoritative_empty,
                    selected,
                    "gemini-4-argon",
                    level
                ),
                None
            );
        }
        let first_map_wins = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": { "gemini-4-argon-high": {} },
                "models": { "gemini-4-argon-low": {} }
            }
        })));
        assert_eq!(
            resolve_antigravity_thinking(
                &first_map_wins,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            None
        );
        // 首个对象缺失时继续使用后续模型映射。
        let legacy_models = sample_key(Some(json!({
            "antigravity": { "models": { "gemini-4-argon-low": {} } }
        })));
        assert_eq!(
            resolve_antigravity_thinking(
                &legacy_models,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some("gemini-4-argon-low")
        );

        // 已存在但禁用的目标档位拒绝；不能被其他档位替代。
        let disabled = antigravity_thinking_key(json!({
            "gemini-4-argon-low": { "disabled": true },
            "gemini-4-argon-high": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &disabled,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &disabled,
                selected,
                "gemini-4-argon-low",
                Some(AntigravityThinkingLevel::High)
            ),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &disabled,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::High)
            ),
            Some("gemini-4-argon-high")
        );
    }

    /// thinking 变体只在剥离已识别终结后缀后完全相同的模型族/版本内选择。
    #[test]
    fn antigravity_thinking_model_resolution_preserves_family_and_version_boundaries() {
        let key = antigravity_thinking_key(json!({
            "gemini-4-argon-2-low": {},
            "gemini-4.1-argon-low": {},
            "gemini-4-argon-ultra-low": {},
            "gemini-4-argon-low": {}
        }));
        let low = Some(AntigravityThinkingLevel::Low);
        assert_eq!(
            resolve_antigravity_thinking(&key, "gemini-4-argon-high", "gemini-4-argon", low),
            Some("gemini-4-argon-low")
        );
        assert_eq!(
            resolve_antigravity_thinking(&key, "gemini-4-argon-2-high", "gemini-4-argon", low),
            Some("gemini-4-argon-2-low")
        );
        assert_eq!(
            resolve_antigravity_thinking(&key, "gemini-4-argon-ultra-high", "gemini-4-argon", low),
            Some("gemini-4-argon-ultra-low")
        );
        // 同版本不同族/缺失版本都不跨边界借用。
        assert_eq!(
            resolve_antigravity_thinking(&key, "gemini-4.2-argon-high", "gemini-4-argon", low),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(&key, "gemini-4-argon-pro-high", "gemini-4-argon", low),
            None
        );
        // 未识别的后缀属于模型身份本身，仍按完整族匹配。
        let identity_key = antigravity_thinking_key(json!({
            "gemini-4-argon-thinking-low": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &identity_key,
                "gemini-4-argon-thinking-high",
                "gemini-4-argon",
                low
            ),
            Some("gemini-4-argon-thinking-low")
        );
    }

    /// 显式客户端后缀优先于请求体 effort，无 effort 保留默认映射。
    #[test]
    fn antigravity_thinking_model_resolution_honors_client_suffix_and_defaults() {
        let key = antigravity_thinking_key(json!({
            "gemini-4-argon-low": {},
            "gemini-4-argon-medium": {},
            "gemini-4-argon-high": {}
        }));
        let selected = "gemini-4-argon-high";
        assert_eq!(
            resolve_antigravity_thinking(
                &key,
                selected,
                "gemini-4-argon-low",
                Some(AntigravityThinkingLevel::High)
            ),
            Some("gemini-4-argon-low")
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &key,
                selected,
                "gemini-4-argon-xhigh",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some("gemini-4-argon-high")
        );
        // 显式后缀的目标档位缺失时拒绝，不静默回退到请求体档位。
        let missing = antigravity_thinking_key(json!({
            "gemini-4-argon-medium": {},
            "gemini-4-argon-high": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &missing,
                selected,
                "gemini-4-argon-low",
                Some(AntigravityThinkingLevel::High)
            ),
            None
        );
        // 无 effort 保留默认映射，即使该默认模型未被本账号发现。
        let default_only = antigravity_thinking_key(json!({ "gemini-4-argon-low": {} }));
        assert_eq!(
            resolve_antigravity_thinking(&default_only, selected, "gemini-4-argon", None),
            Some(selected)
        );
        // 精确命中的已发现身份保留其原始键。
        let cased = antigravity_thinking_key(json!({ "Gemini-4-Argon-High": {} }));
        assert_eq!(
            resolve_antigravity_thinking(
                &cased,
                "gemini-4-argon-high",
                "gemini-4-argon-high",
                None
            ),
            Some("Gemini-4-Argon-High")
        );
        // 基础模型 + 已发现兄弟：有 effort 时按档位动态选择。
        let base_family = antigravity_thinking_key(json!({
            "gemini-4-argon": {},
            "gemini-4-argon-low": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &base_family,
                "gemini-4-argon",
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some("gemini-4-argon-low")
        );
        // 无已发现变体族的基础模型保持既有行为；带后缀的默认映射则缺少兄弟。
        let base_only = antigravity_thinking_key(json!({ "gemini-4-argon": {} }));
        assert_eq!(
            resolve_antigravity_thinking(
                &base_only,
                "gemini-4-argon",
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::High)
            ),
            Some("gemini-4-argon")
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &base_only,
                "gemini-4-argon-high",
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::High)
            ),
            None
        );
    }

    /// xhigh/max/extra 归一化为 high，minimal 优先已发现档位并保留禁用语义。
    #[test]
    fn antigravity_thinking_model_resolution_normalizes_extended_efforts_and_minimal() {
        let key = antigravity_thinking_key(json!({
            "gemini-4-argon-extra-low": {},
            "gemini-4-argon-low": {},
            "gemini-4-argon-high": {}
        }));
        let selected = "gemini-4-argon-high";
        for effort in ["xhigh", "max", "extra", "HIGH"] {
            let level = AntigravityThinkingLevel::parse(effort)
                .unwrap_or_else(|| panic!("effort {effort} should normalize"));
            assert_eq!(
                resolve_antigravity_thinking(&key, selected, "gemini-4-argon", Some(level)),
                Some("gemini-4-argon-high"),
                "effort={effort}"
            );
        }
        let minimal = AntigravityThinkingLevel::parse("minimal").expect("minimal should parse");
        assert_eq!(
            resolve_antigravity_thinking(&key, selected, "gemini-4-argon", Some(minimal)),
            Some("gemini-4-argon-extra-low")
        );
        let low_only = antigravity_thinking_key(json!({ "gemini-4-argon-low": {} }));
        assert_eq!(
            resolve_antigravity_thinking(&low_only, selected, "gemini-4-argon", Some(minimal)),
            Some("gemini-4-argon-low")
        );
        // 已存在的禁用 minimal 不得为 low 让路。
        let disabled_minimal = antigravity_thinking_key(json!({
            "gemini-4-argon-minimal": { "disabled": true },
            "gemini-4-argon-low": {}
        }));
        assert_eq!(
            resolve_antigravity_thinking(
                &disabled_minimal,
                selected,
                "gemini-4-argon",
                Some(minimal)
            ),
            None
        );

        // 缺失发现与非 Antigravity/非 Gemini 模型保持既有选择。
        assert_eq!(
            resolve_antigravity_thinking(
                &sample_key(None),
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some(selected)
        );
        let malformed = sample_key(Some(json!({
            "antigravity": {
                "quota_by_model": [],
                "models": "gemini-4-argon-low",
                "model_quotas": 1
            }
        })));
        assert_eq!(
            resolve_antigravity_thinking(
                &malformed,
                selected,
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some(selected)
        );
        assert_eq!(
            resolve_provider_pool_key_thinking_model(
                &key,
                "codex",
                "gemini-4-argon",
                selected,
                Some(AntigravityThinkingLevel::Low)
            ),
            Some(selected)
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &key,
                "claude-4",
                "gemini-4-argon",
                Some(AntigravityThinkingLevel::Low)
            ),
            Some("claude-4")
        );
    }

    /// effort 敏感 Antigravity 选型的新目标或未变默认 high 仍须通过强 allowlist：
    /// 解析目标本身或显式客户端别名；其他 Provider/非 effort 请求保持枚举语义。
    #[test]
    fn antigravity_thinking_model_resolution_respects_strong_key_allowlists() {
        let selected = "gemini-4-argon-high";
        let key = antigravity_thinking_key(json!({
            "gemini-4-argon-low": {},
            "gemini-4-argon-high": {}
        }));
        let low = Some(AntigravityThinkingLevel::Low);

        // 只授权 high 的 Key 不得被 low 请求借用。
        let mut high_only = key.clone();
        high_only.allowed_models = Some(json!(["gemini-4-argon-high"]));
        assert_eq!(
            resolve_antigravity_thinking(&high_only, selected, "gemini-4-argon", low),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(
                &high_only,
                selected,
                "gemini-4-argon-low",
                Some(AntigravityThinkingLevel::High)
            ),
            None
        );
        // 未改变默认模型时沿用既有枚举结果。
        assert_eq!(
            resolve_antigravity_thinking(&high_only, selected, "gemini-4-argon", None),
            Some(selected)
        );

        // 客户端显式别名本身被允许时，可授权其映射出的低档变体。
        let mut alias_listed = key.clone();
        alias_listed.allowed_models = Some(json!(["gemini-4-argon"]));
        assert_eq!(
            resolve_antigravity_thinking(&alias_listed, selected, "gemini-4-argon", low),
            Some("gemini-4-argon-low")
        );

        // 解析目标本身被允许（low-only Key）。
        let mut low_only = key.clone();
        low_only.allowed_models = Some(json!(["gemini-4-argon-low"]));
        assert_eq!(
            resolve_antigravity_thinking(&low_only, selected, "gemini-4-argon", low),
            Some("gemini-4-argon-low")
        );

        // 延迟行级 allowlist 的 effort 请求即使仍解析为默认 high，也必须由目标或
        // 显式别名授权；low-only 必须拒绝 high。
        let high = Some(AntigravityThinkingLevel::High);
        assert_eq!(
            resolve_antigravity_thinking(&low_only, selected, "gemini-4-argon", high),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(&alias_listed, selected, "gemini-4-argon", high),
            Some(selected)
        );
        assert_eq!(
            resolve_antigravity_thinking(&high_only, selected, "gemini-4-argon", high),
            Some(selected)
        );
        // 非 effort 请求的未改变结果继续沿用枚举语义。
        assert_eq!(
            resolve_antigravity_thinking(&low_only, selected, "gemini-4-argon", None),
            Some(selected)
        );

        // 历史内嵌 JSON 字符串形态保持同样的列表语义。
        let mut embedded_alias = key.clone();
        embedded_alias.allowed_models = Some(json!("[\"gemini-4-argon\"]"));
        assert_eq!(
            resolve_antigravity_thinking(&embedded_alias, selected, "gemini-4-argon", low),
            Some("gemini-4-argon-low")
        );

        // 空列表在 effort 敏感路径拒绝；非 effort 请求仍交由既有枚举处理。
        let mut empty_list = key.clone();
        empty_list.allowed_models = Some(json!([]));
        assert_eq!(
            resolve_antigravity_thinking(&empty_list, selected, "gemini-4-argon", low),
            None
        );
        assert_eq!(
            resolve_antigravity_thinking(&empty_list, selected, "gemini-4-argon", None),
            Some(selected)
        );
        // 其他 Provider 与非 Gemini 模型的既有选择完全不受该 allowlist 复核影响。
        assert_eq!(
            resolve_provider_pool_key_thinking_model(
                &empty_list,
                "codex",
                "gemini-4-argon",
                selected,
                low
            ),
            Some(selected)
        );
        assert_eq!(
            resolve_antigravity_thinking(&empty_list, "claude-4", "gemini-4-argon", low),
            Some("claude-4")
        );
        let mut unrestricted = key.clone();
        unrestricted.allowed_models = None;
        assert_eq!(
            resolve_antigravity_thinking(&unrestricted, selected, "gemini-4-argon", low),
            Some("gemini-4-argon-low")
        );
    }

    /// 验证 `reset_time`/`next_reset_time`（Unix 与 RFC3339）、快照与原始元数据同时间戳一致性、非法重置时间保留耗尽及新观测恢复/阻断切换。
    #[test]
    fn antigravity_reset_aliases_timestamps_and_freshness_transitions_match_materialized_state() {
        let service = ProviderPoolService::with_builtin_adapters();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        let past_unix = now.saturating_sub(3600);
        let future_unix = now.saturating_add(3600);
        let observed_unix = now.saturating_sub(7200);
        let past_rfc3339 = chrono::DateTime::<chrono::Utc>::from_timestamp(past_unix as i64, 0)
            .expect("past unix timestamp should be valid")
            .to_rfc3339();
        let future_rfc3339 = chrono::DateTime::<chrono::Utc>::from_timestamp(future_unix as i64, 0)
            .expect("future unix timestamp should be valid")
            .to_rfc3339();

        for alias in ["reset_time", "next_reset_time"] {
            for (deadline_value, expected_exhausted, materialized_reset_at) in [
                (json!(future_unix), true, future_unix),
                (json!(future_unix.to_string()), true, future_unix),
                (json!(future_rfc3339.clone()), true, future_unix),
                (json!(past_unix), false, past_unix),
                (json!(past_unix.to_string()), false, past_unix),
                (json!(past_rfc3339.clone()), false, past_unix),
            ] {
                let raw_only = sample_key(Some(json!({
                    "antigravity": {
                        "updated_at": observed_unix,
                        "quota_by_model": {
                            "claude-sonnet-4-5": {
                                "remaining_fraction": 0.0,
                                "used_percent": 100.0,
                                (alias): deadline_value.clone()
                            }
                        }
                    }
                })));
                let raw_signals = service.member_signals(
                    "antigravity",
                    &raw_only,
                    None,
                    Some("claude-sonnet-4-5"),
                );
                assert_eq!(
                    raw_signals.quota_exhausted, expected_exhausted,
                    "raw alias={alias}, deadline={deadline_value}"
                );
                assert_eq!(
                    raw_signals.quota_hard_blocked, expected_exhausted,
                    "raw alias={alias}, deadline={deadline_value}"
                );
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &raw_only,
                        "antigravity",
                        "claude-sonnet-4-5"
                    ),
                    expected_exhausted,
                    "raw alias={alias}, deadline={deadline_value}"
                );

                // Materialized snapshot and raw metadata at the exact same timestamp must agree.
                let mut synced = raw_only.clone();
                synced.status_snapshot = Some(json!({
                    "quota": {
                        "version": 2,
                        "provider_type": "antigravity",
                        "observed_at": observed_unix,
                        "updated_at": observed_unix,
                        "exhausted": true,
                        "windows": [{
                            "code": "model:claude-sonnet-4-5",
                            "scope": "model",
                            "model": "claude-sonnet-4-5",
                            "remaining_ratio": 0.0,
                            "used_ratio": 1.0,
                            "reset_at": materialized_reset_at,
                            "is_exhausted": true
                        }]
                    }
                }));
                let synced_signals =
                    service.member_signals("antigravity", &synced, None, Some("claude-sonnet-4-5"));
                assert_eq!(
                    synced_signals.quota_exhausted, expected_exhausted,
                    "synced alias={alias}, deadline={deadline_value}"
                );
                assert_eq!(
                    synced_signals.quota_hard_blocked, expected_exhausted,
                    "synced alias={alias}, deadline={deadline_value}"
                );
                assert_eq!(
                    provider_pool_key_model_quota_hard_blocked(
                        &synced,
                        "antigravity",
                        "claude-sonnet-4-5"
                    ),
                    expected_exhausted,
                    "synced alias={alias}, deadline={deadline_value}"
                );
            }
        }

        // Invalid earlier reset aliases fall through to valid later fields without fabricating defaults.
        let fallthrough_past = sample_key(Some(json!({
            "antigravity": {
                "updated_at": observed_unix,
                "quota_by_model": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0,
                        "reset_at": "invalid-timestamp",
                        "next_reset_at": 0,
                        "reset_time": "",
                        "next_reset_time": past_rfc3339
                    }
                }
            }
        })));
        let fallthrough_past_signals = service.member_signals(
            "antigravity",
            &fallthrough_past,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(!fallthrough_past_signals.quota_exhausted);
        assert!(!fallthrough_past_signals.quota_hard_blocked);

        let fallthrough_relative = sample_key(Some(json!({
            "antigravity": {
                "updated_at": observed_unix,
                "quota_by_model": {
                    "claude-sonnet-4-5": {
                        "remaining_fraction": 0.0,
                        "used_percent": 100.0,
                        "reset_time": "not-rfc3339",
                        "next_reset_time": -10,
                        "reset_seconds": -5,
                        "reset_after_seconds": 60
                    }
                }
            }
        })));
        let fallthrough_relative_signals = service.member_signals(
            "antigravity",
            &fallthrough_relative,
            None,
            Some("claude-sonnet-4-5"),
        );
        assert!(!fallthrough_relative_signals.quota_exhausted);
        assert!(!fallthrough_relative_signals.quota_hard_blocked);

        // Malformed reset deadlines with zero remaining quota retain exhaustion.
        for invalid_reset in [
            json!("not-a-timestamp"),
            json!(""),
            json!(0),
            json!(-3600),
            json!({}),
            Value::Null,
        ] {
            let malformed_key = sample_key(Some(json!({
                "antigravity": {
                    "updated_at": now,
                    "quota_by_model": {
                        "claude-sonnet-4-5": {
                            "remaining_fraction": 0.0,
                            "used_percent": 100.0,
                            "reset_time": invalid_reset.clone(),
                            "next_reset_time": invalid_reset.clone()
                        }
                    }
                }
            })));
            let malformed_signals = service.member_signals(
                "antigravity",
                &malformed_key,
                None,
                Some("claude-sonnet-4-5"),
            );
            assert!(
                malformed_signals.quota_exhausted,
                "invalid_reset={invalid_reset}"
            );
            assert!(
                malformed_signals.quota_hard_blocked,
                "invalid_reset={invalid_reset}"
            );
        }

        // Latest valid observation transitions: 0 -> 0.5 recovers, and 0.5 -> 0 blocks.
        for (older_remaining, newer_remaining, expected_exhausted) in
            [(0.0_f64, 0.5_f64, false), (0.5_f64, 0.0_f64, true)]
        {
            let mut key = sample_key(Some(json!({
                "antigravity": {
                    "updated_at": now,
                    "quota_by_model": {
                        "claude-sonnet-4-5": {
                            "remaining_fraction": newer_remaining,
                            "used_percent": (1.0 - newer_remaining) * 100.0,
                            "reset_time": future_rfc3339.clone()
                        }
                    }
                }
            })));
            key.status_snapshot = Some(json!({
                "quota": {
                    "version": 2,
                    "provider_type": "antigravity",
                    "observed_at": now.saturating_sub(60),
                    "updated_at": now.saturating_sub(60),
                    "exhausted": older_remaining <= 0.0,
                    "windows": [{
                        "code": "model:claude-sonnet-4-5",
                        "scope": "model",
                        "model": "claude-sonnet-4-5",
                        "remaining_ratio": older_remaining,
                        "used_ratio": 1.0 - older_remaining,
                        "reset_at": future_unix,
                        "is_exhausted": older_remaining <= 0.0
                    }]
                }
            }));
            let transition_signals =
                service.member_signals("antigravity", &key, None, Some("claude-sonnet-4-5"));
            assert_eq!(
                transition_signals.quota_exhausted, expected_exhausted,
                "transition {older_remaining} -> {newer_remaining}"
            );
            assert_eq!(
                transition_signals.quota_hard_blocked, expected_exhausted,
                "transition {older_remaining} -> {newer_remaining}"
            );
            assert!(!transition_signals.runtime_quota_hard_blocked);
            assert_eq!(
                provider_pool_key_model_quota_hard_blocked(
                    &key,
                    "antigravity",
                    "claude-sonnet-4-5"
                ),
                expected_exhausted,
                "transition {older_remaining} -> {newer_remaining}"
            );
        }
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
    fn model_quota_windows_are_isolated_without_provider_specific_names() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [
                    {
                        "code": "alpha_short",
                        "quota_group": "alpha",
                        "model": "vendor-alpha-model",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    },
                    {
                        "code": "alpha_long",
                        "quota_group": "alpha",
                        "model": "vendor-alpha-model",
                        "used_ratio": 0.2,
                        "is_exhausted": false
                    },
                    {
                        "code": "beta_short",
                        "quota_group": "beta",
                        "model": "vendor-beta-model",
                        "used_ratio": 1.0,
                        "is_exhausted": true
                    }
                ]
            }
        }));

        let alpha = service.member_signals("codex", &key, None, Some("vendor-alpha-model"));
        let beta = service.member_signals("codex", &key, None, Some("vendor-beta-model"));
        assert!(
            !alpha.quota_exhausted,
            "one available alpha window must keep it usable"
        );
        assert!(beta.quota_exhausted);
        assert!(!beta.quota_hard_blocked);
    }

    #[test]
    fn legacy_family_prefix_is_matched_by_model_tokens() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [
                    { "code": "alpha_weekly", "used_ratio": 1.0, "is_exhausted": true },
                    { "code": "default_weekly", "used_ratio": 0.1, "is_exhausted": false }
                ]
            }
        }));

        let alpha = service.member_signals("codex", &key, None, Some("vendor-alpha-v2"));
        assert!(alpha.quota_exhausted);
        // An unrelated model has no identifiable bucket and therefore falls
        // back to the account-level snapshot rather than guessing a family.
        let unknown = service.member_signals("codex", &key, None, Some("vendor-gamma-v2"));
        assert!(unknown.quota_exhausted);
    }

    #[test]
    fn compact_model_bucket_identity_matches_request_tokens() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [
                    {
                        "code": "additional_0_primary_window",
                        "scope": "model",
                        "model": "spark",
                        "used_ratio": 0.0,
                        "is_exhausted": false
                    }
                ]
            }
        }));

        let signals = service.member_signals("codex", &key, None, Some("gpt-5.3-codex-spark"));
        assert!(
            !signals.quota_exhausted,
            "a compact bucket name should match a token in the selected model"
        );
    }

    #[test]
    fn model_family_token_matches_versioned_alias_without_hardcoding_name() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [{
                    "code": "additional_0_primary_window",
                    "scope": "model",
                    "model": "gpt-5.3-codex-spark",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals = service.member_signals("codex", &key, None, Some("gpt-5.4-codex-spark"));
        assert!(signals.quota_exhausted);
    }

    #[test]
    fn model_only_snapshot_does_not_poison_account_fallback() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [{
                    "code": "model:vendor-alpha",
                    "scope": "model",
                    "model": "vendor-alpha",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals = service.member_signals("codex", &key, None, None);
        assert!(
            !signals.quota_exhausted,
            "account-level inspection must ignore model-only buckets"
        );
    }

    #[test]
    fn model_map_quota_is_resolved_without_materialized_windows() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "grok",
                "exhausted": false,
                "quota_by_model": {
                    "vendor-alpha": {
                        "remaining": 0.0,
                        "total": 10.0
                    },
                    "vendor-beta": {
                        "remaining": 5.0,
                        "total": 10.0
                    }
                }
            }
        }));

        let alpha = service.member_signals("grok", &key, None, Some("vendor-alpha"));
        let beta = service.member_signals("grok", &key, None, Some("vendor-beta"));
        assert!(alpha.quota_exhausted);
        assert!(!beta.quota_exhausted);
    }

    #[test]
    fn raw_provider_metadata_model_bucket_overrides_account_snapshot() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(Some(json!({
            "codex": {
                "updated_at": 1_700_000_001u64,
                "additional_quota_windows": [{
                    "scope": "model",
                    "model": "future-spark",
                    "used_ratio": 0.0,
                    "is_exhausted": false
                }]
            }
        })));
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "exhausted": true,
                "windows": [{
                    "code": "primary",
                    "scope": "account",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals = service.member_signals("codex", &key, None, Some("future-spark"));
        assert!(
            !signals.quota_exhausted,
            "a newer model bucket in provider metadata must not inherit an exhausted account bucket"
        );
    }

    #[test]
    fn model_quota_availability_suppresses_account_hard_block() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(None);
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "allowed": false,
                "limit_reached": true,
                "exhausted": true,
                "windows": [{
                    "scope": "model",
                    "model": "future-model",
                    "used_ratio": 0.0,
                    "is_exhausted": false
                }]
            }
        }));

        let signals = service.member_signals("codex", &key, None, Some("future-model"));
        assert!(!signals.quota_exhausted);
        assert!(!signals.quota_hard_blocked);
    }

    #[test]
    fn newer_model_quota_observation_wins_over_stale_snapshot() {
        let service = ProviderPoolService::with_builtin_adapters();
        let mut key = sample_key(Some(json!({
            "codex": {
                "updated_at": 1_700_000_200u64,
                "additional_quota_windows": [{
                    "scope": "model",
                    "model": "future-model",
                    "used_ratio": 0.0,
                    "is_exhausted": false
                }]
            }
        })));
        key.status_snapshot = Some(json!({
            "quota": {
                "version": 2,
                "provider_type": "codex",
                "observed_at": 1_700_000_100u64,
                "exhausted": true,
                "windows": [{
                    "scope": "model",
                    "model": "future-model",
                    "used_ratio": 1.0,
                    "is_exhausted": true
                }]
            }
        }));

        let signals = service.member_signals("codex", &key, None, Some("future-model"));
        assert!(!signals.quota_exhausted);
    }

    #[test]
    fn codex_newer_flat_quota_metadata_clears_stale_exhausted_snapshot() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_secs();
        let service = ProviderPoolService::with_builtin_adapters();
        // Headers and quota refreshes persist the flat metadata shape while the
        // derived snapshot may still describe the previous quota observation.
        for used_percent in [83.0, 99.0, 100.0] {
            let mut key = sample_key(Some(json!({
                "codex": {
                    "updated_at": now,
                    "primary_used_percent": used_percent,
                    "primary_reset_at": now + 3600
                }
            })));
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "observed_at": now - 60,
                    "updated_at": now - 60,
                    "code": "exhausted",
                    "exhausted": true,
                    "allowed": false,
                    "limit_reached": true,
                    "reset_at": now + 3600,
                    "windows": [{
                        "code": "weekly",
                        "scope": "account",
                        "used_ratio": 1.0,
                        "remaining_ratio": 0.0,
                        "is_exhausted": true,
                        "reset_at": now + 3600
                    }]
                }
            }));
            for model in [None, Some("gpt-5.4"), Some("o3")] {
                let signals = service.member_signals("codex", &key, None, model);
                assert_eq!(signals.quota_exhausted, used_percent >= 100.0);
                assert!(!signals.quota_hard_blocked);
                assert_eq!(
                    provider_pool_key_minimum_quota_reached(&key, "codex", model),
                    used_percent >= 99.0
                );
            }
        }
    }

    #[test]
    fn codex_account_and_model_quota_agree_on_explicit_allow_and_deny_flags() {
        let service = ProviderPoolService::with_builtin_adapters();
        for (flags, exhausted) in [
            (json!({ "allowed": true }), false),
            (json!({ "limit_reached": false }), false),
            (json!({ "allowed": true, "limit_reached": true }), true),
            (json!({ "allowed": false, "limit_reached": false }), true),
        ] {
            for use_windows in [false, true] {
                let mut metadata = flags.clone();
                metadata["updated_at"] = json!(300);
                if use_windows {
                    metadata["windows"] = json!([{ "code": "weekly", "used_ratio": 1.0 }]);
                } else {
                    metadata["primary_used_percent"] = json!(100.0);
                }
                let key = sample_key(Some(json!({ "codex": metadata })));
                for model in [None, Some("gpt-5.4"), Some("o3")] {
                    assert_eq!(
                        service
                            .member_signals("codex", &key, None, model)
                            .quota_exhausted,
                        exhausted,
                        "flags={flags}, use_windows={use_windows}, model={model:?}"
                    );
                    // The opt-in reserve still protects a numerically full
                    // window even when the upstream reports it as allowed.
                    assert!(provider_pool_key_minimum_quota_reached(
                        &key, "codex", model
                    ));
                }
            }
        }
    }

    #[test]
    fn codex_model_quota_honors_latest_account_refusal_without_blocking_spark() {
        let service = ProviderPoolService::with_builtin_adapters();
        for include_window in [false, true] {
            let mut metadata = json!({
                "updated_at": 300,
                "allowed": false,
                "limit_reached": true,
                "spark_primary_used_percent": 17.0
            });
            if include_window {
                metadata["primary_used_percent"] = json!(83.0);
            }
            let mut key = sample_key(Some(json!({ "codex": metadata })));
            for include_snapshot in [false, true] {
                if include_snapshot {
                    key.status_snapshot = Some(json!({
                        "quota": {
                            "provider_type": "codex",
                            "observed_at": 200,
                            "exhausted": false,
                            "windows": [{ "code": "weekly", "used_ratio": 0.83 }]
                        }
                    }));
                }
                assert!(
                    service
                        .member_signals("codex", &key, None, Some("gpt-5.4"))
                        .quota_exhausted
                );
                assert!(
                    !service
                        .member_signals("codex", &key, None, Some("gpt-5.3-codex-spark"))
                        .quota_exhausted
                );
            }
        }
    }

    #[test]
    fn codex_quota_source_freshness_supports_all_timestamp_formats() {
        let service = ProviderPoolService::with_builtin_adapters();
        for observed_at in [
            json!(1_700_000_200_u64),
            json!(1_700_000_200_000_u64),
            json!("1700000200000"),
            json!("2023-11-14T22:16:40Z"),
        ] {
            for use_windows in [false, true] {
                let metadata = if use_windows {
                    json!({
                        "updated_at": observed_at,
                        "windows": [{ "code": "weekly", "used_ratio": 0.83 }]
                    })
                } else {
                    json!({ "updated_at": observed_at, "primary_used_percent": 83.0 })
                };
                let mut key = sample_key(Some(json!({ "codex": metadata })));
                key.status_snapshot = Some(json!({
                    "quota": {
                        "provider_type": "codex",
                        "observed_at": "2023-11-14T22:15:00Z",
                        "exhausted": true,
                        "allowed": false,
                        "windows": [{ "code": "weekly", "used_ratio": 1.0 }]
                    }
                }));
                for model in [None, Some("gpt-5.4")] {
                    let signals = service.member_signals("codex", &key, None, model);
                    assert!(!signals.quota_exhausted, "timestamp={observed_at}");
                    assert!(!signals.quota_hard_blocked, "timestamp={observed_at}");
                    assert!(!provider_pool_key_minimum_quota_reached(
                        &key, "codex", model
                    ));
                }

                // Swap freshness while retaining the same observations. An
                // older usable bucket cannot erase a later exhausted snapshot.
                key.status_snapshot.as_mut().unwrap()["quota"]["observed_at"] =
                    json!("2023-11-14T22:18:20Z");
                assert!(provider_pool_key_account_quota_exhausted(&key, "codex"));
                assert!(provider_pool_key_quota_hard_blocked(&key, "codex"));
                assert_eq!(
                    provider_pool_key_model_quota_exhausted(&key, "codex", "gpt-5.4"),
                    Some(true)
                );
            }
        }
    }

    #[test]
    fn codex_account_quota_recovery_requires_new_account_observation() {
        for metadata in [
            json!({ "updated_at": 100, "primary_used_percent": 83.0 }),
            json!({ "primary_used_percent": 83.0 }),
            json!({ "updated_at": 300, "plan_type": "plus" }),
            json!({ "updated_at": 300, "credits_unlimited": false }),
            json!({ "updated_at": 300, "spark_primary_used_percent": 83.0 }),
            json!({ "updated_at": 300, "windows": [{ "code": "weekly" }] }),
        ] {
            let mut key = sample_key(Some(json!({ "codex": metadata })));
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "observed_at": 200,
                    "exhausted": true,
                    "allowed": false,
                    "limit_reached": true,
                    "windows": [{ "code": "weekly", "used_ratio": 1.0 }]
                }
            }));
            assert!(provider_pool_key_account_quota_exhausted(&key, "codex"));
            assert!(provider_pool_key_quota_hard_blocked(&key, "codex"));
            assert_eq!(
                provider_pool_key_model_quota_exhausted(&key, "codex", "gpt-5.4"),
                Some(true)
            );
        }

        let mut key = sample_key(Some(json!({
            "codex": {
                "updated_at": 300,
                "primary_used_percent": 83.0,
                "allowed": false,
                "limit_reached": true
            }
        })));
        key.status_snapshot = Some(json!({
            "quota": { "provider_type": "codex", "updated_at": 200, "exhausted": false }
        }));
        assert!(provider_pool_key_account_quota_exhausted(&key, "codex"));
        assert!(provider_pool_key_quota_hard_blocked(&key, "codex"));
    }

    #[test]
    fn codex_account_updates_do_not_override_independent_model_quota() {
        for (spark_ratio, account_percent) in [(0.83, 100.0), (1.0, 83.0)] {
            let mut key = sample_key(Some(json!({
                "codex": { "updated_at": 300, "primary_used_percent": account_percent }
            })));
            key.status_snapshot = Some(json!({
                "quota": {
                    "provider_type": "codex",
                    "updated_at": 200,
                    "windows": [{ "code": "spark_5h", "used_ratio": spark_ratio }]
                }
            }));
            assert_eq!(
                provider_pool_key_model_quota_exhausted(&key, "codex", "gpt-5.3-codex-spark"),
                Some(spark_ratio >= 1.0)
            );
        }
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
