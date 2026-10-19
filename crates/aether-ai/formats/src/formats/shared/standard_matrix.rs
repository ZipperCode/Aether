use std::borrow::Cow;

use aether_ai_formats::formats::conversion::request::{
    convert_openai_chat_request_to_claude_request, convert_openai_chat_request_to_gemini_request,
    convert_openai_chat_request_to_openai_responses_request,
    normalize_claude_request_to_openai_chat_request,
    normalize_gemini_request_to_openai_chat_request,
    normalize_openai_responses_request_to_openai_chat_request,
};
use aether_ai_formats::formats::registry::{convert_request, FormatContext};
use aether_ai_formats::provider_compat::proxy::rules::apply_local_body_rules_with_request_headers;
use serde_json::Value;

use crate::formats::shared::model_directives::apply_model_directive_overrides_from_request;

use crate::formats::openai::responses::codex::{
    apply_codex_openai_responses_chat_body_edits, apply_codex_openai_responses_special_body_edits,
    apply_openai_responses_compact_special_body_edits,
};
use crate::formats::openai::responses::xai::apply_xai_upstream_payload_edits_with_client;
use crate::formats::shared::standard_normalize::{
    build_local_openai_chat_request_body_with_model_directives,
    is_claude_messages_shaped_body_on_openai_chat_endpoint,
};

/// Tool schema preservation is a format-conversion policy, shared by the
/// standard matrix and provider-aware Chat/Responses entry points.
pub(super) fn preserves_gemini_tool_schemas(
    provider_type: &str,
    provider_api_format: &str,
) -> bool {
    provider_type.trim().eq_ignore_ascii_case("antigravity")
        && aether_ai_formats::normalize_api_format_alias(provider_api_format)
            == "gemini:generate_content"
}

#[allow(clippy::too_many_arguments)]
pub fn build_standard_request_body(
    body_json: &Value,
    client_api_format: &str,
    mapped_model: &str,
    provider_type: &str,
    provider_api_format: &str,
    request_path: &str,
    upstream_is_stream: bool,
    body_rules: Option<&Value>,
    user_api_key_id: Option<&str>,
) -> Option<Value> {
    build_standard_request_body_with_model_directives(
        body_json,
        client_api_format,
        mapped_model,
        provider_type,
        provider_api_format,
        request_path,
        upstream_is_stream,
        body_rules,
        user_api_key_id,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_standard_request_body_with_model_directives(
    body_json: &Value,
    client_api_format: &str,
    mapped_model: &str,
    provider_type: &str,
    provider_api_format: &str,
    request_path: &str,
    upstream_is_stream: bool,
    body_rules: Option<&Value>,
    user_api_key_id: Option<&str>,
    enable_model_directives: bool,
) -> Option<Value> {
    build_standard_request_body_with_model_directives_and_request_headers(
        body_json,
        client_api_format,
        mapped_model,
        provider_type,
        provider_api_format,
        request_path,
        upstream_is_stream,
        body_rules,
        user_api_key_id,
        None,
        enable_model_directives,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_standard_request_body_with_model_directives_and_request_headers(
    body_json: &Value,
    client_api_format: &str,
    mapped_model: &str,
    provider_type: &str,
    provider_api_format: &str,
    request_path: &str,
    upstream_is_stream: bool,
    body_rules: Option<&Value>,
    user_api_key_id: Option<&str>,
    request_headers: Option<&http::HeaderMap>,
    enable_model_directives: bool,
) -> Option<Value> {
    build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
        body_json,
        client_api_format,
        mapped_model,
        provider_type,
        provider_api_format,
        request_path,
        upstream_is_stream,
        body_rules,
        user_api_key_id,
        request_headers,
        enable_model_directives,
        crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
    body_json: &Value,
    client_api_format: &str,
    mapped_model: &str,
    provider_type: &str,
    provider_api_format: &str,
    request_path: &str,
    upstream_is_stream: bool,
    body_rules: Option<&Value>,
    user_api_key_id: Option<&str>,
    request_headers: Option<&http::HeaderMap>,
    enable_model_directives: bool,
    reasoning_replay_policy: crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy,
) -> Option<Value> {
    let reasoning_replay_policy = if provider_type.trim().eq_ignore_ascii_case("xai") {
        crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::XaiEncrypted
    } else {
        reasoning_replay_policy
    };
    let mut format_context = FormatContext::default()
        .with_mapped_model(mapped_model)
        .with_request_path(request_path)
        .with_upstream_stream(upstream_is_stream);
    format_context.preserve_gemini_tool_schemas =
        preserves_gemini_tool_schemas(provider_type, provider_api_format);
    if let Some(history_scope) = user_api_key_id {
        format_context = format_context.with_history_scope(history_scope);
    }
    let source_api_format = compatible_source_format_for_standard_request(
        body_json,
        client_api_format,
        provider_api_format,
    );
    // Keep the specialized OpenAI builders' compatibility/history preprocessing
    // when routing them through the provider-aware schema-preserving path.
    let antigravity_chat_body = if format_context.preserve_gemini_tool_schemas
        && matches!(
            aether_ai_formats::normalize_api_format_alias(source_api_format.as_ref()).as_str(),
            "openai:chat" | "openai:responses" | "openai:responses:compact"
        ) {
        Some(
            crate::formats::shared::standard_normalize::chat_compatible_body_for_standard_source(
                body_json,
                source_api_format.as_ref(),
                user_api_key_id,
            )?,
        )
    } else {
        None
    };
    // DeepSeek and xAI replay opaque provider state. Preserve their native
    // Responses input items: canonical conversion can lose reasoning IDs and
    // encrypted-only items even when source and destination formats are equal.
    let mut provider_request_body = if is_wire_preserving_responses_hop(
        source_api_format.as_ref(),
        provider_api_format,
        reasoning_replay_policy,
    ) {
        let mut object = body_json.as_object()?.clone();
        object.insert("model".to_string(), Value::String(mapped_model.to_string()));
        Value::Object(object)
    } else {
        convert_request(
            if antigravity_chat_body.is_some() {
                "openai:chat"
            } else {
                source_api_format.as_ref()
            },
            provider_api_format,
            antigravity_chat_body.as_deref().unwrap_or(body_json),
            &format_context,
        )
        .ok()?
    };

    if enable_model_directives {
        apply_model_directive_overrides_from_request(
            &mut provider_request_body,
            provider_api_format,
            mapped_model,
            body_json,
            Some(request_path),
        );
    }

    if !apply_local_body_rules_with_request_headers(
        &mut provider_request_body,
        body_rules,
        Some(body_json),
        request_headers,
    ) {
        return None;
    }
    let client_is_openai_responses_family = matches!(
        aether_ai_formats::normalize_api_format_alias(client_api_format).as_str(),
        "openai:responses" | "openai:responses:compact"
    );
    if client_is_openai_responses_family {
        apply_codex_openai_responses_special_body_edits(
            &mut provider_request_body,
            provider_type,
            provider_api_format,
            body_rules,
            user_api_key_id,
        );
    } else {
        apply_codex_openai_responses_chat_body_edits(
            &mut provider_request_body,
            provider_type,
            provider_api_format,
            body_rules,
            user_api_key_id,
        );
    }
    apply_openai_responses_compact_special_body_edits(
        &mut provider_request_body,
        provider_api_format,
    );
    apply_xai_upstream_payload_edits_with_client(
        &mut provider_request_body,
        provider_type,
        provider_api_format,
        Some(client_api_format),
        Some(body_json),
    );
    crate::formats::openai::responses::strip_incompatible_openai_responses_reasoning_items_with_policy(
        &mut provider_request_body,
        provider_api_format,
        reasoning_replay_policy,
    );
    crate::formats::openai::responses::strip_incompatible_openai_responses_input_item_ids(
        &mut provider_request_body,
        provider_type,
        provider_api_format,
    );
    strip_openai_responses_input_content_cache_control(
        &mut provider_request_body,
        provider_api_format,
    );
    let require_body_stream_field = body_json
        .as_object()
        .is_some_and(|object| object.contains_key("stream"))
        || provider_request_body
            .as_object()
            .is_some_and(|object| object.contains_key("stream"));
    crate::formats::shared::request::enforce_request_body_stream_field(
        &mut provider_request_body,
        provider_api_format,
        upstream_is_stream,
        require_body_stream_field,
    );
    Some(provider_request_body)
}

fn is_wire_preserving_responses_hop(
    source_api_format: &str,
    provider_api_format: &str,
    reasoning_replay_policy: crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy,
) -> bool {
    if !matches!(
        reasoning_replay_policy,
        crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque
            | crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy::XaiEncrypted
    ) {
        return false;
    }
    let source_api_format = aether_ai_formats::normalize_api_format_alias(source_api_format);
    let provider_api_format = aether_ai_formats::normalize_api_format_alias(provider_api_format);
    source_api_format == "openai:responses" && source_api_format == provider_api_format
}

fn compatible_source_format_for_standard_request<'a>(
    body_json: &Value,
    client_api_format: &'a str,
    provider_api_format: &str,
) -> Cow<'a, str> {
    if matches!(
        aether_ai_formats::normalize_api_format_alias(client_api_format).as_str(),
        "openai:chat"
    ) && matches!(
        aether_ai_formats::normalize_api_format_alias(provider_api_format).as_str(),
        "claude:messages"
    ) && is_claude_messages_shaped_body_on_openai_chat_endpoint(body_json)
    {
        return Cow::Borrowed("claude:messages");
    }
    Cow::Borrowed(client_api_format)
}

pub fn build_standard_request_body_from_canonical(
    canonical_request: &Value,
    mapped_model: &str,
    provider_api_format: &str,
    upstream_is_stream: bool,
) -> Option<Value> {
    build_standard_request_body_from_canonical_with_model_directives(
        canonical_request,
        mapped_model,
        provider_api_format,
        upstream_is_stream,
        false,
    )
}

pub fn build_standard_request_body_from_canonical_with_model_directives(
    canonical_request: &Value,
    mapped_model: &str,
    provider_api_format: &str,
    upstream_is_stream: bool,
    enable_model_directives: bool,
) -> Option<Value> {
    let mut provider_request_body =
        match aether_ai_formats::normalize_api_format_alias(provider_api_format).as_str() {
            "openai:chat" => build_local_openai_chat_request_body_with_model_directives(
                canonical_request,
                mapped_model,
                upstream_is_stream,
                enable_model_directives,
            ),
            "openai:responses" => convert_openai_chat_request_to_openai_responses_request(
                canonical_request,
                mapped_model,
                upstream_is_stream,
                false,
            ),
            "openai:responses:compact" => convert_openai_chat_request_to_openai_responses_request(
                canonical_request,
                mapped_model,
                false,
                true,
            ),
            "claude:messages" => convert_openai_chat_request_to_claude_request(
                canonical_request,
                mapped_model,
                upstream_is_stream,
            ),
            "gemini:generate_content" => convert_openai_chat_request_to_gemini_request(
                canonical_request,
                mapped_model,
                upstream_is_stream,
            ),
            _ => None,
        }?;
    if enable_model_directives {
        apply_model_directive_overrides_from_request(
            &mut provider_request_body,
            provider_api_format,
            mapped_model,
            canonical_request,
            None,
        );
    }
    strip_openai_responses_input_content_cache_control(
        &mut provider_request_body,
        provider_api_format,
    );
    Some(provider_request_body)
}

fn strip_openai_responses_input_content_cache_control(
    provider_request_body: &mut Value,
    provider_api_format: &str,
) {
    if !matches!(
        aether_ai_formats::normalize_api_format_alias(provider_api_format).as_str(),
        "openai:responses" | "openai:responses:compact"
    ) {
        return;
    }
    let Some(input) = provider_request_body.get_mut("input") else {
        return;
    };
    strip_responses_input_items_content_cache_control(input);
}

fn strip_responses_input_items_content_cache_control(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                strip_responses_input_items_content_cache_control(item);
            }
        }
        Value::Object(item) => {
            if let Some(content) = item.get_mut("content") {
                strip_responses_content_cache_control(content);
            }
        }
        _ => {}
    }
}

