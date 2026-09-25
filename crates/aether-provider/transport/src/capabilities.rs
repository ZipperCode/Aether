use serde_json::{Map, Value};

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
}
