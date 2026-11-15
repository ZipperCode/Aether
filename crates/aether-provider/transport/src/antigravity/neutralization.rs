use std::collections::BTreeSet;

use serde_json::Value;

#[path = "opening_identity.rs"]
mod opening_identity;

use opening_identity::sanitize_system_text;

const MAX_IDENTITY_NEUTRALIZATION_RULES: usize = 32;
const MAX_IDENTITY_NEUTRALIZATION_TEXT_BYTES: usize = 16_384;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NeutralizationMode {
    Exact,
    OpeningIdentity,
}

/// 仅在跨格式 Antigravity 请求中处理系统指令文本 part。
/// exact 保持完整 part 显式规则替换；opening_identity 只归一化明确的开场身份。
/// 同格式请求保持透传；未启用、模式无效或对应配置无效时均不修改请求。
pub fn apply_antigravity_identity_neutralization(
    envelope: &mut Value,
    provider_config: Option<&Value>,
    same_format: bool,
) {
    if same_format {
        return;
    }
    let Some(antigravity) = provider_config
        .and_then(|config| config.get("pool_advanced"))
        .and_then(|pool_advanced| pool_advanced.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return;
    };
    if !antigravity
        .get("identity_neutralization_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return;
    }
    let mode = match antigravity.get("identity_neutralization_mode") {
        None => NeutralizationMode::Exact,
        Some(Value::String(mode)) if mode == "exact" => NeutralizationMode::Exact,
        Some(Value::String(mode)) if mode == "opening_identity" => {
            NeutralizationMode::OpeningIdentity
        }
        _ => return,
    };

    if mode == NeutralizationMode::OpeningIdentity {
        // 新模式不读取、不校验也不串联 exact 规则；每个 part 独立处理。
        let Some(parts) = envelope
            .get_mut("request")
            .and_then(Value::as_object_mut)
            .and_then(|request| request.get_mut("systemInstruction"))
            .and_then(Value::as_object_mut)
            .and_then(|system_instruction| system_instruction.get_mut("parts"))
            .and_then(Value::as_array_mut)
        else {
            return;
        };
        for part in parts {
            let Some(part) = part.as_object_mut() else {
                continue;
            };
            // 与 Gemini 系统 part 语义一致：思维、签名及显式非文本 part 不属于处理范围。
            if part.get("thought").and_then(Value::as_bool) == Some(true)
                || part.contains_key("thoughtSignature")
                || part.contains_key("thought_signature")
                || part
                    .get("type")
                    .is_some_and(|kind| kind.as_str() != Some("text"))
            {
                continue;
            }
            let Some(text) = part.get("text").and_then(Value::as_str) else {
                continue;
            };
            let sanitized = sanitize_system_text(text);
            if sanitized != text {
                part.insert("text".to_string(), Value::String(sanitized));
            }
        }
        return;
    }

    let Some(rules) = antigravity
        .get("identity_neutralization_rules")
        .and_then(Value::as_array)
        .filter(|rules| rules.len() <= MAX_IDENTITY_NEUTRALIZATION_RULES)
    else {
        return;
    };
    // 原始 Provider 配置可能绕过管理端校验；任一规则无效或重复时整体保持无操作。
    let mut matches = BTreeSet::new();
    let mut parsed_rules = Vec::with_capacity(rules.len());
    for rule in rules {
        let Some(rule) = rule.as_object() else {
            return;
        };
        let Some(source) = rule.get("match").and_then(Value::as_str).filter(|text| {
            !text.is_empty() && text.len() <= MAX_IDENTITY_NEUTRALIZATION_TEXT_BYTES
        }) else {
            return;
        };
        let Some(replacement) = rule
            .get("replacement")
            .and_then(Value::as_str)
            .filter(|text| text.len() <= MAX_IDENTITY_NEUTRALIZATION_TEXT_BYTES)
        else {
            return;
        };
        if !matches.insert(source) {
            return;
        }
        parsed_rules.push((source, replacement));
    }
    let Some(parts) = envelope
        .get_mut("request")
        .and_then(Value::as_object_mut)
        .and_then(|request| request.get_mut("systemInstruction"))
        .and_then(Value::as_object_mut)
        .and_then(|system_instruction| system_instruction.get_mut("parts"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for part in parts {
        let Some(part) = part.as_object_mut() else {
            continue;
        };
        let Some(text) = part.get("text").and_then(Value::as_str) else {
            continue;
        };
        // 仅比较整个字符串，不做 trim、子串或品牌词替换；每个 part 至多应用一条规则。
        let replacement = parsed_rules
            .iter()
            .find_map(|(source, replacement)| (*source == text).then_some(*replacement));
        if let Some(replacement) = replacement {
            part.insert("text".to_string(), Value::String(replacement.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        apply_antigravity_identity_neutralization, opening_identity::OFFICIAL_ANTIGRAVITY_IDENTITY,
    };

    fn envelope(system_text: &str) -> Value {
        json!({
            "request": {
                "systemInstruction": {
                    "parts": [{"text": system_text}]
                },
                "contents": []
            }
        })
    }

    fn config(enabled: Option<bool>) -> Value {
        let mut antigravity = json!({
            "identity_neutralization_rules": [
                {"match": "exact system prompt", "replacement": "neutral system prompt"}
            ]
        });
        if let Some(enabled) = enabled {
            antigravity["identity_neutralization_enabled"] = Value::Bool(enabled);
        }
        json!({"pool_advanced": {"antigravity": antigravity}})
    }

    fn opening_config() -> Value {
        json!({
            "pool_advanced": {"antigravity": {
                "identity_neutralization_enabled": true,
                "identity_neutralization_mode": "opening_identity"
            }}
        })
    }

    fn opening_text(body: &Value) -> &str {
        body["request"]["systemInstruction"]["parts"][0]["text"]
            .as_str()
            .expect("首个系统 part 必须包含文本")
    }

    #[test]
    fn missing_default_or_disabled_config_preserves_system_instruction() {
        for provider_config in [None, Some(config(None)), Some(config(Some(false)))] {
            let mut body = envelope("exact system prompt");
            let original = body.clone();
            apply_antigravity_identity_neutralization(&mut body, provider_config.as_ref(), false);
            assert_eq!(body, original);
        }
    }

    #[test]
    fn replaces_only_complete_exact_text_part_matches() {
        let mut body = json!({
            "request": {
                "systemInstruction": {
                    "parts": [
                        {"text": "exact system prompt"},
                        {"text": " exact system prompt"},
                        {"text": "exact system prompt "},
                        {"text": "prefix exact system prompt suffix"},
                        {"text": 42},
                        {"image": {"data": "unchanged"}}
                    ]
                },
                "contents": []
            }
        });

        apply_antigravity_identity_neutralization(&mut body, Some(&config(Some(true))), false);

        let parts = body["request"]["systemInstruction"]["parts"]
            .as_array()
            .expect("parts should remain an array");
        assert_eq!(parts[0]["text"], "neutral system prompt");
        assert_eq!(parts[1]["text"], " exact system prompt");
        assert_eq!(parts[2]["text"], "exact system prompt ");
        assert_eq!(parts[3]["text"], "prefix exact system prompt suffix");
        assert_eq!(parts[4]["text"], 42);
        assert_eq!(parts[5], json!({"image": {"data": "unchanged"}}));
    }

    #[test]
    fn preserves_unknown_fields_and_user_and_tool_content() {
        let mut body = json!({
            "requestId": "keep-request-id",
            "unknownEnvelopeField": {"keep": true},
            "request": {
                "systemInstruction": {
                    "role": "system",
                    "unknownSystemField": [1, 2, 3],
                    "parts": [{
                        "text": "exact system prompt",
                        "cache_control": {"type": "ephemeral"}
                    }]
                },
                "contents": [
                    {"role": "user", "parts": [{"text": "exact system prompt"}]},
                    {"role": "tool", "parts": [{"text": "exact system prompt"}]}
                ],
                "tools": [{"functionDeclarations": [{"description": "exact system prompt"}]}],
                "unknownRequestField": {"keep": true}
            }
        });
        let mut expected = body.clone();
        expected["request"]["systemInstruction"]["parts"][0]["text"] =
            Value::String("neutral system prompt".to_string());

        apply_antigravity_identity_neutralization(&mut body, Some(&config(Some(true))), false);

        assert_eq!(body, expected);
    }

    #[test]
    fn malformed_raw_rules_fail_closed_without_partial_mutation() {
        let valid = json!({
            "match": "exact system prompt",
            "replacement": "neutral system prompt"
        });
        let mut too_many = vec![valid.clone()];
        too_many.extend(
            (0..32).map(|index| json!({"match": format!("other-{index}"), "replacement": "other"})),
        );
        let invalid_rule_sets = vec![
            vec![valid.clone(), valid.clone()],
            vec![valid.clone(), json!({"match": "", "replacement": "other"})],
            vec![valid.clone(), json!({"match": "other"})],
            vec![
                valid.clone(),
                json!({"match": "x".repeat(16_385), "replacement": "other"}),
            ],
            vec![
                valid,
                json!({"match": "other", "replacement": "x".repeat(16_385)}),
            ],
            too_many,
        ];

        for rules in invalid_rule_sets {
            let provider_config = json!({
                "pool_advanced": {"antigravity": {
                    "identity_neutralization_enabled": true,
                    "identity_neutralization_rules": Value::Array(rules)
                }}
            });
            let mut body = envelope("exact system prompt");
            let original = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&provider_config), false);
            assert_eq!(body, original);
        }
    }

    #[test]
    fn same_format_native_gate_preserves_exact_match() {
        let provider_config = config(Some(true));
        let mut native = envelope("exact system prompt");
        let original = native.clone();
        apply_antigravity_identity_neutralization(&mut native, Some(&provider_config), true);
        assert_eq!(native, original);

        let mut cross_format = envelope("exact system prompt");
        apply_antigravity_identity_neutralization(&mut cross_format, Some(&provider_config), false);
        assert_eq!(
            cross_format["request"]["systemInstruction"]["parts"][0]["text"],
            "neutral system prompt"
        );
    }

    #[test]
    fn opening_identity_transforms_supported_clients_and_retains_following_text() {
        let cases = [
            (
                "You are DSH, an automated software development harness designed for agentic coding workflows.",
                OFFICIAL_ANTIGRAVITY_IDENTITY.to_string(),
            ),
            (
                "You are opencode, an interactive CLI tool that helps users with software engineering tasks. Use the available tools.",
                format!("{OFFICIAL_ANTIGRAVITY_IDENTITY} Use the available tools."),
            ),
            (
                "You are Codex, a coding agent based on GPT-5. You and the user share one workspace.",
                format!("{OFFICIAL_ANTIGRAVITY_IDENTITY} You and the user share one workspace."),
            ),
            (
                "You are a Claude agent, built on Anthropic's Claude Agent SDK.",
                OFFICIAL_ANTIGRAVITY_IDENTITY.to_string(),
            ),
            (
                "You are Hermes Agent, an intelligent AI assistant created by Nous Research. Be direct.",
                format!("{OFFICIAL_ANTIGRAVITY_IDENTITY} Be direct."),
            ),
        ];

        for (input, expected) in cases {
            let mut body = envelope(input);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);
            assert_eq!(opening_text(&body), expected, "输入：{input}");
        }
    }

    #[test]
    fn opening_identity_handles_multiple_identities_without_rewriting_constraints() {
        let stem = OFFICIAL_ANTIGRAVITY_IDENTITY
            .strip_suffix('.')
            .expect("官方身份必须有句号");
        let mut body = envelope(
            "You are an AI agent powered by DeepSeek Harness.\nYou are an assistant created by Acme, keep /tmp/Aether, tool_call and x-anthropic-billing-header: required.",
        );

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);

        assert_eq!(
            opening_text(&body),
            format!(
                "{OFFICIAL_ANTIGRAVITY_IDENTITY}\n{stem}, keep /tmp/Aether, tool_call and x-anthropic-billing-header: required."
            )
        );
    }

    #[test]
    fn opening_identity_preserves_fenced_inline_and_indented_code() {
        let input = "You are DSH.\n```text\nYou are Codex.\nx-anthropic-billing-header: secret\n```\n\n`You are DSH.` and ``You are Codex.``\n\n    You are DSH.\n\tx-anthropic-billing-header: code";
        let expected = input.replacen("You are DSH.", OFFICIAL_ANTIGRAVITY_IDENTITY, 1);
        let mut body = envelope(input);

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);

        assert_eq!(opening_text(&body), expected);
    }

    #[test]
    fn opening_identity_removes_only_standalone_billing_lines() {
        let stem = OFFICIAL_ANTIGRAVITY_IDENTITY
            .strip_suffix('.')
            .expect("官方身份必须有句号");
        let mut body = envelope(
            "   x-anthropic-billing-header: cc_version=2; cch=123;\nYou are an assistant created by Acme, retain x-anthropic-billing-header: as a tool rule.\nMetadata x-anthropic-billing-header: keep this path /tmp/a.",
        );

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);

        assert_eq!(
            opening_text(&body),
            format!(
                "{stem}, retain x-anthropic-billing-header: as a tool rule.\nMetadata x-anthropic-billing-header: keep this path /tmp/a."
            )
        );
    }

    #[test]
    fn opening_identity_preserves_uncertain_and_non_identity_text() {
        let cases = [
            "You are DSH?",
            "Are you an AI assistant created by OpenAI?",
            "You are Clineage.",
            "You are an assistant created by Acme and Bob.",
            "You are an assistant created by acme.com.",
            "You are a coding agent running in read-only mode.",
            "You are an assistant designed for secure code review.",
            "You are an AI agent created by `Acme`. Follow rules.",
            "Use OpenAI at https://api.openai.com/v1 and retain Anthropic tool documentation.",
            "Please tell the user that You are DSH.",
        ];

        for input in cases {
            let mut body = envelope(input);
            let original = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);
            assert_eq!(body, original, "输入不应变化：{input}");
        }
    }

    #[test]
    fn opening_identity_collapses_only_leading_pure_paragraphs_and_is_idempotent() {
        let mut body =
            envelope("You are DSH.\n\nYou are Codex.\n\nKeep tool rules and /tmp/Aether.");
        let provider_config = opening_config();
        apply_antigravity_identity_neutralization(&mut body, Some(&provider_config), false);
        let once = body.clone();
        assert_eq!(
            opening_text(&once),
            format!("{OFFICIAL_ANTIGRAVITY_IDENTITY}\n\nKeep tool rules and /tmp/Aether.")
        );

        apply_antigravity_identity_neutralization(&mut body, Some(&provider_config), false);
        assert_eq!(body, once);
    }

    #[test]
    fn opening_identity_does_not_collapse_across_system_parts() {
        let mut body = json!({
            "request": {
                "systemInstruction": {
                    "parts": [{"text": "You are DSH."}, {"text": "You are Codex."}]
                },
                "contents": []
            }
        });

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);

        let parts = body["request"]["systemInstruction"]["parts"]
            .as_array()
            .expect("parts 应保持数组");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], OFFICIAL_ANTIGRAVITY_IDENTITY);
        assert_eq!(parts[1]["text"], OFFICIAL_ANTIGRAVITY_IDENTITY);
    }

    #[test]
    fn opening_identity_preserves_unknown_fields_user_tools_and_protected_parts() {
        let mut body = json!({
            "requestId": "keep-request-id",
            "unknownEnvelopeField": {"keep": true},
            "request": {
                "systemInstruction": {
                    "role": "system",
                    "unknownSystemField": [1, 2, 3],
                    "parts": [
                        {
                            "text": "You are DSH.",
                            "cache_control": {"type": "ephemeral"},
                            "unknownPartField": {"keep": true}
                        },
                        {"text": "You are DSH.", "thought": true},
                        {"text": "You are DSH.", "thoughtSignature": "keep"},
                        {"text": "You are DSH.", "type": "image"},
                        {"text": 42},
                        {"image": {"data": "unchanged"}}
                    ]
                },
                "contents": [
                    {"role": "user", "parts": [{"text": "You are DSH.\nx-anthropic-billing-header: user"}]},
                    {"role": "tool", "parts": [{"text": "You are Codex."}]}
                ],
                "tools": [{"functionDeclarations": [{"description": "You are DSH."}]}],
                "unknownRequestField": {"keep": true}
            }
        });
        let mut expected = body.clone();
        expected["request"]["systemInstruction"]["parts"][0]["text"] =
            Value::String(OFFICIAL_ANTIGRAVITY_IDENTITY.to_string());

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()), false);

        assert_eq!(body, expected);
    }

    #[test]
    fn opening_identity_respects_native_gate_invalid_modes_and_rule_independence() {
        let provider_config = opening_config();
        let mut native = envelope("You are DSH.");
        let original = native.clone();
        apply_antigravity_identity_neutralization(&mut native, Some(&provider_config), true);
        assert_eq!(native, original);

        let mut disabled_config = provider_config.clone();
        disabled_config["pool_advanced"]["antigravity"]["identity_neutralization_enabled"] =
            Value::Bool(false);
        let mut disabled = envelope("You are DSH.");
        apply_antigravity_identity_neutralization(&mut disabled, Some(&disabled_config), false);
        assert_eq!(disabled, original);

        let mut without_rules = provider_config;
        without_rules["pool_advanced"]["antigravity"]["identity_neutralization_rules"] =
            Value::String("opening mode must ignore exact rules".to_string());
        let mut transformed = envelope("You are DSH.");
        apply_antigravity_identity_neutralization(&mut transformed, Some(&without_rules), false);
        assert_eq!(opening_text(&transformed), OFFICIAL_ANTIGRAVITY_IDENTITY);

        for invalid_mode in [Value::Null, json!(1), json!("opening"), json!("EXACT")] {
            let mut invalid_config = config(Some(true));
            invalid_config["pool_advanced"]["antigravity"]["identity_neutralization_mode"] =
                invalid_mode;
            let mut body = envelope("exact system prompt");
            let original = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&invalid_config), false);
            assert_eq!(body, original);
        }
    }
}