fn strip_responses_content_cache_control(content: &mut Value) {
    match content {
        Value::Array(parts) => {
            for part in parts {
                if let Some(part) = part.as_object_mut() {
                    part.remove("cache_control");
                }
            }
        }
        Value::Object(part) => {
            part.remove("cache_control");
        }
        _ => {}
    }
}

pub fn normalize_standard_request_to_openai_chat_request(
    body_json: &Value,
    client_api_format: &str,
    request_path: &str,
) -> Option<Value> {
    normalize_standard_request_to_openai_chat_request_cow(
        body_json,
        client_api_format,
        request_path,
    )
    .map(Cow::into_owned)
}

fn normalize_standard_request_to_openai_chat_request_cow<'a>(
    body_json: &'a Value,
    client_api_format: &str,
    request_path: &str,
) -> Option<Cow<'a, Value>> {
    match aether_ai_formats::normalize_api_format_alias(client_api_format).as_str() {
        "openai:chat" => Some(Cow::Borrowed(body_json)),
        "openai:responses" | "openai:responses:compact" => {
            normalize_openai_responses_request_to_openai_chat_request(body_json).map(Cow::Owned)
        }
        "claude:messages" => {
            normalize_claude_request_to_openai_chat_request(body_json).map(Cow::Owned)
        }
        "gemini:generate_content" => {
            normalize_gemini_request_to_openai_chat_request(body_json, request_path).map(Cow::Owned)
        }
        _ => None,
    }
}

/// Provider-aware cache-control preparation for Claude Messages -> Gemini on Antigravity.
///
/// Discards Claude prompt-cache breakpoints and ephemeral TTL semantics strictly for
/// Antigravity routes when converting Claude Messages to Gemini GenerateContent.
/// Anthropic prompt caching does not have a lossless equivalent on Gemini, so passing
/// protocol cache markers to the Gemini converter triggers strict conversion refusal.
///
/// This policy strips protocol-level `cache_control` markers before conversion to allow
/// upstream dispatch, without claiming equivalent TTL, breakpoints, cache pricing,
/// or fabricating cache usage.
///
/// Native fallback and non-qualifying provider requests retain original markers.
/// Business fields (tool input, input schemas, document sources) and signatures
/// remain strictly preserved.
pub fn prepare_antigravity_claude_cache_control_body<'a>(
    body: &'a Value,
    client_api_format: &str,
    provider_type: &str,
    provider_api_format: &str,
    provider_config: Option<&Value>,
) -> Cow<'a, Value> {
    // Return borrowed immediately when provider_type is not Antigravity
    // to avoid unnecessary normalization allocations on out-of-scope providers.
    if !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return Cow::Borrowed(body);
    }

    // Config defaults to enabled; explicit false retains strict refusal.
    if let Some(config) = provider_config.and_then(Value::as_object) {
        if matches!(
            config.get("antigravity_cache_control_compat"),
            Some(Value::Bool(false))
        ) {
            return Cow::Borrowed(body);
        }
    }

    // Only Claude Messages source -> Gemini GenerateContent destination qualifies.
    if aether_ai_formats::normalize_api_format_alias(client_api_format) != "claude:messages"
        || aether_ai_formats::normalize_api_format_alias(provider_api_format)
            != "gemini:generate_content"
    {
        return Cow::Borrowed(body);
    }

    // Borrow if no protocol markers present to avoid cloning.
    if !claude_request_has_protocol_cache_control(body) {
        return Cow::Borrowed(body);
    }

    let mut sanitized = body.clone();
    strip_claude_protocol_cache_control(&mut sanitized);
    Cow::Owned(sanitized)
}

fn claude_request_has_protocol_cache_control(body: &Value) -> bool {
    let Some(object) = body.as_object() else {
        return false;
    };
    if object.contains_key("cache_control") {
        return true;
    }
    if let Some(system) = object.get("system") {
        match system {
            Value::Array(blocks) => {
                if blocks.iter().any(|b| {
                    b.as_object()
                        .is_some_and(|o| o.contains_key("cache_control"))
                }) {
                    return true;
                }
            }
            Value::Object(o) => {
                if o.contains_key("cache_control") {
                    return true;
                }
            }
            _ => {}
        }
    }
    if let Some(tools) = object.get("tools").and_then(Value::as_array) {
        if tools.iter().any(|t| {
            t.as_object()
                .is_some_and(|o| o.contains_key("cache_control"))
        }) {
            return true;
        }
    }
    if let Some(messages) = object.get("messages").and_then(Value::as_array) {
        if messages
            .iter()
            .any(claude_message_has_protocol_cache_control)
        {
            return true;
        }
    }
    false
}

fn claude_message_has_protocol_cache_control(message: &Value) -> bool {
    let Some(object) = message.as_object() else {
        return false;
    };
    if object.contains_key("cache_control") {
        return true;
    }
    if let Some(content) = object.get("content") {
        return claude_content_has_protocol_cache_control(content);
    }
    false
}

fn claude_content_has_protocol_cache_control(content: &Value) -> bool {
    match content {
        Value::Array(blocks) => blocks
            .iter()
            .any(claude_content_block_has_protocol_cache_control),
        _ => false,
    }
}

fn claude_content_block_has_protocol_cache_control(block: &Value) -> bool {
    let Some(object) = block.as_object() else {
        return false;
    };
    if object.contains_key("cache_control") {
        return true;
    }
    if object
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("tool_result"))
    {
        // tool_result.content can be an arbitrary JSON Value/object (e.g. {"cache_control": "business"}).
        // Only descend if content is an array of content blocks; never traverse arbitrary objects.
        if let Some(nested_content) = object.get("content").and_then(Value::as_array) {
            return nested_content
                .iter()
                .any(claude_content_block_has_protocol_cache_control);
        }
    }
    false
}

fn strip_claude_protocol_cache_control(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    root.remove("cache_control");

    if let Some(system) = root.get_mut("system") {
        match system {
            Value::Array(blocks) => {
                for block in blocks {
                    if let Some(block_obj) = block.as_object_mut() {
                        block_obj.remove("cache_control");
                    }
                }
            }
            Value::Object(block_obj) => {
                block_obj.remove("cache_control");
            }
            _ => {}
        }
    }

    if let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if let Some(tool_obj) = tool.as_object_mut() {
                tool_obj.remove("cache_control");
            }
        }
    }

    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(message_obj) = message.as_object_mut() {
                message_obj.remove("cache_control");
                if let Some(content) = message_obj.get_mut("content") {
                    strip_claude_content_cache_control(content);
                }
            }
        }
    }
}

fn strip_claude_content_cache_control(content: &mut Value) {
    if let Value::Array(blocks) = content {
        for block in blocks {
            strip_claude_content_block_cache_control(block);
        }
    }
}

