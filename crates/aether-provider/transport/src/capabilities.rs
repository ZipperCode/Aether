use serde_json::{Map, Value};

/// 仅成功远端抓取可建立账号模型能力；配置白名单与执行卡片不属于发现证据。
pub const CODEX_MODEL_DISCOVERY_NAMESPACE: &str = "codex_model_discovery";

/// 保留上游原始套餐区别，仅统一大小写和既有 Provider 前缀。
pub fn normalize_provider_plan_tier(value: &str, provider_type: &str) -> Option<String> {
    let mut normalized = value.trim().to_string();
    if normalized.is_empty() {
        return None;
    }

    let provider_type = provider_type.trim().to_ascii_lowercase();
    if !provider_type.is_empty() && normalized.to_ascii_lowercase().starts_with(&provider_type) {
        normalized = normalized[provider_type.len()..]
            .trim_matches(|ch: char| [' ', ':', '-', '_'].contains(&ch))
            .to_string();
    }

    let normalized = normalized.trim().to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

/// 发现、传输和号池复用同一组已有套餐字段，不按展示名称推断权限。
pub fn provider_plan_tier_from_fields(
    source: &Map<String, Value>,
    provider_type: &str,
) -> Option<String> {
    [
        "plan_type",
        "tier",
        "plan",
        "subscription_title",
        "subscription_plan",
    ]
    .iter()
    .filter_map(|field| source.get(*field).and_then(Value::as_str))
    .find_map(|value| normalize_provider_plan_tier(value, provider_type))
}

/// 上游刷新的套餐优先于初次 OAuth 登录声明；不读取认证以外的运行态。
pub fn provider_plan_tier_from_metadata(
    provider_type: &str,
    upstream_metadata: Option<&Value>,
    auth_config: Option<&Map<String, Value>>,
) -> Option<String> {
    let metadata = upstream_metadata.and_then(Value::as_object);
    metadata
        .and_then(|metadata| metadata.get(&provider_type.trim().to_ascii_lowercase()))
        .and_then(Value::as_object)
        .into_iter()
        .chain(metadata)
        .chain(auth_config)
        .find_map(|source| provider_plan_tier_from_fields(source, provider_type))
}

/// 只拒绝官方明确不支持生图的 Free；未知套餐与型号权限仍由原有准入和上游决定。
pub fn codex_oauth_capability_skip_reason(
    provider_type: &str,
    auth_type: &str,
    api_format: &str,
    upstream_metadata: Option<&Value>,
    auth_config: Option<&Value>,
) -> Option<&'static str> {
    if !provider_type.trim().eq_ignore_ascii_case("codex")
        || !auth_type.trim().eq_ignore_ascii_case("oauth")
        || !aether_ai_formats::api_format_alias_matches(api_format, "openai:image")
    {
        return None;
    }

    (provider_plan_tier_from_metadata(
        provider_type,
        upstream_metadata,
        auth_config.and_then(Value::as_object),
    )
    .as_deref()
        == Some("free"))
    .then_some("codex_plan_image_generation_unsupported")
}

/// 只在 Codex OAuth 图片请求解析已有认证声明；普通候选不增加 JSON 解析成本。
pub fn codex_oauth_transport_capability_skip_reason(
    transport: &crate::GatewayProviderTransportSnapshot,
) -> Option<&'static str> {
    if !transport
        .provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("codex")
        || !transport.key.auth_type.trim().eq_ignore_ascii_case("oauth")
        || !aether_ai_formats::api_format_alias_matches(
            &transport.endpoint.api_format,
            "openai:image",
        )
    {
        return None;
    }
    let auth_config = transport
        .key
        .decrypted_auth_config
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok());
    codex_oauth_capability_skip_reason(
        &transport.provider.provider_type,
        &transport.key.auth_type,
        &transport.endpoint.api_format,
        transport.key.upstream_metadata.as_ref(),
        auth_config.as_ref(),
    )
}

