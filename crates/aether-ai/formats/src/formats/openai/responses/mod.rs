use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde_json::Value;

pub mod codex;
pub(crate) mod history;
pub mod request;
pub mod response;
pub mod spec;
pub mod stream;

const TOOL_ERROR_PREFIX: &str = "[tool error]";
const AETHER_REASONING_ITEM_ID_PREFIX: &str = "rs_aether_";
/// Aether 合成的 Gemini 工具签名 carrier 前缀，用于与 provider reasoning 区分。
const GEMINI_TOOL_SIGNATURE_CARRIER_PREFIX: &str = "cpa-gemini-responses-carrier-v1:";
/// 单个原始签名上限；解码前后都校验，避免 carrier 放大无界内存。
const MAX_GEMINI_THOUGHT_SIGNATURE_LEN: usize = 32 * 1024 * 1024;
/// Base64 无填充编码的最大长度上界。
const MAX_GEMINI_THOUGHT_SIGNATURE_ENCODED_LEN: usize =
    MAX_GEMINI_THOUGHT_SIGNATURE_LEN.div_ceil(3) * 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 指明合成 reasoning carrier 应绑定前一个还是后一个工具调用。
pub(crate) enum GeminiToolSignatureCarrierDirection {
    /// carrier 位于调用前，绑定后续工具调用。
    Next,
    /// carrier 位于调用后，绑定上一工具调用。
    Previous,
}

impl GeminiToolSignatureCarrierDirection {
    /// 返回 carrier wire 中稳定的小写方向标识。
    fn as_str(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Previous => "previous",
        }
    }
}

/// 以默认 next 方向编码 Gemini 工具签名；空值或超限值拒绝生成。
pub(crate) fn encode_gemini_tool_signature_carrier(signature: &str) -> Option<String> {
    encode_gemini_tool_signature_carrier_with_direction(
        signature,
        GeminiToolSignatureCarrierDirection::Next,
    )
}

/// 按指定方向编码签名，保留签名原始空白与字节值。
pub(crate) fn encode_gemini_tool_signature_carrier_with_direction(
    signature: &str,
    direction: GeminiToolSignatureCarrierDirection,
) -> Option<String> {
    (!signature.trim().is_empty() && signature.len() <= MAX_GEMINI_THOUGHT_SIGNATURE_LEN).then(
        || {
            format!(
                "{GEMINI_TOOL_SIGNATURE_CARRIER_PREFIX}{}:function:{}",
                direction.as_str(),
                STANDARD_NO_PAD.encode(signature)
            )
        },
    )
}

/// 解码并校验 Aether Gemini carrier；拒绝未知方向、嵌套 carrier、非法 Base64 与超限值。
pub(crate) fn decode_gemini_tool_signature_carrier(
    carrier: &str,
) -> Option<(String, GeminiToolSignatureCarrierDirection)> {
    let payload = carrier.strip_prefix(GEMINI_TOOL_SIGNATURE_CARRIER_PREFIX)?;
    let (direction, encoded) = payload.split_once(":function:")?;
    let direction = match direction {
        "next" => GeminiToolSignatureCarrierDirection::Next,
        "previous" => GeminiToolSignatureCarrierDirection::Previous,
        _ => return None,
    };
    if encoded.len() > MAX_GEMINI_THOUGHT_SIGNATURE_ENCODED_LEN {
        return None;
    }
    let decoded = STANDARD_NO_PAD.decode(encoded).ok()?;
    if decoded.len() > MAX_GEMINI_THOUGHT_SIGNATURE_LEN {
        return None;
    }
    let signature = String::from_utf8(decoded).ok()?;
    (!signature.trim().is_empty() && !signature.starts_with(GEMINI_TOOL_SIGNATURE_CARRIER_PREFIX))
        .then_some((signature, direction))
}

/// Controls which provider-owned reasoning items may be replayed on a Responses request.
///
/// OpenAI reasoning references are identified by their `rs...` item IDs. DeepSeek's Responses
/// contract instead returns opaque, id-less `reasoning_text` items whose `encrypted_content`
/// must be sent back unchanged on later tool turns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenAiResponsesReasoningReplayPolicy {
    #[default]
    OpenAiItemIds,
    DeepSeekOpaque,
}

/// Builds a stable, wire-compatible ID for a reasoning item synthesized by Aether.
///
/// The marker lets the outbound request sanitizer distinguish synthetic summaries from
/// provider-backed reasoning items. Synthetic items without encrypted reasoning state are useful
/// in client responses, but cannot be replayed as provider-owned reasoning state.
pub fn openai_responses_synthetic_reasoning_item_id(
    response_id: &str,
    output_index: usize,
) -> String {
    let seed = format!("{response_id}:{output_index}");
    format!(
        "{AETHER_REASONING_ITEM_ID_PREFIX}{}",
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, seed.as_bytes()).simple()
    )
}

