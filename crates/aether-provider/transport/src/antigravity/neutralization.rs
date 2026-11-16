use std::collections::BTreeSet;

use serde_json::Value;

#[path = "opening_identity.rs"]
mod opening_identity;

use opening_identity::sanitize_system_text;

#[cfg(test)]
#[path = "neutralization_literal_tests.rs"]
mod literal_tests;

const MAX_IDENTITY_NEUTRALIZATION_RULES: usize = 32;
const MAX_IDENTITY_NEUTRALIZATION_TEXT_BYTES: usize = 16_384;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NeutralizationMode {
    Exact,
    Literal,
    OpeningIdentity,
}

/// 按提供商显式配置处理 Antigravity 系统指令文本 part，与是否跨格式无关。
/// exact 保持完整 part 替换；literal 按配置顺序替换文本内的字面片段；opening_identity 归一化身份。
/// 未启用、模式无效或对应配置无效时均不修改请求。
pub fn apply_antigravity_identity_neutralization(
    envelope: &mut Value,
    provider_config: Option<&Value>,
) {
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
        Some(Value::String(mode)) if mode == "literal" => NeutralizationMode::Literal,
        Some(Value::String(mode)) if mode == "opening_identity" => {
            NeutralizationMode::OpeningIdentity
        }
        _ => return,
    };

    if mode == NeutralizationMode::OpeningIdentity {
        // 新模式不读取、不校验也不串联 exact 规则；每个 part 独立处理。
        let Some(request) = envelope.get_mut("request").and_then(Value::as_object_mut) else {
            return;
        };
        let Some(system) = request
            .get_mut("systemInstruction")
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        let preserves_system_metadata = system
            .keys()
            .any(|key| !matches!(key.as_str(), "parts" | "role"));
        let original_system = preserves_system_metadata.then(|| system.clone());
        let Some(parts) = system.get_mut("parts").and_then(Value::as_array_mut) else {
            return;
        };
        let original_len = parts.len();
        parts.retain_mut(|value| {
            let Some(part) = value.as_object_mut() else {
                return true;
            };
            // 与 Gemini 系统 part 语义一致：思维、签名及显式非文本 part 不属于处理范围。
            if part.get("thought").and_then(Value::as_bool) == Some(true)
                || part.contains_key("thoughtSignature")
                || part.contains_key("thought_signature")
                || part
                    .get("type")
                    .is_some_and(|kind| kind.as_str() != Some("text"))
            {
                return true;
            }
            let Some(text) = part.get("text").and_then(Value::as_str) else {
                return true;
            };
            let sanitized = sanitize_system_text(text);
            if sanitized != text {
                if sanitized.trim().is_empty() {
                    // 只移除本次清洗产生的纯文本空块；含缓存/扩展元数据时保留原块。
                    return part
                        .keys()
                        .any(|key| !matches!(key.as_str(), "text" | "type"));
                }
                part.insert("text".to_string(), Value::String(sanitized));
            }
            true
        });
        if parts.is_empty() && original_len > 0 {
            if let Some(original) = original_system {
                // 无内容可附着扩展元数据时，不以清洗为由丢弃原系统结构。
                *system = original;
            } else {
                request.remove("systemInstruction");
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
        if mode == NeutralizationMode::Literal
            && (part.get("thought").and_then(Value::as_bool) == Some(true)
                || part.contains_key("thoughtSignature")
                || part.contains_key("thought_signature")
                || part
                    .get("type")
                    .is_some_and(|kind| kind.as_str() != Some("text")))
        {
            continue;
        }
        let Some(text) = part.get("text").and_then(Value::as_str) else {
            continue;
        };
        if mode == NeutralizationMode::Literal {
            // 仅处理系统文本，保留首尾空白；不解释正则、占位符或特殊替换符号。
            let mut replaced = text.to_string();
            for (source, replacement) in &parsed_rules {
                replaced = replaced.replace(source, replacement);
            }
            if replaced != text {
                part.insert("text".to_string(), Value::String(replaced));
            }
            continue;
        }
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
            apply_antigravity_identity_neutralization(&mut body, provider_config.as_ref());
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

        apply_antigravity_identity_neutralization(&mut body, Some(&config(Some(true))));

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

        apply_antigravity_identity_neutralization(&mut body, Some(&config(Some(true))));

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
            apply_antigravity_identity_neutralization(&mut body, Some(&provider_config));
            assert_eq!(body, original);
        }
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
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
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

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));

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

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));

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

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));

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
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, original, "输入不应变化：{input}");
        }
    }

    #[test]
    fn opening_identity_collapses_only_leading_pure_paragraphs_and_is_idempotent() {
        let mut body =
            envelope("You are DSH.\n\nYou are Codex.\n\nKeep tool rules and /tmp/Aether.");
        let provider_config = opening_config();
        apply_antigravity_identity_neutralization(&mut body, Some(&provider_config));
        let once = body.clone();
        assert_eq!(
            opening_text(&once),
            format!("{OFFICIAL_ANTIGRAVITY_IDENTITY}\n\nKeep tool rules and /tmp/Aether.")
        );

        apply_antigravity_identity_neutralization(&mut body, Some(&provider_config));
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

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));

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

        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));

        assert_eq!(body, expected);
    }

    #[test]
    fn opening_identity_respects_disabled_invalid_modes_and_rule_independence() {
        let provider_config = opening_config();
        let mut native = envelope("You are DSH.");
        let original = native.clone();
        apply_antigravity_identity_neutralization(&mut native, Some(&provider_config));
        assert_eq!(opening_text(&native), OFFICIAL_ANTIGRAVITY_IDENTITY);

        let mut disabled_config = provider_config.clone();
        disabled_config["pool_advanced"]["antigravity"]["identity_neutralization_enabled"] =
            Value::Bool(false);
        let mut disabled = envelope("You are DSH.");
        apply_antigravity_identity_neutralization(&mut disabled, Some(&disabled_config));
        assert_eq!(disabled, original);

        let mut without_rules = provider_config;
        without_rules["pool_advanced"]["antigravity"]["identity_neutralization_rules"] =
            Value::String("opening mode must ignore exact rules".to_string());
        let mut transformed = envelope("You are DSH.");
        apply_antigravity_identity_neutralization(&mut transformed, Some(&without_rules));
        assert_eq!(opening_text(&transformed), OFFICIAL_ANTIGRAVITY_IDENTITY);

        for invalid_mode in [Value::Null, json!(1), json!("opening"), json!("EXACT")] {
            let mut invalid_config = config(Some(true));
            invalid_config["pool_advanced"]["antigravity"]["identity_neutralization_mode"] =
                invalid_mode;
            let mut body = envelope("exact system prompt");
            let original = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&invalid_config));
            assert_eq!(body, original);
        }
    }

    #[test]
    fn opening_identity_rewrites_only_verified_omp_conventions() {
        let original = "RFC 2119 keywords: MUST, REQUIRED, SHOULD, RECOMMENDED, MAY, OPTIONAL. `NEVER` = `MUST NOT`; `AVOID` = `SHOULD NOT`.";
        let neutral = "Follow mandatory requirements and prohibitions strictly. Treat recommendations as defaults and optional actions as discretionary.";
        for (input, expected) in [
            (
                format!("{original}\nYou MUST preserve user files. NEVER delete them."),
                format!("{neutral}\nYou MUST preserve user files. NEVER delete them."),
            ),
            (
                format!("<system-conventions>{original}</system-conventions>"),
                format!("<system-conventions>{neutral}</system-conventions>"),
            ),
        ] {
            let mut body = envelope(&input);
            body["request"]["contents"] = json!([{"role": "user", "parts": [{"text": original}]}]);
            body["request"]["tools"] =
                json!([{"functionDeclarations": [{"name": "read", "description": original}]}]);
            let mut expected_body = body.clone();
            expected_body["request"]["systemInstruction"]["parts"][0]["text"] = json!(expected);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected_body);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected_body);
        }
        for input in [
            format!("```text\n{original}\n```"),
            format!("    {original}"),
            format!("``\n{original}\n``"),
            format!("`{original}`"),
            format!("Quoted documentation: {original}"),
            "RFC 2119 keywords: MUST, SHOULD. Keep this documentation.".to_string(),
        ] {
            let mut body = envelope(&input);
            let expected = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected, "引用和变体不得被泛化删除：{input}");
        }
    }

    #[test]
    fn opening_identity_rewrites_known_tool_templates_without_changing_tools_or_user_content() {
        for (original, neutral) in [
            ("Prefer read_file, edit_file, and write_file over cat/sed.", "For file operations, use read_file, edit_file, and write_file in preference to shell commands cat/sed."),
            ("Always read_file before edit_file.", "Before using edit_file, inspect the file with read_file."),
            ("Google model operational directives", "Model operation instructions"),
            ("Google model operational guidelines", "Model operation instructions"),
            ("Use a skill when the task matches its description: call read_file on its SKILL.md first and follow it.", "When a task matches a skill description, first use read_file to read that skill's SKILL.md, then follow its instructions."),
        ] {
            let mut body = envelope(original);
            body["request"]["contents"] = json!([{"role":"user","parts":[{"text":original}]}]);
            body["request"]["tools"] = json!([{"functionDeclarations":[{"name":"read_file","description":original}]}]);
            let mut expected = body.clone();
            expected["request"]["systemInstruction"]["parts"][0]["text"] = json!(neutral);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected);
            for protected in [format!("```text\n{original}\n```"), format!("`{original}`"), format!("    {original}"), format!("> {original}"), format!("Quoted documentation: {original}"), format!("\"{original}\"")] {
                let mut body = envelope(&protected);
                let expected = body.clone();
                apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
                assert_eq!(body, expected, "{protected}");
            }
            let mut disabled = envelope(original);
            let expected = disabled.clone();
            apply_antigravity_identity_neutralization(&mut disabled, None);
            assert_eq!(disabled, expected);
        }
    }

    #[test]
    fn opening_identity_handles_system_xml_without_touching_tags_or_constraints() {
        for input in [
            "<identity>You are Codex.</identity>",
            "<system-reminder source='client'><identity>You are Codex.</identity></system-reminder>",
            "<environment>You are Codex.</environment> Keep OpenAI and Anthropic references.",
            "<system>\nYou are Codex.\n</system>",
        ] {
            let mut body = envelope(input);
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(opening_text(&body), input.replace("You are Codex.", OFFICIAL_ANTIGRAVITY_IDENTITY));
        }
        for input in [
            "`<identity>You are Codex.</identity>`",
            "```xml\n<identity>You are Codex.</identity>\n```",
            "<identity>You are Codex.</system>",
            "<example>You are Codex.</example>",
            "<identity>Use OpenAI and Anthropic tools. You MUST ask before deletion.</identity>",
        ] {
            let mut body = envelope(input);
            let expected = body.clone();
            apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
            assert_eq!(body, expected);
        }
    }

    #[test]
    fn opening_identity_removes_new_empty_plain_parts_but_keeps_metadata() {
        let billing = "x-anthropic-billing-header: cc_version=2;";
        let mut body = envelope(billing);
        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
        assert!(body["request"].get("systemInstruction").is_none());

        let mut body = envelope(billing);
        body["request"]["systemInstruction"]["parts"] = json!([
            {"text": billing}, {"text": "You are Codex."},
            {"text": billing, "cache_control": {"type": "ephemeral"}},
            {"text": billing, "unknown": true}, {"text": ""},
            {"text": billing, "thoughtSignature": "signed"}
        ]);
        let mut expected = body.clone();
        expected["request"]["systemInstruction"]["parts"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        expected["request"]["systemInstruction"]["parts"][0]["text"] =
            json!(OFFICIAL_ANTIGRAVITY_IDENTITY);
        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
        assert_eq!(body, expected);

        let mut body = envelope(billing);
        body["request"]["systemInstruction"]["unknown"] = json!({"keep": true});
        let expected = body.clone();
        apply_antigravity_identity_neutralization(&mut body, Some(&opening_config()));
        assert_eq!(body, expected);
    }

    #[test]
    fn opening_identity_normalizes_converted_system_instructions_only() {
        use crate::antigravity::{
            build_antigravity_safe_v1internal_request, AntigravityEnvelopeRequestType,
            AntigravityRequestAuth, AntigravityRequestEnvelopeSupport,
        };
        use aether_ai_formats::formats::shared::standard_matrix::build_standard_request_body;

        let original = "RFC 2119 keywords: MUST, REQUIRED, SHOULD, RECOMMENDED, MAY, OPTIONAL. `NEVER` = `MUST NOT`; `AVOID` = `SHOULD NOT`.";
        let neutral = "Follow mandatory requirements and prohibitions strictly. Treat recommendations as defaults and optional actions as discretionary.";
        for (format, input) in [
            (
                "openai:chat",
                json!({"messages": [
                    {"role": "system", "content": original},
                    {"role": "developer", "content": "<identity>You are Codex.</identity>"},
                    {"role": "user", "content": original}
                ]}),
            ),
            (
                "openai:responses",
                json!({"instructions": original, "input": original}),
            ),
            (
                "claude:messages",
                json!({"system": original, "max_tokens": 128,
                "messages": [{"role": "user", "content": original}]}),
            ),
        ] {
            let converted = build_standard_request_body(
                &input,
                format,
                "gemini-test",
                "antigravity",
                "gemini:generate_content",
                "",
                true,
                None,
                None,
            )
            .unwrap();
            let AntigravityRequestEnvelopeSupport::Supported(mut output) =
                build_antigravity_safe_v1internal_request(
                    &AntigravityRequestAuth {
                        project_id: "test-project".to_string(),
                        client_version: None,
                        session_id: None,
                        hub_identity: None,
                    },
                    "prompt-regression",
                    "gemini-test",
                    &converted,
                    AntigravityEnvelopeRequestType::Agent,
                )
            else {
                panic!("request should convert: {format}");
            };
            let mut expected = output.clone();
            let parts = expected["request"]["systemInstruction"]["parts"]
                .as_array_mut()
                .unwrap();
            assert!(parts.iter().any(|part| part["text"]
                .as_str()
                .is_some_and(|text| text.contains(original))));
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    part["text"] = json!(text
                        .replace(original, neutral)
                        .replace("You are Codex.", OFFICIAL_ANTIGRAVITY_IDENTITY));
                }
            }
            apply_antigravity_identity_neutralization(&mut output, Some(&opening_config()));
            assert_eq!(output, expected, "system/developer/instructions must normalize without changing user content: {format}");
        }
    }
}