/// 校验 Codex OAuth 文本候选的最终上游模型是否属于该账号最近一次成功抓取的真实远端目录。
pub fn codex_oauth_discovered_model_skip_reason(
    provider_type: &str,
    auth_type: &str,
    api_format: &str,
    selected_provider_model_name: &str,
    upstream_metadata: Option<&Value>,
) -> Option<&'static str> {
    if !provider_type.trim().eq_ignore_ascii_case("codex")
        || !auth_type.trim().eq_ignore_ascii_case("oauth")
        || !(aether_ai_formats::api_format_alias_matches(api_format, "openai:responses")
            || aether_ai_formats::api_format_alias_matches(api_format, "openai:responses:compact")
            || aether_ai_formats::api_format_alias_matches(api_format, "openai:search"))
    {
        return None;
    }

    let selected_model = selected_provider_model_name.trim();
    if selected_model.is_empty() {
        return Some("key_model_disabled");
    }

    let Some(model_ids) = upstream_metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(CODEX_MODEL_DISCOVERY_NAMESPACE))
        .and_then(Value::as_object)
        .and_then(|discovery| discovery.get("model_ids"))
        .and_then(Value::as_array)
    else {
        return Some("key_model_disabled");
    };

    if model_ids.is_empty() {
        return Some("key_model_disabled");
    }

    // 命中后仍检查剩余条目，避免混入畸形身份的目录被部分放行。
    let mut matched = false;
    for entry in model_ids {
        let Some(model_id) = entry.as_str() else {
            return Some("key_model_disabled");
        };
        let trimmed = model_id.trim();
        if trimmed.is_empty() || trimmed.len() != model_id.len() {
            return Some("key_model_disabled");
        }
        if model_id == selected_model {
            matched = true;
        }
    }

    if matched {
        None
    } else {
        Some("key_model_disabled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_image_capability_only_rejects_known_free_oauth() {
        for plan in [
            "Free",
            "codex: FREE",
            "plus",
            "pro",
            "prolite",
            "promax",
            "future",
            "",
        ] {
            let metadata = json!({"codex": {"plan_type": plan}});
            let expected = matches!(plan, "Free" | "codex: FREE")
                .then_some("codex_plan_image_generation_unsupported");
            assert_eq!(
                codex_oauth_capability_skip_reason(
                    "codex",
                    "oauth",
                    "openai:image",
                    Some(&metadata),
                    None
                ),
                expected,
                "{plan}"
            );
            for (provider, auth, format) in [
                ("openai", "oauth", "openai:image"),
                ("codex", "api_key", "openai:image"),
                ("codex", "oauth", "openai:search"),
                ("codex", "oauth", "openai:responses:compact"),
                ("codex", "oauth", "codex:live"),
            ] {
                assert_eq!(
                    codex_oauth_capability_skip_reason(
                        provider,
                        auth,
                        format,
                        Some(&metadata),
                        None
                    ),
                    None
                );
            }
        }
        assert_eq!(
            codex_oauth_capability_skip_reason("codex", "oauth", "openai:image", None, None),
            None
        );
    }

    #[test]
    fn codex_plan_sources_preserve_precedence_aliases_and_distinct_raw_tiers() {
        let auth = json!({"plan_type": "free"});
        assert_eq!(
            codex_oauth_capability_skip_reason("codex", "oauth", "openai:image", None, Some(&auth)),
            Some("codex_plan_image_generation_unsupported")
        );
        for field in [
            "plan_type",
            "tier",
            "plan",
            "subscription_title",
            "subscription_plan",
        ] {
            let metadata = json!({"codex": {field: " ProLite "}, "plan_type": "free"});
            assert_eq!(
                provider_plan_tier_from_metadata("codex", Some(&metadata), auth.as_object())
                    .as_deref(),
                Some("prolite")
            );
            assert_eq!(
                codex_oauth_capability_skip_reason(
                    "codex",
                    "oauth",
                    "openai:image",
                    Some(&metadata),
                    Some(&auth)
                ),
                None
            );
        }
        let metadata = json!({"plan_type": "codex_free"});
        assert_eq!(
            codex_oauth_capability_skip_reason(
                "codex",
                "oauth",
                "openai:image",
                Some(&metadata),
                None
            ),
            Some("codex_plan_image_generation_unsupported")
        );
    }

    #[test]
    fn codex_tool_discovery_capability_does_not_gate_alpha_search() {
        let metadata =
            json!({"codex_models": {"cards": {"future-model": {"supports_search_tool": false}}}});
        assert_eq!(
            codex_oauth_capability_skip_reason(
                "codex",
                "oauth",
                "openai:search",
                Some(&metadata),
                None
            ),
            None
        );
    }

    #[test]
    fn codex_model_discovery_helper_rejects_missing_malformed_empty_and_tainted_arrays() {
        assert_eq!(
            codex_oauth_discovered_model_skip_reason(
                "codex",
                "oauth",
                "openai:responses",
                "gpt-6.1-sol",
                None,
            ),
            Some("key_model_disabled")
        );

        for invalid_metadata in [
            json!(null),
            json!("bad"),
            json!([]),
            json!({}),
            json!({
                "codex_models": {"cards": {"gpt-6.1-sol": {}}},
                "cached_models": ["gpt-6.1-sol"]
            }),
            json!({"codex_model_discovery": null}),
            json!({"codex_model_discovery": true}),
            json!({"codex_model_discovery": "gpt-6.1-sol"}),
            json!({"codex_model_discovery": ["gpt-6.1-sol"]}),
            json!({"codex_model_discovery": {}}),
            json!({"codex_model_discovery": {"model_ids": null}}),
            json!({"codex_model_discovery": {"model_ids": "gpt-6.1-sol"}}),
            json!({"codex_model_discovery": {"model_ids": {"gpt-6.1-sol": true}}}),
            json!({"codex_model_discovery": {"model_ids": []}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", 123]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", null]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", false]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", {}]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", []]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", ""]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", "   \t\n "]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", " gpt-basic-test"]}}),
            json!({"codex_model_discovery": {"model_ids": ["gpt-6.1-sol", "gpt-basic-test\t"]}}),
            json!({"codex_model_discovery": {"model_ids": [" gpt-6.1-sol "]}}),
        ] {
            assert_eq!(
                codex_oauth_discovered_model_skip_reason(
                    "codex",
                    "oauth",
                    "openai:responses",
                    "gpt-6.1-sol",
                    Some(&invalid_metadata),
                ),
                Some("key_model_disabled"),
                "invalid metadata should be rejected: {invalid_metadata}"
            );
        }

        let valid_metadata = json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-6.1-sol"]
            }
        });
        for blank_model in ["", "   ", "\t\n"] {
            assert_eq!(
                codex_oauth_discovered_model_skip_reason(
                    "codex",
                    "oauth",
                    "openai:responses",
                    blank_model,
                    Some(&valid_metadata),
                ),
                Some("key_model_disabled")
            );
        }
    }

    #[test]
    fn codex_model_discovery_helper_enforces_exact_case_sensitive_text_models_and_bypasses() {
        let metadata = json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-basic-test", "gpt-6.1-sol", "GPT-6.1-Pro"]
            }
        });

        for format in [
            "openai:responses",
            "/v1/responses",
            " OPENAI:RESPONSES ",
            "openai:responses:compact",
            "/v1/responses/compact",
            "openai:search",
            "openai_search",
            "search",
            "/v1/alpha/search",
        ] {
            for model in [
                "gpt-basic-test",
                "gpt-6.1-sol",
                "  gpt-6.1-sol  ",
                "GPT-6.1-Pro",
            ] {
                assert_eq!(
                    codex_oauth_discovered_model_skip_reason(
                        " Codex ",
                        " OAuth ",
                        format,
                        model,
                        Some(&metadata),
                    ),
                    None,
                    "expected exact hit for format={format} model={model}"
                );
            }

            for rejected_model in [
                "GPT-6.1-SOL",
                "gpt-6.1-pro",
                "gpt-6.1",
                "gpt-*",
                "codex-pro-alias",
            ] {
                assert_eq!(
                    codex_oauth_discovered_model_skip_reason(
                        "codex",
                        "oauth",
                        format,
                        rejected_model,
                        Some(&metadata),
                    ),
                    Some("key_model_disabled"),
                    "expected rejection for format={format} model={rejected_model}"
                );
            }
        }

        let wildcard_metadata = json!({
            "codex_model_discovery": {
                "model_ids": ["gpt-*"]
            }
        });
        assert_eq!(
            codex_oauth_discovered_model_skip_reason(
                "codex",
                "oauth",
                "openai:responses",
                "gpt-6.1-sol",
                Some(&wildcard_metadata),
            ),
            Some("key_model_disabled")
        );

        for (provider, auth, format) in [
            ("openai", "oauth", "openai:responses"),
            ("antigravity", "oauth", "openai:responses"),
            ("custom", "oauth", "openai:responses"),
            ("codex", "bearer", "openai:responses"),
            ("codex", "api_key", "openai:responses"),
            ("codex", "", "openai:responses"),
            ("codex", "oauth", "openai:image"),
            ("codex", "oauth", "codex:live"),
            ("codex", "oauth", "codex_live"),
            ("codex", "oauth", "live"),
            ("codex", "oauth", "/v1/live"),
            ("codex", "oauth", "openai:realtime"),
            ("codex", "oauth", "realtime"),
            ("codex", "oauth", "/v1/realtime"),
            ("codex", "oauth", "openai:chat"),
            ("codex", "oauth", "/v1/chat/completions"),
            ("codex", "oauth", "openai:embedding"),
            ("codex", "oauth", "openai:compact"),
        ] {
            assert_eq!(
                codex_oauth_discovered_model_skip_reason(provider, auth, format, "", None),
                None,
                "expected bypass for provider={provider} auth={auth} format={format}"
            );
        }
    }
}
