use serde_json::json;

use super::apply_antigravity_identity_neutralization;

fn config(rules: serde_json::Value) -> serde_json::Value {
    json!({"pool_advanced": {"antigravity": {
        "identity_neutralization_enabled": true,
        "identity_neutralization_mode": "literal",
        "identity_neutralization_rules": rules
    }}})
}

#[test]
fn literal_replaces_fragments_only_in_plain_system_parts() {
    let source = "Read xd://<tool> for full docs + JSON schema before first use.";
    let replacement = "工具首次使用前，先读取对应的 xd:// 文档，确认使用说明和参数结构。";
    let config = config(json!([{"match": source, "replacement": replacement}]));
    let mut body = json!({"request": {
        "systemInstruction": {"custom": true, "parts": [
            {"text": format!("Before\n{source}\n{source}\nAfter"), "cacheControl": {"type": "ephemeral"}},
            {"text": source, "thought": true},
            {"text": source, "thoughtSignature": "signed"},
            {"text": source, "thought_signature": "signed"},
            {"text": source, "type": "image"},
            {"text": source.to_lowercase()},
            {"text": source, "type": "text"}
        ]},
        "contents": [{"role": "user", "parts": [{"text": source}, {"functionResponse": {"name": "read", "response": {"text": source}}}]}],
        "tools": [{"functionDeclarations": [{"name": "read", "description": source}]}],
        "custom": {"text": source}
    }});
    let mut expected = body.clone();
    expected["request"]["systemInstruction"]["parts"][0]["text"] =
        json!(format!("Before\n{replacement}\n{replacement}\nAfter"));
    expected["request"]["systemInstruction"]["parts"][6]["text"] = json!(replacement);
    apply_antigravity_identity_neutralization(&mut body, Some(&config));
    assert_eq!(body, expected);
    apply_antigravity_identity_neutralization(&mut body, Some(&config));
    assert_eq!(body, expected);
}

#[test]
fn literal_is_ordered_not_regex_and_does_not_chain_identity_rewrite() {
    let mut config = config(json!([
        {"match": "[a]+", "replacement": "中间"},
        {"match": "中间", "replacement": "$1"},
        {"match": "remove", "replacement": ""}
    ]));
    let mut body = json!({"request": {"systemInstruction": {"parts": [
        {"text": "You are Codex. [a]+ aaa remove [a]+"}
    ]}}});
    apply_antigravity_identity_neutralization(&mut body, Some(&config));
    assert_eq!(
        body["request"]["systemInstruction"]["parts"][0]["text"],
        "You are Codex. $1 aaa  $1"
    );
    config["pool_advanced"]["antigravity"]["identity_neutralization_enabled"] = json!(false);
    let mut disabled = json!({"request": {"systemInstruction": {"parts": [{"text": "[a]+"}]}}});
    let original = disabled.clone();
    apply_antigravity_identity_neutralization(&mut disabled, Some(&config));
    assert_eq!(disabled, original);
}

#[test]
fn literal_invalid_rules_preserve_the_entire_request() {
    for invalid in [
        json!({"match": "", "replacement": "x"}),
        json!({"match": "later"}),
        json!({"match": "source", "replacement": "duplicate"}),
        json!({"match": "x".repeat(16_385), "replacement": "x"}),
        json!({"match": "later", "replacement": "x".repeat(16_385)}),
    ] {
        let config = config(json!([{"match": "source", "replacement": "changed"}, invalid]));
        let mut body = json!({"request": {"systemInstruction": {"parts": [{"text": "prefix source suffix"}]}}});
        let original = body.clone();
        apply_antigravity_identity_neutralization(&mut body, Some(&config));
        assert_eq!(body, original);
    }
}