fn strip_claude_content_block_cache_control(block: &mut Value) {
    let Some(object) = block.as_object_mut() else {
        return;
    };
    object.remove("cache_control");

    // Only tool_result content arrays are traversed for nested protocol content blocks.
    // An arbitrary JSON Value or object in tool_result.content (e.g. `{"cache_control": "business"}`)
    // is user/tool business data and MUST NOT be traversed or modified.
    // Tool inputs (e.g. `tool_use.input`), thinking signatures, image/document sources, etc.
    // MUST NOT be traversed or modified.
    let is_tool_result = object
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("tool_result"));
    if is_tool_result {
        if let Some(nested_content) = object.get_mut("content").and_then(Value::as_array_mut) {
            for nested_block in nested_content {
                strip_claude_content_block_cache_control(nested_block);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::formats::openai::responses::history::record_converted_response_history;

    use super::{
        build_standard_request_body, build_standard_request_body_from_canonical,
        build_standard_request_body_with_model_directives,
        build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy,
        normalize_standard_request_to_openai_chat_request,
        prepare_antigravity_claude_cache_control_body,
    };
    use crate::formats::openai::responses::OpenAiResponsesReasoningReplayPolicy;
    use serde_json::{json, Value};

    const STANDARD_SURFACES: &[&str] = &[
        "openai:chat",
        "openai:responses",
        "claude:messages",
        "gemini:generate_content",
    ];

    fn sample_request_for(api_format: &str) -> (Value, &'static str) {
        match api_format {
            "openai:chat" => (
                json!({
                    "model": "source-model",
                    "messages": [
                        {"role": "system", "content": "Be concise."},
                        {"role": "user", "content": "Hello matrix"}
                    ],
                    "max_tokens": 32
                }),
                "/v1/chat/completions",
            ),
            "openai:responses" => (
                json!({
                    "model": "source-model",
                    "instructions": "Be concise.",
                    "input": "Hello matrix",
                    "max_output_tokens": 32
                }),
                "/v1/responses",
            ),
            "claude:messages" => (
                json!({
                    "model": "source-model",
                    "system": "Be concise.",
                    "messages": [{
                        "role": "user",
                        "content": [{"type": "text", "text": "Hello matrix"}]
                    }],
                    "max_tokens": 32
                }),
                "/v1/messages",
            ),
            "gemini:generate_content" => (
                json!({
                    "systemInstruction": {
                        "parts": [{"text": "Be concise."}]
                    },
                    "contents": [{
                        "role": "user",
                        "parts": [{"text": "Hello matrix"}]
                    }],
                    "generationConfig": {
                        "maxOutputTokens": 32
                    }
                }),
                "/v1beta/models/source-model:generateContent",
            ),
            other => panic!("unexpected api format: {other}"),
        }
    }

    fn assert_stream_flag(provider_api_format: &str, upstream_is_stream: bool, converted: &Value) {
        match provider_api_format {
            "openai:chat" | "openai:responses" | "claude:messages" => {
                if upstream_is_stream {
                    assert_eq!(
                        converted.get("stream").and_then(Value::as_bool),
                        Some(true),
                        "{provider_api_format} stream flag should be true for upstream streaming"
                    );
                } else {
                    assert!(
                        converted.get("stream").is_none(),
                        "{provider_api_format} should not gain stream:false for ordinary sync requests"
                    );
                }
            }
            "openai:responses:compact" => {
                assert!(
                    converted.get("stream").is_none(),
                    "openai responses compact keeps stream out of the request body"
                );
            }
            "gemini:generate_content" => {
                assert!(
                    converted.get("stream").is_none(),
                    "gemini streaming is represented by endpoint URL, not request body"
                );
            }
            other => panic!("unexpected provider api format: {other}"),
        }
    }

    fn assert_explicit_stream_flag(
        provider_api_format: &str,
        upstream_is_stream: bool,
        converted: &Value,
    ) {
        match provider_api_format {
            "openai:chat" | "openai:responses" | "claude:messages" => {
                assert_eq!(
                    converted.get("stream").and_then(Value::as_bool),
                    Some(upstream_is_stream),
                    "{provider_api_format} stream flag should follow upstream_is_stream"
                );
            }
            "openai:responses:compact" | "gemini:generate_content" => {
                assert!(converted.get("stream").is_none());
            }
            other => panic!("unexpected provider api format: {other}"),
        }
    }

    #[test]
    fn standard_request_body_overrides_client_stream_true_for_non_stream_upstream() {
        let request = json!({
            "model": "source-model",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });

        for provider_api_format in [
            "openai:chat",
            "openai:responses",
            "openai:responses:compact",
            "claude:messages",
            "gemini:generate_content",
        ] {
            let converted = build_standard_request_body(
                &request,
                "openai:chat",
                "mapped-model",
                "custom",
                provider_api_format,
                "/v1/chat/completions",
                false,
                None,
                None,
            )
            .unwrap_or_else(|| panic!("openai:chat -> {provider_api_format} should build"));

            assert_explicit_stream_flag(provider_api_format, false, &converted);
        }
    }

    #[test]
    fn same_responses_family_matrix_preserves_opaque_reasoning_items_and_unknown_fields() {
        let input = (0..66)
            .map(|index| {
                json!({
                    "type": "reasoning",
                    "encrypted_content": format!("opaque-state-{index}"),
                    "content": [{
                        "type": "reasoning_text",
                        "text": format!("thinking {index}")
                    }],
                    "future_capability": {"budget_class": "dynamic"}
                })
            })
            .collect::<Vec<_>>();
        let request = json!({
            "model": "deepseek-v4-flash",
            "input": input,
            "future_request_field": {"preserve": true}
        });

        let preserved = build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
            &request,
            "openai:responses",
            "deepseek-v4-flash",
            "custom",
            "openai:responses",
            "/v1/responses",
            false,
            None,
            None,
            None,
            false,
            OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
        )
        .expect("same-family Responses body should build");
        assert_eq!(preserved["model"], "deepseek-v4-flash");
        assert_eq!(preserved["future_request_field"]["preserve"], true);
        assert_eq!(
            preserved["input"]
                .as_array()
                .expect("input array")
                .iter()
                .filter(|item| item["type"] == "reasoning")
                .count(),
            66
        );
        assert_eq!(
            preserved["input"][0]["future_capability"]["budget_class"],
            "dynamic"
        );

        let strict = build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
            &request,
            "openai:responses",
            "deepseek-v4-flash",
            "openai",
            "openai:responses",
            "/v1/responses",
            false,
            None,
            None,
            None,
            false,
            OpenAiResponsesReasoningReplayPolicy::OpenAiItemIds,
        )
        .expect("strict same-family Responses body should build");
        assert_eq!(
            strict["input"]
                .as_array()
                .expect("strict input array")
                .iter()
                .filter(|item| item["type"] == "reasoning")
                .count(),
            0
        );

        let compact = build_standard_request_body_with_model_directives_and_request_headers_and_reasoning_replay_policy(
            &request,
            "openai:responses:compact",
            "deepseek-v4-flash",
            "custom",
            "openai:responses:compact",
            "/v1/responses/compact",
            false,
            None,
            None,
            None,
            false,
            OpenAiResponsesReasoningReplayPolicy::DeepSeekOpaque,
        )
        .expect("Compact body should continue through canonical normalization");
        assert_eq!(
            compact["input"]
                .as_array()
                .expect("compact input array")
                .iter()
                .filter(|item| item["type"] == "reasoning")
                .count(),
            0,
            "compact must not use the DeepSeek opaque replay policy"
        );
    }

    #[test]
    fn standard_request_body_scopes_previous_response_history_by_api_key() {
        record_converted_response_history(
            &json!({
                "needs_conversion": true,
                "client_api_format": "openai:responses",
                "provider_api_format": "openai:chat",
                "api_key_id": "standard-history-key-a",
                "original_request_body": {
                    "model": "source-model",
                    "input": [{"role": "user", "content": "inspect the repository"}]
                }
            }),
            &json!({
                "id": "resp_standard_history_scope_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_standard_history_scope_1",
                    "call_id": "call_standard_history_scope_1",
                    "name": "inspect_repository",
                    "arguments": "{}"
                }]
            }),
        );
        let continuation = json!({
            "model": "source-model",
            "previous_response_id": "resp_standard_history_scope_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_standard_history_scope_1",
                "output": "inspection-complete"
            }]
        });

        let owner = build_standard_request_body(
            &continuation,
            "openai:responses",
            "mapped-model",
            "custom",
            "openai:chat",
            "/v1/responses",
            false,
            None,
            Some("standard-history-key-a"),
        )
        .expect("the owning API key should restore response history");
        assert_eq!(
            owner["messages"][1]["tool_calls"][0]["id"],
            "call_standard_history_scope_1"
        );
        assert_eq!(
            owner["messages"][2]["tool_call_id"],
            "call_standard_history_scope_1"
        );

        assert!(build_standard_request_body(
            &continuation,
            "openai:responses",
            "mapped-model",
            "custom",
            "openai:chat",
            "/v1/responses",
            false,
            None,
            Some("standard-history-key-b"),
        )
        .is_none());
    }

    #[test]
    fn standard_request_body_stream_policy_wins_after_body_rules() {
        let request = json!({
            "model": "source-model",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });
        let body_rules = json!([
            {"action":"set","path":"stream","value":true}
        ]);

        for provider_api_format in [
            "openai:chat",
            "openai:responses",
            "openai:responses:compact",
            "claude:messages",
            "gemini:generate_content",
        ] {
            let converted = build_standard_request_body(
                &request,
                "openai:chat",
                "mapped-model",
                "custom",
                provider_api_format,
                "/v1/chat/completions",
                false,
                Some(&body_rules),
                None,
            )
            .unwrap_or_else(|| panic!("openai:chat -> {provider_api_format} should build"));

            assert_explicit_stream_flag(provider_api_format, false, &converted);
        }
    }

    #[test]
    fn standard_openai_chat_to_claude_accepts_claude_native_body_from_chat_endpoint() {
        let request = json!({
            "model": "deepseek-v4-flash",
            "messages": [
                {"role": "user", "content": "lookup"},
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "checking"},
                        {
                            "type": "tool_use",
                            "id": "call_1",
                            "name": "lookup",
                            "input": {"q": "db"}
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": {"rows": 1},
                        "is_error": false
                    }]
                }
            ],
            "tools": [{
                "name": "lookup",
                "description": "Lookup data",
                "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}}
            }],
            "tool_choice": {"type": "auto"},
            "max_tokens": 128,
            "stream": true
        });

        let converted = build_standard_request_body(
            &request,
            "openai:chat",
            "claude-sonnet-4-5",
            "custom",
            "claude:messages",
            "/v1/chat/completions",
            true,
            None,
            None,
        )
        .expect("claude-native chat endpoint body should build as claude messages");

        assert_eq!(converted["model"], "claude-sonnet-4-5");
        assert_eq!(converted["max_tokens"], 128);
        assert_eq!(converted["tools"][0]["name"], "lookup");
        assert_eq!(
            converted["tools"][0]["input_schema"]["properties"]["q"]["type"],
            "string"
        );
        assert_eq!(converted["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(converted["messages"][1]["content"][1]["id"], "call_1");
        assert_eq!(
            converted["messages"][2]["content"][0]["type"],
            "tool_result"
        );
        assert_eq!(
            converted["messages"][2]["content"][0]["tool_use_id"],
            "call_1"
        );
        assert_eq!(
            serde_json::from_str::<Value>(
                converted["messages"][2]["content"][0]["content"]
                    .as_str()
                    .expect("object tool result content should be serialized for Claude")
            )
            .expect("serialized tool result content should remain JSON"),
            json!({"rows": 1})
        );
        assert_eq!(converted["tool_choice"]["type"], "auto");
        assert_eq!(converted["stream"], true);
    }

    #[test]
    fn standard_openai_chat_to_claude_normalizes_multiturn_tool_history() {
        let request = json!({
            "model": "deepseek-v4-flash",
            "messages": [
                {"role": "system", "content": "Be precise."},
                {"role": "user", "content": "check two things"},
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "weather-1",
                            "type": "function",
                            "function": {"name": "get_weather", "arguments": "{\"city\":\"NYC\"}"}
                        },
                        {
                            "id": "call_2",
                            "type": "function",
                            "function": {"name": "lookup", "arguments": "{\"q\":\"db\"}"}
                        }
                    ]
                },
                {"role": "tool", "tool_call_id": "weather-1", "content": ""},
                {"role": "tool", "tool_call_id": "call_2", "content": [{"type": "text", "text": "rows=1"}]},
                {"role": "user", "content": "now answer"}
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {"name": "get_weather", "description": "Get weather", "parameters": null}
                },
                {
                    "type": "function",
                    "function": {
                        "name": "lookup",
                        "description": "Lookup data",
                        "parameters": {"properties": {"q": {"type": "string"}}}
                    }
                }
            ],
            "parallel_tool_calls": true,
            "max_tokens": 128,
            "stream": true
        });

        let converted = build_standard_request_body(
            &request,
            "openai:chat",
            "claude-sonnet-4-5",
            "custom",
            "claude:messages",
            "/v1/chat/completions",
            true,
            None,
            None,
        )
        .expect("openai chat tool history should build as claude messages");

        assert_eq!(converted["model"], "claude-sonnet-4-5");
        assert_eq!(converted["system"], "Be precise.");
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(converted["messages"][1]["role"], "assistant");
        assert_eq!(converted["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(
            converted["messages"][1]["content"][0]["id"],
            "toolu_weather-1"
        );
        assert_eq!(converted["messages"][1]["content"][1]["id"], "call_2");
        assert_eq!(converted["messages"][2]["role"], "user");
        assert_eq!(
            converted["messages"][2]["content"][0]["type"],
            "tool_result"
        );
        assert_eq!(
            converted["messages"][2]["content"][0]["tool_use_id"],
            "toolu_weather-1"
        );
        assert_eq!(converted["messages"][2]["content"][0]["content"], "(empty)");
        assert_eq!(
            converted["messages"][2]["content"][1]["tool_use_id"],
            "call_2"
        );
        assert_eq!(converted["messages"][2]["content"][1]["content"], "rows=1");
        assert_eq!(converted["messages"][2]["content"][2]["type"], "text");
        assert_eq!(converted["messages"][2]["content"][2]["text"], "now answer");
        assert_eq!(converted["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(
            converted["tools"][0]["input_schema"]["properties"],
            json!({})
        );
        assert_eq!(converted["tools"][1]["input_schema"]["type"], "object");
        assert_eq!(converted["stream"], true);
    }

    fn codex_default_body_rules() -> Value {
        json!([
            {"action":"drop","path":"max_output_tokens"},
            {"action":"drop","path":"temperature"},
            {"action":"drop","path":"top_p"},
            {"action":"set","path":"store","value":false},
            {
                "action":"set",
                "path":"instructions",
                "value":"You are GPT-5.",
                "condition":{"path":"instructions","op":"not_exists"}
            }
        ])
    }

    fn legacy_openai_responses_request_body(
        request: &Value,
        provider_api_format: &str,
        upstream_is_stream: bool,
    ) -> Value {
        let chat_canonical = normalize_standard_request_to_openai_chat_request(
            request,
            "openai:responses",
            "/v1/responses",
        )
        .expect("legacy openai responses normalization should succeed");
        build_standard_request_body_from_canonical(
            &chat_canonical,
            "mapped-model",
            provider_api_format,
            upstream_is_stream,
        )
        .expect("legacy openai responses target conversion should succeed")
    }

    fn legacy_openai_chat_request_body(
        request: &Value,
        provider_api_format: &str,
        upstream_is_stream: bool,
    ) -> Value {
        build_standard_request_body_from_canonical(
            request,
            "mapped-model",
            provider_api_format,
            upstream_is_stream,
        )
        .expect("legacy openai chat target conversion should succeed")
    }

    fn legacy_claude_request_body(
        request: &Value,
        provider_api_format: &str,
        upstream_is_stream: bool,
    ) -> Value {
        let chat_canonical = normalize_standard_request_to_openai_chat_request(
            request,
            "claude:messages",
            "/v1/messages",
        )
        .expect("legacy claude normalization should succeed");
        build_standard_request_body_from_canonical(
            &chat_canonical,
            "mapped-model",
            provider_api_format,
            upstream_is_stream,
        )
        .expect("legacy claude target conversion should succeed")
    }

    fn legacy_gemini_request_body(
        request: &Value,
        provider_api_format: &str,
        upstream_is_stream: bool,
    ) -> Value {
        let chat_canonical = normalize_standard_request_to_openai_chat_request(
            request,
            "gemini:generate_content",
            "/v1beta/models/source-model:generateContent",
        )
        .expect("legacy gemini normalization should succeed");
        build_standard_request_body_from_canonical(
            &chat_canonical,
            "mapped-model",
            provider_api_format,
            upstream_is_stream,
        )
        .expect("legacy gemini target conversion should succeed")
    }

    #[test]
    fn builds_request_body_for_all_standard_surface_pairs_in_sync_and_stream_modes() {
        for client_api_format in STANDARD_SURFACES {
            let (request, request_path) = sample_request_for(client_api_format);
            for provider_api_format in STANDARD_SURFACES {
                for upstream_is_stream in [false, true] {
                    let converted = build_standard_request_body(
                        &request,
                        client_api_format,
                        "mapped-model",
                        "custom",
                        provider_api_format,
                        request_path,
                        upstream_is_stream,
                        None,
                        None,
                    )
                    .unwrap_or_else(|| {
                        panic!(
                            "{client_api_format} -> {provider_api_format} should build with upstream_is_stream={upstream_is_stream}"
                        )
                    });

                    assert_stream_flag(provider_api_format, upstream_is_stream, &converted);
                    assert!(
                        converted.to_string().contains("Hello matrix"),
                        "{client_api_format} -> {provider_api_format} should retain user content"
                    );
                }
            }
        }
    }

    #[test]
    fn openai_responses_request_uses_typed_canonical_without_changing_target_payloads() {
        let request = json!({
            "model": "gpt-5",
            "instructions": "Be exact.",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Inspect this"},
                        {
                            "type": "input_image",
                            "image_url": "data:image/png;base64,iVBORw0KGgo=",
                            "detail": "high"
                        },
                        {
                            "type": "input_file",
                            "file_data": "data:application/pdf;base64,JVBERi0x",
                            "filename": "spec.pdf"
                        }
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_123",
                    "name": "lookup",
                    "arguments": "{\"q\":\"rust\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_123",
                    "output": "{\"ok\":true}"
                }
            ],
            "max_output_tokens": 64,
            "temperature": 0.2,
            "top_p": 0.9,
            "parallel_tool_calls": true,
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "Lookup data",
                "parameters": {"type": "object"}
            }],
            "tool_choice": {"type": "function", "name": "lookup"},
            "reasoning": {"effort": "high"},
            "text": {
                "format": {
                    "type": "json_schema",
                    "json_schema": {"name": "answer", "schema": {"type": "object"}}
                },
                "verbosity": "low"
            },
            "metadata": {"trace": "abc"}
        });

        // verbosity 仅在 OpenAI 系与 Chat 目标有已审计映射；Claude/Gemini
        // 目标按 fail-closed 语义拒绝，typed 路由不再静默丢弃。
        for provider_api_format in [
            "openai:chat",
            "openai:responses",
            "openai:responses:compact",
        ] {
            for upstream_is_stream in [false, true] {
                let converted = build_standard_request_body(
                    &request,
                    "openai:responses",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1/responses",
                    upstream_is_stream,
                    None,
                    None,
                )
                .expect("typed canonical route should build");
                let legacy = legacy_openai_responses_request_body(
                    &request,
                    provider_api_format,
                    upstream_is_stream,
                );
                assert_eq!(
                    converted, legacy,
                    "typed canonical openai:responses -> {provider_api_format} changed payload with upstream_is_stream={upstream_is_stream}"
                );
            }
        }
        for provider_api_format in ["claude:messages", "gemini:generate_content"] {
            assert!(
                build_standard_request_body(
                    &request,
                    "openai:responses",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1/responses",
                    false,
                    None,
                    None,
                )
                .is_none(),
                "typed canonical openai:responses -> {provider_api_format} must fail closed on unmapped verbosity"
            );
        }
    }

    #[test]
    fn standard_request_body_applies_reasoning_effort_suffix_to_claude_target() {
        let request = json!({
            "model": "gpt-5.4-max",
            "messages": [{"role": "user", "content": "Need high effort"}],
            "reasoning_effort": "low"
        });

        let converted = build_standard_request_body_with_model_directives(
            &request,
            "openai:chat",
            "claude-sonnet-4-5",
            "anthropic",
            "claude:messages",
            "/v1/chat/completions",
            false,
            None,
            None,
            true,
        )
        .expect("openai chat should convert to claude chat");

        assert_eq!(converted["model"], "claude-sonnet-4-5");
        assert_eq!(converted["output_config"]["effort"], "max");
        assert_eq!(converted["thinking"]["budget_tokens"], 8192);
    }

    #[test]
    fn standard_request_body_applies_reasoning_effort_suffix_from_gemini_path() {
        let request = json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": "Need high effort"}]
            }]
        });

        let converted = build_standard_request_body_with_model_directives(
            &request,
            "gemini:generate_content",
            "gpt-5.4",
            "openai",
            "openai:chat",
            "/v1beta/models/gemini-2.5-pro-high:generateContent",
            false,
            None,
            None,
            true,
        )
        .expect("gemini should convert to openai chat");

        assert_eq!(converted["model"], "gpt-5.4");
        assert_eq!(converted["reasoning_effort"], "high");
    }

    #[test]
    fn openai_chat_request_uses_typed_canonical_without_changing_target_payloads() {
        let request = json!({
            "model": "gpt-5",
            "messages": [
                {"role": "system", "content": "Be exact."},
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Inspect this"},
                        {
                            "type": "image_url",
                            "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}
                        }
                    ]
                },
                {
                    "role": "assistant",
                    "content": null,
                    "reasoning_parts": [{
                        "type": "thinking",
                        "thinking": "plan",
                        "signature": "sig_123"
                    }],
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "lookup",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_123",
                    "content": {"ok": true}
                }
            ],
            "max_completion_tokens": 64,
            "temperature": 0.2,
            "tools": [{
                "type": "function",
                "function": {
                    "name": "lookup",
                    "description": "Lookup data",
                    "parameters": {"type": "object"}
                }
            }],
            "tool_choice": {"type": "function", "function": {"name": "lookup"}},
            "reasoning_effort": "medium",
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "answer", "schema": {"type": "object"}}
            }
        });

        for provider_api_format in STANDARD_SURFACES {
            for upstream_is_stream in [false, true] {
                let converted = build_standard_request_body(
                    &request,
                    "openai:chat",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1/chat/completions",
                    upstream_is_stream,
                    None,
                    None,
                )
                .expect("typed canonical openai chat route should build");
                let legacy = legacy_openai_chat_request_body(
                    &request,
                    provider_api_format,
                    upstream_is_stream,
                );
                assert_eq!(
                    converted, legacy,
                    "typed canonical openai:chat -> {provider_api_format} changed payload with upstream_is_stream={upstream_is_stream}"
                );
            }
        }
    }

    /// 验证 Claude 请求经规范模型投影到各目标时保持既有载荷，并校验 Gemini 工具结果字符串语义。
    #[test]
    fn claude_request_uses_typed_canonical_without_changing_non_claude_target_payloads() {
        let request = json!({
            "model": "claude-sonnet-4-5",
            "system": "Be exact.",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Inspect this"},
                        {
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": "image/png",
                                "data": "iVBORw0KGgo="
                            }
                        },
                        {
                            "type": "document",
                            "source": {
                                "type": "base64",
                                "media_type": "application/pdf",
                                "data": "JVBERi0x"
                            }
                        }
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "thinking",
                            "thinking": "plan",
                            "signature": "sig_123"
                        },
                        {
                            "type": "tool_use",
                            "id": "toolu_123",
                            "name": "lookup",
                            "input": {"q": "rust"}
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "toolu_123",
                        "content": {"ok": true}
                    }]
                }
            ],
            "max_tokens": 64,
            "temperature": 0.2,
            "top_p": 0.9,
            "tools": [{
                "name": "lookup",
                "description": "Lookup data",
                "input_schema": {"type": "object"}
            }],
            "tool_choice": {
                "type": "tool",
                "name": "lookup",
                "disable_parallel_tool_use": false
            },
            "metadata": {"trace": "abc"},
            "thinking": {"type": "enabled", "budget_tokens": 2048}
        });

        // Claude thinking 块到 Responses 目标保持 fail closed：Claude signature 与
        // OpenAI encrypted reasoning 是不同协议语义，不做隐式互换；Chat/Gemini
        // 目标继续走已审计映射并保持与 legacy 输出一致。
        for provider_api_format in ["openai:chat", "gemini:generate_content"] {
            for upstream_is_stream in [false, true] {
                let converted = build_standard_request_body(
                    &request,
                    "claude:messages",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1/messages",
                    upstream_is_stream,
                    None,
                    None,
                )
                .expect("typed canonical claude route should build");
                if provider_api_format == "gemini:generate_content" {
                    assert_eq!(
                        converted["contents"][2]["parts"][0]["functionResponse"]["response"]
                            ["result"],
                        json!({"ok": true})
                    );
                    continue;
                }
                let legacy =
                    legacy_claude_request_body(&request, provider_api_format, upstream_is_stream);
                assert_eq!(
                    converted, legacy,
                    "typed canonical claude:messages -> {provider_api_format} changed payload with upstream_is_stream={upstream_is_stream}"
                );
            }
        }
        for provider_api_format in ["openai:responses", "openai:responses:compact"] {
            assert!(
                build_standard_request_body(
                    &request,
                    "claude:messages",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1/messages",
                    false,
                    None,
                    None,
                )
                .is_none(),
                "typed canonical claude:messages -> {provider_api_format} must fail closed on Claude thinking blocks"
            );
        }
    }

    #[test]
    fn gemini_request_uses_typed_canonical_without_changing_non_gemini_target_payloads() {
        let request = json!({
            "systemInstruction": {
                "parts": [{"text": "Be exact."}]
            },
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        {"text": "Inspect this"},
                        {"inlineData": {"mimeType": "image/png", "data": "iVBORw0KGgo="}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"text": "plan", "thought": true, "thoughtSignature": "sig_123"},
                        {"functionCall": {"id": "call_123", "name": "lookup", "args": {"q": "rust"}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "id": "call_123",
                            "name": "lookup",
                            "response": {"result": {"ok": true}}
                        }
                    }]
                }
            ],
            "generationConfig": {
                "maxOutputTokens": 64,
                "temperature": 0.2,
                "thinkingConfig": {"includeThoughts": true, "thinkingBudget": 2048}
            },
            "tools": [{
                "functionDeclarations": [{
                    "name": "lookup",
                    "description": "Lookup data",
                    "parameters": {"type": "object"}
                }]
            }],
            "toolConfig": {
                "functionCallingConfig": {
                    "mode": "ANY",
                    "allowedFunctionNames": ["lookup"]
                }
            }
        });

        // Gemini thought parts 到 Responses 目标保持 fail closed（thoughtSignature
        // 与 OpenAI encrypted reasoning 协议语义不同）；Chat 走已审计映射并保持
        // 与 legacy 输出一致。Claude 目标 typed 直转可构建，但 legacy 的
        // chat 中间形态会以扩展字段携带 thoughtSignature 侧车，按 fail-closed
        // 语义被拒绝，因此该面不再做 legacy 等价对比。
        for upstream_is_stream in [false, true] {
            let converted = build_standard_request_body(
                &request,
                "gemini:generate_content",
                "mapped-model",
                "custom",
                "openai:chat",
                "/v1beta/models/source-model:generateContent",
                upstream_is_stream,
                None,
                None,
            )
            .expect("typed canonical gemini route should build");
            let legacy = legacy_gemini_request_body(&request, "openai:chat", upstream_is_stream);
            assert_eq!(
                converted, legacy,
                "typed canonical gemini:generate_content -> openai:chat changed payload with upstream_is_stream={upstream_is_stream}"
            );
        }
        for upstream_is_stream in [false, true] {
            let converted = build_standard_request_body(
                &request,
                "gemini:generate_content",
                "mapped-model",
                "custom",
                "claude:messages",
                "/v1beta/models/source-model:generateContent",
                upstream_is_stream,
                None,
                None,
            )
            .expect("typed canonical gemini route should build for claude");
            assert_eq!(converted["model"], "mapped-model");
            assert_eq!(converted["messages"][1]["content"][0]["type"], "thinking");
            assert_eq!(converted["messages"][1]["content"][1]["type"], "tool_use");
        }
        for provider_api_format in ["openai:responses", "openai:responses:compact"] {
            assert!(
                build_standard_request_body(
                    &request,
                    "gemini:generate_content",
                    "mapped-model",
                    "custom",
                    provider_api_format,
                    "/v1beta/models/source-model:generateContent",
                    false,
                    None,
                    None,
                )
                .is_none(),
                "typed canonical gemini:generate_content -> {provider_api_format} must fail closed on Gemini thought parts"
            );
        }
    }

    #[test]
    fn applies_codex_body_rules_for_all_standard_sources_to_openai_responses() {
        let body_rules = codex_default_body_rules();

        for client_api_format in STANDARD_SURFACES {
            let (mut request, request_path) = sample_request_for(client_api_format);
            // 按各格式的原生字段位置注入采样参数：Gemini 的 temperature/top_p
            // 属于 generationConfig，根级键不在其审计 schema 内。
            if *client_api_format == "gemini:generate_content" {
                let generation_config = request
                    .as_object_mut()
                    .and_then(|object| object.get_mut("generationConfig"))
                    .and_then(Value::as_object_mut)
                    .expect("gemini sample request has generationConfig");
                generation_config.insert("temperature".to_string(), json!(0.7));
                generation_config.insert("topP".to_string(), json!(0.8));
            } else if let Some(object) = request.as_object_mut() {
                object.insert("temperature".to_string(), json!(0.7));
                object.insert("top_p".to_string(), json!(0.8));
            }

            let converted = build_standard_request_body(
                &request,
                client_api_format,
                "gpt-5.5",
                "codex",
                "openai:responses",
                request_path,
                true,
                Some(&body_rules),
                Some("key-1"),
            )
            .unwrap_or_else(|| {
                panic!("{client_api_format} -> openai:responses should build with codex body rules")
            });

            assert_eq!(converted["model"], "gpt-5.5");
            assert_eq!(converted["stream"], true);
            assert_eq!(converted["store"], false);
            assert!(converted.get("max_output_tokens").is_none());
            assert!(converted.get("temperature").is_none());
            assert!(converted.get("top_p").is_none());
            assert!(
                converted.get("instructions").is_some(),
                "{client_api_format} -> openai:responses should keep or inject instructions"
            );
        }
    }

    #[test]
    fn codex_store_body_rule_preserves_response_input_item_ids() {
        let request = json!({
            "model": "gpt-5.4",
            "input": [{
                "id": "msg-1",
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            }]
        });
        let body_rules = json!([{"action":"set","path":"store","value":true}]);

        let converted = build_standard_request_body(
            &request,
            "openai:responses",
            "gpt-5.4",
            "codex",
            "openai:responses",
            "/v1/responses",
            true,
            Some(&body_rules),
            Some("key-1"),
        )
        .expect("Codex Responses request should apply the store body rule");

        assert_eq!(converted["store"], true);
        assert_eq!(converted["input"][0]["id"], "msg-1");
    }

    #[test]
    fn standard_openai_responses_strips_content_cache_control_after_body_rules() {
        let request = json!({
            "model": "gpt-5.1",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            }],
            "prompt_cache_key": "cache_123"
        });
        let body_rules = json!([
            {
                "action": "set",
                "path": "input[0].content[0].cache_control",
                "value": {"type": "ephemeral"}
            }
        ]);

        let converted = build_standard_request_body(
            &request,
            "openai:responses",
            "gpt-5.1",
            "openai",
            "openai:responses",
            "/v1/responses",
            false,
            Some(&body_rules),
            None,
        )
        .expect("responses request should build");

        assert_eq!(converted["prompt_cache_key"], "cache_123");
        assert!(!converted["input"].to_string().contains("cache_control"));
    }

    #[test]
    fn standard_codex_responses_rejects_claude_cache_control_without_bridge_projection() {
        // Claude 原生 cache_control 跨格式没有无损等价物：pure/runtime 统一
        // fail closed，不再依赖 codex overlay 事后剥离。需要保留 cache 语义的
        // 流程必须先走显式 bridge sanitize（投影为 prompt cache breakpoint）。
        // Responses 源 cache_control 的 overlay 剥离仍由
        // standard_openai_responses_strips_content_cache_control_after_body_rules 覆盖。
        fn claude_request(user_text: &str) -> Value {
            json!({
                "model": "claude-sonnet",
                "system": [{
                    "type": "text",
                    "text": "stable system brief",
                    "cache_control": {"type": "ephemeral"}
                }],
                "messages": [{
                    "role": "user",
                    "content": [{"type": "text", "text": user_text}]
                }],
                "max_tokens": 128
            })
        }

        for user_text in ["new turn A", "new turn B"] {
            assert!(
                build_standard_request_body(
                    &claude_request(user_text),
                    "claude:messages",
                    "gpt-5.4",
                    "codex",
                    "openai:responses",
                    "/v1/messages",
                    true,
                    None,
                    Some("key-a"),
                )
                .is_none(),
                "claude cache_control must fail closed for codex responses without bridge sanitize"
            );
        }

        // 无 cache_control 的同形请求仍可正常构建。
        let clean = json!({
            "model": "claude-sonnet",
            "system": [{"type": "text", "text": "stable system brief"}],
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "new turn A"}]
            }],
            "max_tokens": 128
        });
        let converted = build_standard_request_body(
            &clean,
            "claude:messages",
            "gpt-5.4",
            "codex",
            "openai:responses",
            "/v1/messages",
            true,
            None,
            Some("key-a"),
        )
        .expect("clean claude to codex responses request should build");
        assert!(converted.get("prompt_cache_key").is_none());
        assert!(!converted.to_string().contains("cache_control"));
    }

    #[test]
    fn builds_openai_chat_request_from_claude_chat_source() {
        let request = json!({
            "model": "claude-3-7-sonnet",
            "system": "You are concise.",
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello from Claude"}]
                }
            ],
            "max_tokens": 128
        });

        let converted = build_standard_request_body(
            &request,
            "claude:messages",
            "gpt-5",
            "openai",
            "openai:chat",
            "/v1/messages",
            false,
            None,
            None,
        )
        .expect("claude chat should convert to openai chat");

        assert_eq!(converted["model"], "gpt-5");
        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][0]["content"], "You are concise.");
        assert_eq!(converted["messages"][1]["role"], "user");
        assert_eq!(converted["messages"][1]["content"], "Hello from Claude");
    }

    #[test]
    fn builds_streaming_openai_chat_request_from_gemini_chat_source_with_include_usage() {
        let request = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [{"text": "Hello from Gemini"}]
                }
            ]
        });

        let converted = build_standard_request_body(
            &request,
            "gemini:generate_content",
            "gpt-5",
            "openai",
            "openai:chat",
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
            true,
            None,
            None,
        )
        .expect("gemini chat stream should convert to openai chat");

        assert_eq!(converted["model"], "gpt-5");
        assert_eq!(converted["stream"], true);
        assert_eq!(converted["stream_options"]["include_usage"], true);
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(converted["messages"][0]["content"], "Hello from Gemini");
    }

    #[test]
    fn builds_claude_chat_request_from_gemini_chat_source() {
        let request = json!({
            "systemInstruction": {
                "parts": [{"text": "Be brief."}]
            },
            "contents": [
                {
                    "role": "user",
                    "parts": [{"text": "Hello from Gemini"}]
                }
            ]
        });

        let converted = build_standard_request_body(
            &request,
            "gemini:generate_content",
            "claude-sonnet-4-5",
            "anthropic",
            "claude:messages",
            "/v1beta/models/gemini-2.5-pro:generateContent",
            false,
            None,
            None,
        )
        .expect("gemini chat should convert to claude chat");

        assert_eq!(converted["model"], "claude-sonnet-4-5");
        assert_eq!(converted["messages"][0]["role"], "user");
        assert!(
            converted["messages"]
                .to_string()
                .contains("Hello from Gemini"),
            "converted claude payload should retain the gemini user text: {converted}"
        );
    }

    /// 验证 Claude CLI 请求使用 Gemini 3 目标时保持基础消息内容。
    #[test]
    fn builds_gemini_cli_request_from_claude_cli_source() {
        let request = json!({
            "model": "claude-sonnet-4-5",
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Need CLI output"}]
                }
            ],
            "max_tokens": 64
        });

        let converted = build_standard_request_body(
            &request,
            "claude:messages",
            "gemini-3-flash-preview",
            "google",
            "gemini:generate_content",
            "/v1/messages",
            false,
            None,
            None,
        )
        .expect("claude cli should convert to gemini cli");

        assert_eq!(converted["contents"][0]["role"], "user");
        assert_eq!(
            converted["contents"][0]["parts"][0]["text"],
            "Need CLI output"
        );
    }

    #[test]
    fn builds_openai_chat_request_from_openai_responses_source_with_chat_shape() {
        let request = json!({
            "model": "gpt-5",
            "instructions": "You are concise.",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    {
                        "type": "input_image",
                        "image_url": "https://example.com/cat.png",
                        "detail": "high"
                    },
                    {
                        "type": "input_file",
                        "file_data": "data:application/pdf;base64,JVBERi0x",
                        "filename": "spec.pdf"
                    },
                    {"type": "input_text", "text": "Summarize this"}
                ]
            }],
            "reasoning": {"effort": "high"},
            "text": {
                "format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "answer_schema",
                        "schema": {
                            "type": "object",
                            "properties": {"answer": {"type": "string"}}
                        }
                    }
                }
            }
        });

        let converted = build_standard_request_body(
            &request,
            "openai:responses",
            "gpt-5",
            "openai",
            "openai:chat",
            "/v1/responses",
            false,
            None,
            None,
        )
        .expect("responses request should convert to chat completions");

        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][0]["content"], "You are concise.");
        assert_eq!(converted["reasoning_effort"], "high");
        assert_eq!(
            converted["response_format"]["json_schema"]["name"],
            "answer_schema"
        );
        assert_eq!(converted["messages"][1]["content"][0]["type"], "image_url");
        assert_eq!(
            converted["messages"][1]["content"][0]["image_url"]["url"],
            "https://example.com/cat.png"
        );
        assert_eq!(
            converted["messages"][1]["content"][0]["image_url"]["detail"],
            "high"
        );
        assert_eq!(converted["messages"][1]["content"][1]["type"], "file");
        assert_eq!(
            converted["messages"][1]["content"][1]["file"]["filename"],
            "spec.pdf"
        );
    }

    #[test]
    fn builds_gemini_request_from_openai_chat_with_structured_output_and_images() {
        let request = json!({
            "model": "gpt-5",
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,iVBORw0KGgo="
                        }
                    },
                    {"type": "text", "text": "Describe it"}
                ]
            }],
            "reasoning_effort": "medium",
            "n": 2,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "answer_schema",
                    "schema": {
                        "type": "object",
                        "properties": {"answer": {"type": "string"}}
                    }
                }
            },
            "web_search_options": {
                "search_context_size": "high"
            }
        });

        let converted = build_standard_request_body(
            &request,
            "openai:chat",
            "gemini-2.5-pro",
            "google",
            "gemini:generate_content",
            "/v1/chat/completions",
            false,
            None,
            None,
        )
        .expect("openai chat should convert to gemini");

        assert_eq!(
            converted["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            2048
        );
        assert_eq!(converted["generationConfig"]["candidateCount"], 2);
        assert_eq!(
            converted["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            converted["generationConfig"]["responseSchema"]["type"],
            "object"
        );
        assert_eq!(
            converted["contents"][0]["parts"][0]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(converted["tools"][0]["googleSearch"], json!({}));
    }

    #[test]
    fn claude_client_web_search_tool_survives_conversion_to_gemini() {
        let request = json!({
            "model": "gemini-3-flash-preview",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": "find the release notes"}],
            "tools": [
                {
                    "name": "WebSearch",
                    "description": "Search the web and use the results to inform responses",
                    "input_schema": {
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                        "required": ["query"]
                    }
                },
                {
                    "name": "Read",
                    "description": "Read a file",
                    "input_schema": {
                        "type": "object",
                        "properties": {"file_path": {"type": "string"}},
                        "required": ["file_path"]
                    }
                }
            ]
        });

        let converted = build_standard_request_body(
            &request,
            "claude:messages",
            "gemini-3-flash-preview",
            "google",
            "gemini:generate_content",
            "/v1/messages",
            false,
            None,
            None,
        )
        .expect("claude messages should convert to gemini");

        let tools = converted["tools"]
            .as_array()
            .expect("tools should be an array");
        assert!(
            tools.iter().all(|tool| tool.get("googleSearch").is_none()
                && tool.get("googleSearchRetrieval").is_none()),
            "a client-declared WebSearch tool must not become server-side grounding: {tools:?}"
        );

        let declared: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool.get("functionDeclarations"))
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|declaration| declaration.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(declared, vec!["WebSearch", "Read"], "{tools:?}");
    }

    #[test]
    fn builds_claude_request_from_openai_chat_with_thinking_and_data_url_image() {
        let request = json!({
            "model": "gpt-5",
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/jpeg;base64,/9j/4AAQSk"
                        }
                    },
                    {"type": "text", "text": "What is this?"}
                ]
            }],
            "reasoning_effort": "low"
        });

        let converted = build_standard_request_body(
            &request,
            "openai:chat",
            "claude-sonnet-4-5",
            "anthropic",
            "claude:messages",
            "/v1/chat/completions",
            false,
            None,
            None,
        )
        .expect("openai chat should convert to claude");

        assert_eq!(converted["thinking"]["type"], "enabled");
        assert_eq!(converted["thinking"]["budget_tokens"], 1280);
        assert_eq!(
            converted["messages"][0]["content"][0]["source"]["type"],
            "base64"
        );
        assert_eq!(
            converted["messages"][0]["content"][0]["source"]["media_type"],
            "image/jpeg"
        );
    }

    #[test]
    fn openai_responses_nested_tools_survive_claude_messages_and_kiro_envelope_conversion() {
        let request = json!({
            "model": "gpt-5",
            "input": "Use the weather tool for Shanghai.",
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"}
                        },
                        "required": []
                    }
                }
            }],
            "tool_choice": {
                "type": "function",
                "function": {"name": "get_weather"}
            }
        });

        let claude = build_standard_request_body(
            &request,
            "openai:responses",
            "claude-sonnet-4.6",
            "kiro",
            "claude:messages",
            "/v1/responses",
            true,
            None,
            None,
        )
        .expect("openai responses should convert to claude messages");
        assert_eq!(claude["tools"][0]["name"], "get_weather");
        assert_eq!(claude["tool_choice"]["name"], "get_weather");

        assert_eq!(claude["tools"][0]["name"], "get_weather");
        assert!(
            claude["tools"][0]["input_schema"].get("required").is_some(),
            "surface conversion should preserve the Claude tool schema before transport envelopes"
        );
    }

    /// 验证 Responses 内置工具与函数声明组合会生成 Gemini 3 服务端调用开关。
    #[test]
    fn openai_responses_builtin_and_function_tools_enable_gemini_server_invocations() {
        let request = json!({
            "model": "gpt-5",
            "input": "Search first, then save the result.",
            "tools": [
                {"type": "web_search_preview"},
                {
                    "type": "function",
                    "name": "save_result",
                    "description": "Save a search result",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "result": {"type": "string"}
                        },
                        "required": ["result"]
                    }
                }
            ],
            "tool_choice": "required"
        });

        let gemini = build_standard_request_body(
            &request,
            "openai:responses",
            "gemini-3-flash-preview",
            "google",
            "gemini:generate_content",
            "/v1/responses",
            true,
            None,
            None,
        )
        .expect("openai responses should convert to gemini generate content");

        assert_eq!(gemini["tools"][0]["googleSearch"], json!({}));
        assert_eq!(
            gemini["tools"][1]["functionDeclarations"][0]["name"],
            "save_result"
        );
        assert_eq!(
            gemini["toolConfig"]["includeServerSideToolInvocations"],
            true
        );
        assert_eq!(gemini["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
    }

    #[test]
    fn xai_keeps_client_search_functions_distinct_from_hosted_search() {
        for name in ["web_search", "web_search_internal"] {
            for hosted in [false, true] {
                let mut tools = vec![json!({
                    "name": name,
                    "description": "Search internal documents",
                    "input_schema": {"type": "object", "properties": {"query": {"type": "string"}}}
                })];
                if hosted {
                    tools.push(json!({"type": "web_search_20260209", "name": "internet_search"}));
                }
                let request = json!({
                    "model": "source", "max_tokens": 64,
                    "messages": [{"role": "user", "content": "Search internal documents"}],
                    "tools": tools,
                    "tool_choice": {"type": "tool", "name": name}
                });
                if hosted {
                    // Claude 原生 web_search 服务端工具到通用 Responses 目标
                    // 无已审计映射（xAI overlay 的 hosted search 投影是 provider
                    // 私有适配），跨格式统一 fail closed。
                    assert!(
                        build_standard_request_body(
                            &request,
                            "claude:messages",
                            "grok-4.6",
                            "xai",
                            "openai:responses",
                            "/v1/messages",
                            true,
                            None,
                            None,
                        )
                        .is_none(),
                        "claude hosted web_search must fail closed for generic responses targets"
                    );
                    continue;
                }
                let converted = build_standard_request_body(
                    &request,
                    "claude:messages",
                    "grok-4.6",
                    "xai",
                    "openai:responses",
                    "/v1/messages",
                    true,
                    None,
                    None,
                )
                .unwrap();
                assert_eq!(
                    converted["tool_choice"],
                    json!({"type": "function", "name": name})
                );
                assert!(converted["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|tool| tool["type"] != "web_search"));
            }
        }

        let request = json!({
            "model": "source", "max_tokens": 64,
            "messages": [{"role": "user", "content": "Search the internet"}],
            "tools": [{"type": "web_search_20260209", "name": "internet_search"}],
            "tool_choice": {"type": "tool", "name": "internet_search"}
        });
        assert!(
            build_standard_request_body(
                &request,
                "claude:messages",
                "grok-4.6",
                "xai",
                "openai:responses",
                "/v1/messages",
                true,
                None,
                None,
            )
            .is_none(),
            "claude web_search server tool must fail closed without an audited mapping"
        );
    }

    #[test]
    fn xai_preserves_function_choices_in_chat_and_responses_requests() {
        for name in ["web_search", "web_search_internal"] {
            for (client, request) in [
                (
                    "openai:chat",
                    json!({
                        "messages": [{"role": "user", "content": "search"}],
                        "tools": [{"type": "function", "function": {"name": name, "parameters": {"type": "object"}}}],
                        "tool_choice": {"type": "function", "function": {"name": name}}
                    }),
                ),
                (
                    "openai:responses",
                    json!({
                        "input": "search",
                        "tools": [{"type": "function", "name": name, "parameters": {"type": "object"}}],
                        "tool_choice": {"type": "function", "name": name}
                    }),
                ),
            ] {
                let converted = build_standard_request_body(
                    &request,
                    client,
                    "grok-4.6",
                    "xai",
                    "openai:responses",
                    "/v1/responses",
                    true,
                    None,
                    None,
                )
                .unwrap();
                assert_eq!(
                    converted["tool_choice"],
                    json!({"type": "function", "name": name})
                );
                assert_eq!(converted["tools"].as_array().unwrap().len(), 1);
            }
        }
    }

    #[test]
    fn xai_image_allowed_tools_preserves_mode_and_restricts_available_tools() {
        for mode in ["auto", "required"] {
            for mixed in [false, true] {
                let mut allowed = vec![json!({"type": "image_generation"})];
                if mixed {
                    allowed.push(json!({"type": "function", "name": "lookup"}));
                }
                let request = json!({
                    "input": "Draw a cat",
                    "tools": [
                        {"type": "web_search"}, {"type": "image_generation"},
                        {"type": "function", "name": "lookup", "parameters": {"type": "object"}}
                    ],
                    "tool_choice": {"type": "allowed_tools", "mode": mode, "tools": allowed}
                });
                let converted = build_standard_request_body(
                    &request,
                    "openai:responses",
                    "grok-4.6",
                    "xai",
                    "openai:responses",
                    "/v1/responses",
                    true,
                    None,
                    None,
                )
                .unwrap();
                if mixed {
                    assert_eq!(
                        converted["tool_choice"],
                        json!({
                            "type": "allowed_tools", "mode": mode,
                            "tools": [{"type": "function", "name": "lookup"}]
                        })
                    );
                    assert_eq!(converted["tools"].as_array().unwrap().len(), 3);
                } else {
                    assert_eq!(converted["tool_choice"], mode);
                    assert_eq!(converted["tools"], json!([{"type": "image_generation"}]));
                }
            }
        }
    }

    #[test]
    fn xai_responses_preserves_requested_encrypted_reasoning_and_replayed_input() {
        let reasoning = json!({"type": "reasoning", "id": "550e8400-e29b-41d4-a716-446655440000", "summary": [], "encrypted_content": "opaque-xai-state"});
        let request = json!({
            "input": [reasoning.clone(), {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Previous answer"}]}, {"role": "user", "content": "Continue"}],
            "include": ["reasoning.encrypted_content"], "store": false
        });
        let converted = build_standard_request_body(
            &request,
            "openai:responses",
            "grok-4.6",
            "xai",
            "openai:responses",
            "/v1/responses",
            true,
            None,
            None,
        )
        .unwrap();
        assert_eq!(converted["include"], request["include"]);
        assert_eq!(converted["input"][0], reasoning);
        assert_eq!(converted["store"], false);
    }

    #[test]
    fn xai_standard_conversion_rejects_unmapped_chat_fields_for_responses() {
        // stop/stream_options（以及 web_search_options 的通用 Responses 映射）
        // 没有已审计等价物：跨格式 fail closed，不再依赖 xAI 事后剥离。
        let request = json!({
            "model": "source-model",
            "messages": [{"role": "user", "content": "Hello xAI"}],
            "max_tokens": 128,
            "stop": ["END"],
            "stream_options": {"include_usage": true},
            "metadata": {"user_id": "claude-session"},
            "web_search_options": {"search_context_size": "high"}
        });
        assert!(
            build_standard_request_body(
                &request,
                "openai:chat",
                "grok-4.6",
                "xai",
                "openai:responses",
                "/v1/chat/completions",
                true,
                None,
                None,
            )
            .is_none(),
            "unmapped chat fields must fail closed onto xAI Responses"
        );

        // 无未映射字段的同形请求仍可正常转换。
        let clean = json!({
            "model": "source-model",
            "messages": [{"role": "user", "content": "Hello xAI"}],
            "max_tokens": 128,
            "metadata": {"user_id": "claude-session"}
        });
        let converted = build_standard_request_body(
            &clean,
            "openai:chat",
            "grok-4.6",
            "xai",
            "openai:responses",
            "/v1/chat/completions",
            true,
            None,
            None,
        )
        .expect("clean chat should convert onto xAI Responses");

        assert_eq!(converted["model"], "grok-4.6");
        assert!(converted.get("stop").is_none());
        assert!(converted.get("stream_options").is_none());
        assert!(converted.get("metadata").is_none());
        assert!(converted.get("input").is_some() || converted.get("messages").is_none());
        assert_eq!(converted["max_output_tokens"], 128);
    }

    #[test]
    fn xai_standard_conversion_covers_claude_and_gemini_clients() {
        let claude = json!({
            "model": "claude-sonnet",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "Hello xAI"}],
            "metadata": {
                "user_id": "{\"device_id\":\"dev-1\",\"account_uuid\":\"acct-1\",\"session_id\":\"sess-1\"}"
            },
            "tools": [
                {"type": "web_search_20250305", "name": "web_search"},
                {
                    "name": "lookup",
                    "description": "Look something up",
                    "input_schema": {"type": "object", "properties": {}}
                }
            ],
            "tool_choice": {"type": "tool", "name": "web_search"}
        });
        // Claude 原生 web_search 服务端工具到 Responses 无已审计通用映射：
        // 统一 fail closed；xAI overlay 的 hosted search 投影属 provider 私有适配。
        assert!(
            build_standard_request_body(
                &claude,
                "claude:messages",
                "grok-4.6",
                "xai",
                "openai:responses",
                "/v1/messages",
                true,
                None,
                None,
            )
            .is_none(),
            "claude web_search server tool must fail closed onto xAI Responses"
        );

        // 普通函数工具的 Claude 请求仍可转换。
        let claude_clean = json!({
            "model": "claude-sonnet",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "Hello xAI"}],
            "tools": [{
                "name": "lookup",
                "description": "Look something up",
                "input_schema": {"type": "object", "properties": {}}
            }],
            "tool_choice": {"type": "tool", "name": "lookup"}
        });
        let converted = build_standard_request_body(
            &claude_clean,
            "claude:messages",
            "grok-4.6",
            "xai",
            "openai:responses",
            "/v1/messages",
            true,
            None,
            None,
        )
        .expect("claude with plain function tools should convert onto xAI Responses");
        assert_eq!(converted["model"], "grok-4.6");
        assert!(converted.get("metadata").is_none());
        assert!(converted.get("context_management").is_none());
        assert!(converted["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|tool| tool["type"] == "function"));
        assert_eq!(converted["tool_choice"]["type"], "function");
        assert!(converted.get("input").is_some());

        let gemini = json!({
            "model": "gemini-2.5-pro",
            "contents": [{
                "role": "user",
                "parts": [{"text": "Hello xAI"}]
            }],
            "tools": [{"googleSearch": {}}]
        });
        // Gemini googleSearch 内置工具到通用 Responses 无已审计映射
        // （xAI hosted search 投影是 provider 私有适配），跨格式统一 fail closed。
        assert!(
            build_standard_request_body(
                &gemini,
                "gemini:generate_content",
                "grok-4.6",
                "xai",
                "openai:responses",
                "/v1beta/models/gemini-2.5-pro:generateContent",
                false,
                None,
                None,
            )
            .is_none(),
            "gemini googleSearch builtin tool must fail closed for generic responses targets"
        );

        // 无内置工具的 Gemini 请求仍可转换。
        let gemini_clean = json!({
            "model": "gemini-2.5-pro",
            "contents": [{
                "role": "user",
                "parts": [{"text": "Hello xAI"}]
            }]
        });
        let converted = build_standard_request_body(
            &gemini_clean,
            "gemini:generate_content",
            "grok-4.6",
            "xai",
            "openai:responses",
            "/v1beta/models/gemini-2.5-pro:generateContent",
            false,
            None,
            None,
        )
        .expect("clean gemini should convert onto xAI Responses");
        assert_eq!(converted["model"], "grok-4.6");
        assert!(converted.get("input").is_some());

        let same_format = json!({
            "model": "grok-4.6",
            "input": "hello",
            "previous_response_id": "resp_123",
            "stop": ["END"],
            "metadata": {"user_id": "claude-session"}
        });
        let converted = build_standard_request_body(
            &same_format,
            "openai:responses",
            "grok-4.6",
            "xai",
            "openai:responses",
            "/v1/responses",
            true,
            None,
            None,
        )
        .expect("same-format xAI Responses should sanitize in place");
        assert!(converted.get("previous_response_id").is_none());
        assert!(converted.get("stop").is_none());
        assert!(converted.get("metadata").is_none());
    }

    #[test]
    fn antigravity_claude_cache_control_scope_and_toggle_matrix() {
        use std::borrow::Cow;

        let clean_request = json!({
            "model": "claude-sonnet-4-5",
            "messages": [{"role": "user", "content": "Hello"}]
        });
        let cached_request = json!({
            "model": "claude-sonnet-4-5",
            "system": [{"type": "text", "text": "Be concise.", "cache_control": {"type": "ephemeral"}}],
            "messages": [{"role": "user", "content": "Hello"}]
        });

        // 1. Clean request has no markers -> borrowed Cow regardless of scope/config
        let clean_res = prepare_antigravity_claude_cache_control_body(
            &clean_request,
            "claude:messages",
            "antigravity",
            "gemini:generate_content",
            None,
        );
        assert!(matches!(clean_res, Cow::Borrowed(_)));
        assert!(std::ptr::eq(clean_res.as_ref(), &clean_request));

        // 2. Non-Antigravity provider -> borrowed Cow
        for other_provider in ["gemini", "openai", "claude", "bedrock"] {
            let res = prepare_antigravity_claude_cache_control_body(
                &cached_request,
                "claude:messages",
                other_provider,
                "gemini:generate_content",
                None,
            );
            assert!(matches!(res, Cow::Borrowed(_)));
            assert!(std::ptr::eq(res.as_ref(), &cached_request));
        }

        // 3. Non-qualifying client format -> borrowed Cow
        let non_claude_client = prepare_antigravity_claude_cache_control_body(
            &cached_request,
            "openai:chat",
            "antigravity",
            "gemini:generate_content",
            None,
        );
        assert!(matches!(non_claude_client, Cow::Borrowed(_)));

        // 4. Non-qualifying provider format -> borrowed Cow
        let non_gemini_target = prepare_antigravity_claude_cache_control_body(
            &cached_request,
            "claude:messages",
            "antigravity",
            "claude:messages",
            None,
        );
        assert!(matches!(non_gemini_target, Cow::Borrowed(_)));

        // 5. Config toggle: explicit false retains strict refusal (borrowed)
        let disabled_config = json!({"antigravity_cache_control_compat": false});
        let strict = prepare_antigravity_claude_cache_control_body(
            &cached_request,
            "claude:messages",
            "antigravity",
            "gemini:generate_content",
            Some(&disabled_config),
        );
        assert!(matches!(strict, Cow::Borrowed(_)));

        // 6. Qualifying + enabled (None, empty obj, or true) with markers -> owned sanitized Cow
        for config in [
            None,
            Some(json!({})),
            Some(json!({"antigravity_cache_control_compat": true})),
        ] {
            let prepared = prepare_antigravity_claude_cache_control_body(
                &cached_request,
                "claude:messages",
                "antigravity",
                "gemini:generate_content",
                config.as_ref(),
            );
            assert!(matches!(prepared, Cow::Owned(_)));
            assert!(prepared.get("system").unwrap()[0]
                .get("cache_control")
                .is_none());
        }

        // 7. Case and whitespace insensitivity on provider and format
        let loose_alias = prepare_antigravity_claude_cache_control_body(
            &cached_request,
            " CLAUDE:MESSAGES ",
            " AnTiGrAvItY ",
            " GEMINI:GENERATE_CONTENT ",
            None,
        );
        assert!(matches!(loose_alias, Cow::Owned(_)));
    }

    #[test]
    fn antigravity_claude_cache_control_sanitizes_protocol_preserves_business_fields_and_converts()
    {
        use std::borrow::Cow;

        let original = json!({
            "model": "claude-opus-5-5",
            "max_tokens": 4096,
            "cache_control": {"type": "ephemeral"},
            "system": [{"type": "text", "text": "Lookup tool instructions.", "cache_control": {"type": "ephemeral"}}],
            "tools": [{
                "name": "lookup",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "cache_control": {"type": "string", "description": "business field"}
                    }
                },
                "cache_control": {"type": "ephemeral"}
            }],
            "messages": [
                {"role": "user", "content": "Perform lookup."},
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "thinking",
                            "thinking": "Formulating query.",
                            "signature": "opaque-signature-token"
                        },
                        {
                            "type": "tool_use",
                            "id": "call_1",
                            "name": "lookup",
                            "input": {"cache_control": "business-payload-value"}
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": [
                            {
                                "type": "text",
                                "text": "result details",
                                "cache_control": {"type": "ephemeral"}
                            }
                        ],
                        "cache_control": {"type": "ephemeral"}
                    }]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Continue.", "cache_control": {"type": "ephemeral"}}]
                }
            ]
        });

        let snapshot = original.clone();

        // Strict conversion on original fails closed because Gemini target has no lossless cache_control equivalent
        let disabled_config = json!({"antigravity_cache_control_compat": false});
        let strict = prepare_antigravity_claude_cache_control_body(
            &original,
            "claude:messages",
            "antigravity",
            "gemini:generate_content",
            Some(&disabled_config),
        );
        assert!(matches!(strict, Cow::Borrowed(_)));
        assert!(build_standard_request_body(
            &strict,
            "claude:messages",
            "claude-opus-5-5",
            "antigravity",
            "gemini:generate_content",
            "/v1/messages",
            false,
            None,
            None
        )
        .is_none());

        // Sanitized conversion succeeds for both stream=false and stream=true
        for stream in [false, true] {
            let prepared = prepare_antigravity_claude_cache_control_body(
                &original,
                "claude:messages",
                "antigravity",
                "gemini:generate_content",
                None,
            );
            assert!(matches!(prepared, Cow::Owned(_)));

            // Protocol markers stripped
            assert!(prepared.get("cache_control").is_none());
            assert!(prepared["system"][0].get("cache_control").is_none());
            assert!(prepared["tools"][0].get("cache_control").is_none());
            assert!(prepared["messages"][2]["content"][0]
                .get("cache_control")
                .is_none());
            assert!(prepared["messages"][2]["content"][0]["content"][0]
                .get("cache_control")
                .is_none());
            assert!(prepared["messages"][3]["content"][0]
                .get("cache_control")
                .is_none());

            // Business fields preserved
            assert_eq!(
                prepared["tools"][0]["input_schema"]["properties"]["cache_control"]["type"],
                "string"
            );
            assert_eq!(
                prepared["messages"][1]["content"][1]["input"]["cache_control"],
                "business-payload-value"
            );
            assert_eq!(
                prepared["messages"][1]["content"][0]["signature"],
                "opaque-signature-token"
            );
            assert_eq!(
                prepared["messages"][2]["content"][0]["content"][0]["text"],
                "result details"
            );

            // Conversion into Gemini request body succeeds
            let converted = build_standard_request_body(
                &prepared,
                "claude:messages",
                "claude-opus-5-5",
                "antigravity",
                "gemini:generate_content",
                "/v1/messages",
                stream,
                None,
                None,
            )
            .expect("prepared body should convert cleanly to Gemini");
            assert!(converted.get("contents").is_some());
        }

        // Original request completely untouched
        assert_eq!(original, snapshot);

        // Native Claude fallback retains all original cache markers
        let fallback = build_standard_request_body(
            &original,
            "claude:messages",
            "claude-opus-5-5",
            "claude",
            "claude:messages",
            "/v1/messages",
            false,
            None,
            None,
        )
        .expect("native Claude fallback must succeed");
        assert_eq!(fallback["system"], snapshot["system"]);
        assert_eq!(fallback["tools"], snapshot["tools"]);

        let fallback_blocks: Vec<&Value> = fallback["messages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| {
                m.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .collect();
        let tool_result_block = fallback_blocks
            .iter()
            .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .expect("tool_result block preserved in fallback");
        assert_eq!(
            tool_result_block["cache_control"],
            json!({"type": "ephemeral"})
        );
        assert_eq!(
            tool_result_block["content"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );
        let continue_block = fallback_blocks
            .iter()
            .find(|b| b.get("text").and_then(Value::as_str) == Some("Continue."))
            .expect("continue text block preserved in fallback");
        assert_eq!(
            continue_block["cache_control"],
            json!({"type": "ephemeral"})
        );
    }

    #[test]
    fn antigravity_claude_cache_control_preserves_arbitrary_object_tool_result_content() {
        use std::borrow::Cow;

        // 1. Tool result content is an arbitrary JSON object with a business `cache_control` key.
        // No protocol markers present -> must return Borrowed (no clone, no modification).
        let clean_with_object_tool_result = json!({
            "model": "claude-sonnet-4-5",
            "messages": [
                {"role": "user", "content": "Fetch data."},
                {
                    "role": "assistant",
                    "content": [{"type": "tool_use", "id": "call_1", "name": "fetch", "input": {}}]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": {"cache_control": "business-header-value", "status": "ok"}
                    }]
                }
            ]
        });

        let borrowed = prepare_antigravity_claude_cache_control_body(
            &clean_with_object_tool_result,
            "claude:messages",
            "antigravity",
            "gemini:generate_content",
            None,
        );
        assert!(matches!(borrowed, Cow::Borrowed(_)));
        assert!(std::ptr::eq(
            borrowed.as_ref(),
            &clean_with_object_tool_result
        ));
        assert_eq!(
            borrowed["messages"][2]["content"][0]["content"]["cache_control"],
            "business-header-value"
        );

        // 2. Same arbitrary object tool result, but with outer protocol cache_control markers.
        // Sanitizer must strip outer protocol markers, preserve the tool_result object intact,
        // and standard conversion into Gemini must retain it under functionResponse.response.result.
        let cached_with_object_tool_result = json!({
            "model": "claude-sonnet-4-5",
            "system": [{"type": "text", "text": "System prompt", "cache_control": {"type": "ephemeral"}}],
            "messages": [
                {"role": "user", "content": "Fetch data."},
                {
                    "role": "assistant",
                    "content": [{"type": "tool_use", "id": "call_1", "name": "fetch", "input": {}}]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": {"cache_control": "business-header-value", "status": "ok"},
                        "cache_control": {"type": "ephemeral"}
                    }]
                }
            ]
        });

        let prepared = prepare_antigravity_claude_cache_control_body(
            &cached_with_object_tool_result,
            "claude:messages",
            "antigravity",
            "gemini:generate_content",
            None,
        );
        assert!(matches!(prepared, Cow::Owned(_)));
        assert!(prepared["system"][0].get("cache_control").is_none());
        assert!(prepared["messages"][2]["content"][0]
            .get("cache_control")
            .is_none());
        // Business object inside tool_result.content untouched
        assert_eq!(
            prepared["messages"][2]["content"][0]["content"],
            json!({"cache_control": "business-header-value", "status": "ok"})
        );

        // Standard conversion succeeds into Gemini GenerateContent
        let converted = build_standard_request_body(
            &prepared,
            "claude:messages",
            "claude-sonnet-4-5",
            "antigravity",
            "gemini:generate_content",
            "/v1/messages",
            false,
            None,
            None,
        )
        .expect("conversion into Gemini must succeed");

        let parts: Vec<&Value> = converted["contents"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["parts"].as_array().unwrap())
            .collect();
        let function_response = parts
            .iter()
            .find_map(|p| p.get("functionResponse"))
            .expect("functionResponse part present in Gemini body");
        assert_eq!(
            function_response["response"]["result"]["cache_control"],
            "business-header-value"
        );
    }
}