/// Removes reasoning history items that cannot be replayed against an OpenAI Responses backend.
///
/// Reasoning IDs are opaque provider references and must never be repaired by changing their
/// prefix. Foreign IDs (for example `item_...`) are therefore removed. Aether-synthesized
/// reasoning summaries are also removed unless they carry encrypted reasoning state that can be
/// replayed statelessly.
pub fn strip_incompatible_openai_responses_reasoning_items(
    body: &mut Value,
    provider_api_format: &str,
) -> usize {
    strip_incompatible_openai_responses_reasoning_items_with_policy(
        body,
        provider_api_format,
        OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds,
    )
}

pub fn strip_incompatible_openai_responses_reasoning_items_with_policy(
    body: &mut Value,
    provider_api_format: &str,
    policy: OpenAiResponsesReasoningReplayPolicy,
) -> usize {
    if !aether_ai_formats::is_openai_responses_family_format(provider_api_format) {
        return 0;
    }
    // DeepSeek's id-less opaque state is valid only on the normal Responses
    // continuation contract. Both the legacy Compact endpoint and the current
    // `compaction_trigger` operation must retain the strict OpenAI item-id
    // replay rules even when the same provider key serves ordinary Responses.
    let normal_responses =
        aether_ai_formats::normalize_api_format_alias(provider_api_format) == "openai:responses";
    let compact_operation = openai_responses_request_operation(provider_api_format, body).is_some();
    let policy = if policy == OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
        && (!normal_responses || compact_operation)
    {
        OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds
    } else {
        policy
    };
    let Some(items) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return 0;
    };
    let original_len = items.len();
    items.retain(|item| openai_responses_reasoning_item_is_replayable(item, policy));
    original_len.saturating_sub(items.len())
}

fn openai_responses_reasoning_item_is_replayable(
    item: &Value,
    policy: OpenAiResponsesReasoningReplayPolicy,
) -> bool {
    let Some(object) = item.as_object() else {
        return true;
    };
    if object.get("type").and_then(Value::as_str) != Some("reasoning") {
        return true;
    }
    if policy == OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
        && deepseek_opaque_reasoning_item_is_replayable(object)
    {
        return true;
    }
    let Some(id) = object
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| id.starts_with("rs"))
    else {
        return false;
    };
    if !id.starts_with(AETHER_REASONING_ITEM_ID_PREFIX) {
        return true;
    }
    object
        .get("encrypted_content")
        .and_then(Value::as_str)
        .is_some_and(|encrypted_content| !encrypted_content.trim().is_empty())
}

/// 删除官方 OpenAI/Codex Responses 上游会拒绝的类型化输入 Item ID。
///
/// Item ID 是不透明的上游引用，前缀不兼容时只能删除，不能伪造新前缀。`call_id` 不属于
/// Item ID，因此保持原样以维持工具调用与结果的配对。
pub fn strip_incompatible_openai_responses_input_item_ids(
    body: &mut Value,
    provider_type: &str,
    provider_api_format: &str,
) -> usize {
    let is_official_responses_provider = provider_type.trim().eq_ignore_ascii_case("openai")
        || provider_type.trim().eq_ignore_ascii_case("codex");
    if !is_official_responses_provider
        || !aether_ai_formats::is_openai_responses_family_format(provider_api_format)
    {
        return 0;
    }
    let Some(items) = body.get_mut("input").and_then(Value::as_array_mut) else {
        return 0;
    };

    let mut stripped = 0;
    for item in items {
        let Some(object) = item.as_object_mut() else {
            continue;
        };
        let Some(expected_prefix) = object
            .get("type")
            .and_then(Value::as_str)
            .and_then(openai_responses_input_item_id_prefix)
        else {
            continue;
        };
        let incompatible = object
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.starts_with(expected_prefix));
        if incompatible {
            object.remove("id");
            stripped += 1;
        }
    }
    stripped
}

fn openai_responses_input_item_id_prefix(item_type: &str) -> Option<&'static str> {
    match item_type {
        "message" => Some("msg"),
        "function_call" | "tool_call" | "local_shell_call" | "custom_tool_call"
        | "mcp_tool_call" => Some("fc"),
        _ => None,
    }
}

