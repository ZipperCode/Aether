use std::collections::BTreeSet;

use serde_json::Value;

const MAX_IDENTITY_NEUTRALIZATION_RULES: usize = 32;
const MAX_IDENTITY_NEUTRALIZATION_TEXT_BYTES: usize = 16_384;

/// 仅在跨格式 Antigravity 请求中按显式规则替换完整的系统指令文本 part。
/// 同格式请求保持透传；未启用、缺少配置或规则格式无效时均不修改请求。
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

    use super::apply_antigravity_identity_neutralization;

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
}