fn deepseek_opaque_reasoning_item_is_replayable(object: &serde_json::Map<String, Value>) -> bool {
    if let Some(id) = object.get("id") {
        let id_is_empty = id.is_null() || id.as_str().is_some_and(|value| value.trim().is_empty());
        if !id_is_empty {
            return false;
        }
    }
    let has_encrypted_content = object
        .get("encrypted_content")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let has_reasoning_text =
        object
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|content| {
                content.iter().any(|part| {
                    part.get("type").and_then(Value::as_str) == Some("reasoning_text")
                        && part.get("text").is_some_and(Value::is_string)
                })
            });
    has_encrypted_content && has_reasoning_text
}

/// Semantic operation carried by an OpenAI Responses request that asks the
/// service to compact a thread. The request still uses the Responses wire
/// contract and transport endpoint.
pub const OPENAI_RESPONSES_OPERATION_COMPACT: &str = "compact";

/// Resolves the operation expressed by an OpenAI Responses wire request.
///
/// `responses_compaction_v2` is represented by a `compaction_trigger` input
/// item on the normal Responses request. The legacy Compact API format is
/// retained as the same operation for observability and scoped model mapping.
pub fn openai_responses_request_operation(api_format: &str, body: &Value) -> Option<&'static str> {
    if aether_ai_formats::is_openai_responses_compact_format(api_format) {
        return Some(OPENAI_RESPONSES_OPERATION_COMPACT);
    }
    if !aether_ai_formats::is_openai_responses_format(api_format) {
        return None;
    }

    body.get("input")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction_trigger"))
        })
        .then_some(OPENAI_RESPONSES_OPERATION_COMPACT)
}

fn encode_tool_result_error(output: Value, is_error: bool) -> Value {
    if !is_error {
        return output;
    }
    let detail = match output {
        Value::String(text) => text,
        Value::Null => String::new(),
        value => serde_json::to_string(&value).unwrap_or_else(|_| value.to_string()),
    };
    if detail.is_empty() {
        Value::String(TOOL_ERROR_PREFIX.to_string())
    } else {
        Value::String(format!("{TOOL_ERROR_PREFIX}\n{detail}"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        decode_gemini_tool_signature_carrier, encode_gemini_tool_signature_carrier_with_direction,
        openai_responses_request_operation, openai_responses_synthetic_reasoning_item_id,
        strip_incompatible_openai_responses_input_item_ids,
        strip_incompatible_openai_responses_reasoning_items,
        strip_incompatible_openai_responses_reasoning_items_with_policy,
        GeminiToolSignatureCarrierDirection, OpenAiResponsesReasoningReplayPolicy,
        MAX_GEMINI_THOUGHT_SIGNATURE_ENCODED_LEN, MAX_GEMINI_THOUGHT_SIGNATURE_LEN,
        OPENAI_RESPONSES_OPERATION_COMPACT,
    };

    /// 验证两个方向均能无损往返包含空白与填充字符的签名。
    #[test]
    fn gemini_tool_signature_carrier_roundtrips_direction_and_exact_value() {
        let signature = "  opaque-signature-with-padding==  ";
        for direction in [
            GeminiToolSignatureCarrierDirection::Next,
            GeminiToolSignatureCarrierDirection::Previous,
        ] {
            let carrier = encode_gemini_tool_signature_carrier_with_direction(signature, direction)
                .expect("signature carrier");
            assert_eq!(
                decode_gemini_tool_signature_carrier(&carrier),
                Some((signature.to_string(), direction))
            );
        }
    }

    /// 验证嵌套 carrier 与编码前后超限值都被拒绝。
    #[test]
    fn gemini_tool_signature_carrier_rejects_nested_and_oversized_values() {
        let nested = encode_gemini_tool_signature_carrier_with_direction(
            "opaque-signature",
            GeminiToolSignatureCarrierDirection::Next,
        )
        .expect("inner carrier");
        let nested = encode_gemini_tool_signature_carrier_with_direction(
            &nested,
            GeminiToolSignatureCarrierDirection::Previous,
        )
        .expect("outer carrier");
        assert_eq!(decode_gemini_tool_signature_carrier(&nested), None);
        assert_eq!(
            encode_gemini_tool_signature_carrier_with_direction(
                &"x".repeat(MAX_GEMINI_THOUGHT_SIGNATURE_LEN + 1),
                GeminiToolSignatureCarrierDirection::Next,
            ),
            None
        );
        let oversized = format!(
            "cpa-gemini-responses-carrier-v1:next:function:{}",
            "A".repeat(MAX_GEMINI_THOUGHT_SIGNATURE_ENCODED_LEN + 1)
        );
        assert_eq!(decode_gemini_tool_signature_carrier(&oversized), None);
    }

    #[test]
    fn resolves_compaction_trigger_as_compact_operation_on_responses_transport() {
        assert_eq!(
            openai_responses_request_operation(
                "openai:responses",
                &json!({
                    "input": [
                        {"role": "user", "content": "keep working"},
                        {"type": "compaction_trigger"}
                    ]
                }),
            ),
            Some(OPENAI_RESPONSES_OPERATION_COMPACT)
        );
        assert_eq!(
            openai_responses_request_operation(
                "openai:responses",
                &json!({"input": [{"role": "user", "content": "keep working"}]}),
            ),
            None
        );
    }

    #[test]
    fn resolves_legacy_compact_contract_without_a_body_marker() {
        assert_eq!(
            openai_responses_request_operation("openai:responses:compact", &json!({})),
            Some(OPENAI_RESPONSES_OPERATION_COMPACT)
        );
    }

    #[test]
    fn synthetic_reasoning_item_ids_are_stable_and_wire_compatible() {
        let first = openai_responses_synthetic_reasoning_item_id("resp_123", 0);
        let second = openai_responses_synthetic_reasoning_item_id("resp_123", 0);
        let other = openai_responses_synthetic_reasoning_item_id("resp_123", 1);

        assert!(first.starts_with("rs_aether_"));
        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    #[test]
    fn strips_foreign_and_non_replayable_synthetic_reasoning_items() {
        let portable_synthetic = openai_responses_synthetic_reasoning_item_id("resp_123", 1);
        let local_synthetic = openai_responses_synthetic_reasoning_item_id("resp_123", 2);
        let mut body = json!({
            "input": [
                {"type": "reasoning", "id": "rs_provider_123", "summary": []},
                {"type": "reasoning", "id": "item_72d3bd8d367d01977ace23f1", "summary": []},
                {"type": "reasoning", "id": "resp_123_rs_0", "summary": []},
                {"type": "reasoning", "summary": []},
                {
                    "type": "reasoning",
                    "id": portable_synthetic,
                    "summary": [],
                    "encrypted_content": "opaque"
                },
                {"type": "reasoning", "id": local_synthetic, "summary": []},
                {"type": "message", "id": "item_message_123", "role": "user", "content": "hi"}
            ]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items(&mut body, "openai:responses"),
            4
        );
        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 3);
        assert_eq!(input[0]["id"], "rs_provider_123");
        assert_eq!(input[1]["encrypted_content"], "opaque");
        assert_eq!(input[2]["id"], "item_message_123");
    }

    #[test]
    fn reasoning_item_sanitizer_is_scoped_to_responses_targets() {
        let mut body = json!({
            "input": [{"type": "reasoning", "id": "item_foreign", "summary": []}]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items(&mut body, "openai:chat"),
            0
        );
        assert_eq!(body["input"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn strips_invalid_typed_input_item_ids_without_breaking_tool_pairing() {
        let mut body = json!({
            "input": [
                {"type": "message", "id": "item_message", "role": "assistant", "content": []},
                {"type": "message", "id": "msg_valid", "role": "user", "content": "continue"},
                {
                    "type": "function_call",
                    "id": "item_call",
                    "call_id": "call_123",
                    "name": "exec_command",
                    "arguments": "{}"
                },
                {
                    "type": "function_call",
                    "id": "fc_valid",
                    "call_id": "call_456",
                    "name": "apply_patch",
                    "arguments": "{}"
                },
                {
                    "type": "tool_search_call",
                    "id": "tsc_valid",
                    "call_id": "call_search",
                    "execution": "client",
                    "arguments": {"query": "tools"}
                },
                {
                    "type": "function_call_output",
                    "id": "item_output",
                    "call_id": "call_123",
                    "output": "done"
                },
                {"type": "web_search_call", "id": "item_search"},
                {"type": "reasoning", "id": "rs_valid", "summary": []}
            ]
        });

        assert_eq!(
            strip_incompatible_openai_responses_input_item_ids(
                &mut body,
                "openai",
                "openai:responses"
            ),
            2
        );
        let input = body["input"].as_array().expect("input array");
        assert!(input[0].get("id").is_none());
        assert_eq!(input[1]["id"], "msg_valid");
        assert!(input[2].get("id").is_none());
        assert_eq!(input[2]["call_id"], "call_123");
        assert_eq!(input[3]["id"], "fc_valid");
        assert_eq!(input[4]["id"], "tsc_valid");
        assert_eq!(input[4]["call_id"], "call_search");
        assert_eq!(input[5]["id"], "item_output");
        assert_eq!(input[5]["call_id"], "call_123");
        assert_eq!(input[6]["id"], "item_search");
        assert_eq!(input[7]["id"], "rs_valid");
    }

    #[test]
    fn input_item_id_sanitizer_is_scoped_to_official_openai_responses_targets() {
        let original = json!({
            "input": [{"type": "message", "id": "item_foreign", "role": "user"}]
        });
        let mut chat_body = original.clone();
        let mut compatible_body = original.clone();

        assert_eq!(
            strip_incompatible_openai_responses_input_item_ids(
                &mut chat_body,
                "openai",
                "openai:chat"
            ),
            0
        );
        assert_eq!(chat_body, original);
        assert_eq!(
            strip_incompatible_openai_responses_input_item_ids(
                &mut compatible_body,
                "openai_compatible",
                "openai:responses"
            ),
            0
        );
        assert_eq!(compatible_body, original);
    }

    #[test]
    fn codex_responses_targets_strip_foreign_typed_item_ids() {
        let mut body = json!({
            "input": [
                {"type": "message", "id": "item_message", "role": "user"},
                {
                    "type": "function_call",
                    "id": "item_call",
                    "call_id": "call_123",
                    "name": "exec_command",
                    "arguments": "{}"
                },
                {
                    "type": "function_call_output",
                    "id": "fco_123",
                    "call_id": "call_123",
                    "output": "ok"
                }
            ]
        });

        assert_eq!(
            strip_incompatible_openai_responses_input_item_ids(
                &mut body,
                "codex",
                "openai:responses"
            ),
            2
        );
        let input = body["input"].as_array().expect("input array");
        assert!(input[0].get("id").is_none());
        assert!(input[1].get("id").is_none());
        assert_eq!(input[1]["call_id"], "call_123");
        assert_eq!(input[2]["id"], "fco_123");
    }

    #[test]
    fn deepseek_policy_preserves_idless_opaque_reasoning_text_only_for_deepseek() {
        let item = json!({
            "type": "reasoning",
            "encrypted_content": "550e8400-e29b-41d4-a716-446655440000",
            "content": [{
                "type": "reasoning_text",
                "text": "opaque provider reasoning that must be replayed"
            }]
        });
        let mut strict = json!({"input": [item.clone()]});
        let mut deepseek = json!({"input": [item]});

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut strict,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds,
            ),
            1
        );
        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut deepseek,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            0
        );
        assert_eq!(deepseek["input"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn deepseek_policy_does_not_preserve_unbound_reasoning_summaries() {
        let mut body = json!({
            "input": [
                {
                    "type": "reasoning",
                    "content": [{"type": "reasoning_text", "text": "missing state"}]
                },
                {
                    "type": "reasoning",
                    "encrypted_content": "opaque-without-reasoning-text",
                    "summary": [{"type": "summary_text", "text": "summary"}]
                }
            ]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut body,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            2
        );
        assert_eq!(body["input"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn deepseek_policy_preserves_empty_reasoning_text_with_opaque_state() {
        let mut body = json!({
            "input": [{
                "type": "reasoning",
                "encrypted_content": "opaque-state",
                "content": [{"type": "reasoning_text", "text": ""}],
                "future_capability": {"preserve": true}
            }]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut body,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            0
        );
        assert_eq!(body["input"][0]["content"][0]["text"], "");
        assert_eq!(body["input"][0]["future_capability"]["preserve"], true);
    }

    #[test]
    fn deepseek_policy_rejects_non_string_reasoning_text() {
        let mut body = json!({
            "input": [{
                "type": "reasoning",
                "encrypted_content": "opaque-state",
                "content": [{"type": "reasoning_text", "text": {"not": "text"}}]
            }]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut body,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            1
        );
        assert_eq!(body["input"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn deepseek_policy_keeps_strict_replay_for_compaction_trigger_operation() {
        let mut body = json!({
            "input": [
                {
                    "type": "reasoning",
                    "encrypted_content": "opaque-state",
                    "content": [{"type": "reasoning_text", "text": "thinking"}]
                },
                {"type": "compaction_trigger"}
            ]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut body,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            1
        );
        assert_eq!(body["input"], json!([{"type": "compaction_trigger"}]));
    }

    #[test]
    fn deepseek_policy_does_not_preserve_opaque_item_with_foreign_id() {
        let mut body = json!({
            "input": [{
                "type": "reasoning",
                "id": "item_provider_owned",
                "encrypted_content": "opaque-state",
                "content": [{"type": "reasoning_text", "text": "thinking"}]
            }]
        });

        assert_eq!(
            strip_incompatible_openai_responses_reasoning_items_with_policy(
                &mut body,
                "openai:responses",
                OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
            ),
            1
        );
        assert_eq!(body["input"].as_array().map(Vec::len), Some(0));
    }
}
