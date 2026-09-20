use serde_json::{json, Map, Value};

use crate::{
    formats::context::FormatContext,
    formats::shared::citations::{
        canonical_citation, canonical_citations_to_claude_citations,
        canonical_citations_to_openai_annotations,
    },
    protocol::canonical::{
        canonical_extension_object_mut, canonical_usage_total_input_tokens,
        canonical_usage_total_tokens_for_inclusive_input,
        gemini_candidate_to_openai_url_annotations, gemini_extensions,
        gemini_part_to_canonical_block, gemini_stop_reason_to_canonical, gemini_usage_to_canonical,
        namespace_extension_object, CanonicalContentBlock, CanonicalResponse,
        CanonicalResponseOutput, CanonicalRole, CanonicalStopReason, CanonicalUsage,
        CLAUDE_EXTENSION_NAMESPACE, OPENAI_RESPONSES_EXTENSION_NAMESPACE,
    },
};

/// Project Gemini grounding metadata onto the answer text as structured
/// citations.
///
/// Native `googleSearch` grounding runs inside Google, so there is no
/// client-visible tool call and the evidence only exists in
/// `candidates[].groundingMetadata`. Cross-format targets used to drop that
/// wholesale, leaving callers with prose that names its sources but nothing a
/// client can render or verify. Every grounded span is therefore emitted twice,
/// each time in the target family's own standard shape: OpenAI `url_citation`
/// annotations and Claude `web_search_result_location` citations. Both ride
/// extension namespaces the respective emitters already merge onto the text
/// block, so no target has to learn anything Gemini-specific.
fn attach_gemini_grounding_citations(
    candidate: &Map<String, Value>,
    content: &mut [(usize, CanonicalContentBlock)],
) {
    let Some(grounding) = gemini_candidate_grounding(candidate) else {
        return;
    };
    // 保留原始 part 索引，避免引用被附到思考块后的第一个可见文本上。
    for (part_index, block) in content {
        let CanonicalContentBlock::Text { text, extensions } = block else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let citations = gemini_grounding_citations(grounding, text, *part_index);
        if citations.is_empty() {
            continue;
        }
        let unanchored = citations
            .iter()
            .all(|citation| citation.get("start_index").is_none());
        let annotations = canonical_citations_to_openai_annotations(&citations);
        let claude_citations = canonical_citations_to_claude_citations(&citations);
        canonical_extension_object_mut(extensions, OPENAI_RESPONSES_EXTENSION_NAMESPACE)
            .entry("annotations".to_string())
            .or_insert_with(|| Value::Array(annotations));
        canonical_extension_object_mut(extensions, CLAUDE_EXTENSION_NAMESPACE)
            .entry("citations".to_string())
            .or_insert_with(|| Value::Array(claude_citations));
        if unanchored {
            break;
        }
    }
}

pub(crate) fn gemini_candidate_grounding(candidate: &Map<String, Value>) -> Option<&Value> {
    candidate
        .get("groundingMetadata")
        .or_else(|| candidate.get("grounding_metadata"))
}

/// Normalise `groundingMetadata` into neutral citations against `text`.
///
/// Gemini reports segment bounds as UTF-8 byte offsets while every target
/// counts characters, so the bounds are converted rather than copied.
pub(crate) fn gemini_grounding_citations(
    grounding: &Value,
    text: &str,
    part_index: usize,
) -> Vec<Value> {
    let chunks = grounding
        .get("groundingChunks")
        .or_else(|| grounding.get("grounding_chunks"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if chunks.is_empty() {
        return Vec::new();
    }

    let supports = grounding
        .get("groundingSupports")
        .or_else(|| grounding.get("grounding_supports"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    let mut citations = Vec::new();
    for support in supports {
        let segment = support.get("segment");
        // protobuf 省略的 partIndex 表示 0；其他文本块的区间不能挪到当前块。
        if segment
            .and_then(|segment| {
                segment
                    .get("partIndex")
                    .or_else(|| segment.get("part_index"))
            })
            .and_then(Value::as_u64)
            .unwrap_or(0)
            != part_index as u64
        {
            continue;
        }
        let start = segment
            .and_then(|segment| {
                segment
                    .get("startIndex")
                    .or_else(|| segment.get("start_index"))
            })
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let end = segment
            .and_then(|segment| segment.get("endIndex").or_else(|| segment.get("end_index")))
            .and_then(Value::as_u64);
        let start_byte = gemini_clamped_byte_offset(text, start);
        let end_byte = end
            .map(|end| gemini_clamped_byte_offset(text, end))
            .filter(|end| *end >= start_byte);
        let cited_text = segment
            .and_then(|segment| segment.get("text"))
            .and_then(Value::as_str)
            .or_else(|| end_byte.map(|end| &text[start_byte..end]))
            .map(str::trim)
            .filter(|cited_text| !cited_text.is_empty());
        let indices = support
            .get("groundingChunkIndices")
            .or_else(|| support.get("grounding_chunk_indices"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for index in indices {
            let Some(chunk) = index
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| chunks.get(index))
            else {
                continue;
            };
            let Some((uri, title)) = gemini_grounding_chunk_source(chunk) else {
                continue;
            };
            citations.push(canonical_citation(
                uri,
                title,
                Some(text[..start_byte].chars().count()),
                end_byte.map(|end| text[..end].chars().count()),
                cited_text,
            ));
        }
    }

    // `groundingSupports` is optional; without it the chunks are still the
    // evidence, just unanchored.
    if supports.is_empty() {
        for chunk in chunks {
            let Some((uri, title)) = gemini_grounding_chunk_source(chunk) else {
                continue;
            };
            citations.push(canonical_citation(uri, title, None, None, None));
        }
    }
    citations
}

fn gemini_grounding_chunk_source(chunk: &Value) -> Option<(&str, Option<&str>)> {
    let source = chunk
        .get("web")
        .or_else(|| chunk.get("retrievedContext"))
        .or_else(|| chunk.get("retrieved_context"))?;
    let uri = source
        .get("uri")
        .or_else(|| source.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|uri| !uri.is_empty())?;
    let title = source
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty());
    Some((uri, title))
}

/// Gemini offsets are byte counts into the UTF-8 answer. A truncated or stale
/// offset must not panic the conversion, so snap it into range and back onto a
/// character boundary.
fn gemini_clamped_byte_offset(text: &str, byte_offset: u64) -> usize {
    let mut offset = usize::try_from(byte_offset)
        .unwrap_or(text.len())
        .min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn from(body: &Value, _ctx: &FormatContext) -> Option<CanonicalResponse> {
    from_raw(body)
}

pub fn to(response: &CanonicalResponse, ctx: &FormatContext) -> Option<Value> {
    to_raw(response, &ctx.report_context_value())
}

pub fn from_raw(body_json: &Value) -> Option<CanonicalResponse> {
    let body = body_json.as_object()?;
    if body.contains_key("error") {
        return None;
    }

    let candidates = body.get("candidates")?.as_array()?;
    let usage = gemini_usage_to_canonical(body.get("usageMetadata"));
    let mut outputs = Vec::new();
    for (fallback_index, candidate) in candidates.iter().enumerate() {
        let candidate_object = candidate.as_object()?;
        let parts = candidate_object
            .get("content")
            .and_then(Value::as_object)
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut indexed_content = parts
            .iter()
            .enumerate()
            .filter_map(|(index, part)| {
                gemini_part_to_canonical_block(part, index).map(|block| (index, block))
            })
            .collect::<Vec<_>>();
        if let Some(annotations_by_part) =
            gemini_candidate_to_openai_url_annotations(candidate_object)
        {
            for (part_index, annotations) in annotations_by_part {
                let Some((_, CanonicalContentBlock::Text { extensions, .. })) = indexed_content
                    .iter_mut()
                    .find(|(source_part_index, _)| *source_part_index == part_index)
                else {
                    continue;
                };
                canonical_extension_object_mut(extensions, OPENAI_RESPONSES_EXTENSION_NAMESPACE)
                    .insert("annotations".to_string(), Value::Array(annotations));
            }
        }
        attach_gemini_grounding_citations(candidate_object, &mut indexed_content);
        let content = indexed_content
            .into_iter()
            .map(|(_, block)| block)
            .collect::<Vec<_>>();
        let mut stop_reason = candidate_object
            .get("finishReason")
            .or_else(|| candidate_object.get("finish_reason"))
            .and_then(Value::as_str)
            .and_then(gemini_stop_reason_to_canonical);
        if content
            .iter()
            .any(|block| matches!(block, CanonicalContentBlock::ToolUse { .. }))
            && stop_reason
                .as_ref()
                .is_none_or(|reason| matches!(reason, CanonicalStopReason::EndTurn))
        {
            stop_reason = Some(CanonicalStopReason::ToolUse);
        }
        let mut extensions = gemini_extensions(
            candidate_object,
            &["index", "content", "finishReason", "finish_reason"],
        );
        if let Some(content_object) = candidate_object.get("content").and_then(Value::as_object) {
            let content_extra = content_object
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "role" | "parts"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>();
            if !content_extra.is_empty() {
                canonical_extension_object_mut(&mut extensions, "gemini").insert(
                    "raw_content_fields".to_string(),
                    Value::Object(content_extra),
                );
            }
        }
        if let Some(raw_finish_reason) = candidate_object
            .get("finishReason")
            .or_else(|| candidate_object.get("finish_reason"))
            .cloned()
        {
            canonical_extension_object_mut(&mut extensions, "gemini")
                .insert("raw_finish_reason".to_string(), raw_finish_reason);
        }
        outputs.push(CanonicalResponseOutput {
            index: candidate_object
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(fallback_index),
            role: CanonicalRole::Assistant,
            content,
            stop_reason,
            extensions,
        });
    }
    outputs.retain(|output| {
        gemini_response_output_has_visible_content(output)
            || gemini_response_output_is_reasoning_exhausted_terminal(output, usage.as_ref())
    });
    if outputs.is_empty() {
        return None;
    }
    let content = outputs
        .first()
        .map(|output| output.content.clone())
        .unwrap_or_default();
    let stop_reason = outputs
        .first()
        .and_then(|output| output.stop_reason.clone());

    let mut canonical = CanonicalResponse {
        id: body
            .get("responseId")
            .or_else(|| body.get("_v1internal_response_id"))
            .and_then(Value::as_str)
            .unwrap_or("gemini-local-finalize")
            .to_string(),
        model: body
            .get("modelVersion")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        outputs,
        content,
        stop_reason,
        usage,
        extensions: gemini_extensions(
            body,
            &[
                "responseId",
                "_v1internal_response_id",
                "modelVersion",
                "candidates",
                "usageMetadata",
            ],
        ),
    };
    if let Some(candidates) = body.get("candidates").cloned() {
        canonical_extension_object_mut(&mut canonical.extensions, "gemini")
            .insert("raw_candidates".to_string(), candidates);
    }
    Some(canonical)
}

fn gemini_response_output_has_visible_content(output: &CanonicalResponseOutput) -> bool {
    output.content.iter().any(|block| match block {
        CanonicalContentBlock::Text { text, .. } | CanonicalContentBlock::Thinking { text, .. } => {
            !text.trim().is_empty()
        }
        CanonicalContentBlock::ToolUse { .. }
        | CanonicalContentBlock::ToolResult { .. }
        | CanonicalContentBlock::Image { .. }
        | CanonicalContentBlock::File { .. }
        | CanonicalContentBlock::Audio { .. } => true,
        CanonicalContentBlock::Unknown { .. } => false,
    })
}

fn gemini_response_output_is_reasoning_exhausted_terminal(
    output: &CanonicalResponseOutput,
    usage: Option<&CanonicalUsage>,
) -> bool {
    matches!(output.stop_reason, Some(CanonicalStopReason::MaxTokens))
        && usage.is_some_and(|usage| usage.reasoning_tokens > 0)
        && output.content.iter().any(|block| {
            matches!(
                block,
                CanonicalContentBlock::Thinking {
                    text,
                    signature: Some(signature),
                    ..
                } if text.trim().is_empty() && !signature.trim().is_empty()
            )
        })
}

pub fn to_raw(canonical: &CanonicalResponse, report_context: &Value) -> Option<Value> {
    let mut response = canonical_to_gemini_response(canonical, report_context)?;
    if let Some(object) = response.as_object_mut() {
        if let Some(gemini) = canonical
            .extensions
            .get("gemini")
            .and_then(Value::as_object)
        {
            for (key, value) in gemini {
                if key == "raw_candidates" || object.contains_key(key) {
                    continue;
                }
                object.insert(key.clone(), value.clone());
            }
        }
    }
    Some(response)
}

fn canonical_to_gemini_response(
    canonical: &CanonicalResponse,
    report_context: &Value,
) -> Option<Value> {
    let outputs = if canonical.outputs.is_empty() {
        vec![CanonicalResponseOutput {
            index: 0,
            role: crate::protocol::canonical::CanonicalRole::Assistant,
            content: canonical.content.clone(),
            stop_reason: canonical.stop_reason.clone(),
            extensions: Default::default(),
        }]
    } else {
        canonical.outputs.clone()
    };
    let mut candidates = Vec::new();
    for output in outputs {
        let parts = canonical_blocks_to_gemini_parts(&output.content)?;
        let mut candidate = json!({
            "index": output.index,
            "content": {
                "role": "model",
                "parts": parts,
            },
            "finishReason": canonical_stop_reason_to_gemini(
                output.stop_reason.as_ref().or(canonical.stop_reason.as_ref())
            ),
        });
        if let Some(candidate_object) = candidate.as_object_mut() {
            if let Some(gemini) = output.extensions.get("gemini").and_then(Value::as_object) {
                if let Some(raw_finish_reason) = gemini.get("raw_finish_reason").cloned() {
                    candidate_object.insert("finishReason".to_string(), raw_finish_reason);
                }
                if let Some(content_extra) =
                    gemini.get("raw_content_fields").and_then(Value::as_object)
                {
                    if let Some(content_object) = candidate_object
                        .get_mut("content")
                        .and_then(Value::as_object_mut)
                    {
                        for (key, value) in content_extra {
                            content_object.entry(key.clone()).or_insert(value.clone());
                        }
                    }
                }
                for (key, value) in gemini {
                    if matches!(key.as_str(), "raw_finish_reason" | "raw_content_fields") {
                        continue;
                    }
                    candidate_object.entry(key.clone()).or_insert(value.clone());
                }
            }
        }
        candidates.push(candidate);
    }

    let mut response = Map::new();
    response.insert(
        "responseId".to_string(),
        Value::String(if canonical.id.trim().is_empty() {
            "resp-local-finalize".to_string()
        } else {
            canonical.id.clone()
        }),
    );
    response.insert(
        "modelVersion".to_string(),
        Value::String(
            if canonical.model.trim().is_empty() || canonical.model == "unknown" {
                report_context
                    .get("mapped_model")
                    .and_then(Value::as_str)
                    .or_else(|| report_context.get("model").and_then(Value::as_str))
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                canonical.model.clone()
            },
        ),
    );
    response.insert("candidates".to_string(), Value::Array(candidates));
    if let Some(usage) = &canonical.usage {
        response.insert(
            "usageMetadata".to_string(),
            canonical_usage_to_gemini_usage_metadata(usage),
        );
    }
    Some(Value::Object(response))
}

fn canonical_blocks_to_gemini_parts(blocks: &[CanonicalContentBlock]) -> Option<Vec<Value>> {
    let mut parts = Vec::new();
    for block in blocks {
        if let Some(part) = canonical_block_to_gemini_part(block)? {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        parts.push(json!({ "text": "" }));
    }
    Some(parts)
}

fn canonical_block_to_gemini_part(block: &CanonicalContentBlock) -> Option<Option<Value>> {
    match block {
        CanonicalContentBlock::Text { text, extensions } => Some(Some(
            with_gemini_part_extensions(json!({ "text": text }), extensions),
        )),
        CanonicalContentBlock::Thinking {
            text,
            signature,
            extensions,
            ..
        } => {
            if text.trim().is_empty() {
                return Some(None);
            }
            let mut part = Map::new();
            part.insert("text".to_string(), Value::String(text.clone()));
            part.insert("thought".to_string(), Value::Bool(true));
            if let Some(signature) = signature.as_ref().filter(|value| !value.is_empty()) {
                part.insert(
                    "thoughtSignature".to_string(),
                    Value::String(signature.clone()),
                );
            }
            Some(Some(with_gemini_part_extensions(
                Value::Object(part),
                extensions,
            )))
        }
        CanonicalContentBlock::ToolUse {
            id,
            name,
            input,
            extensions,
        } => Some(Some(with_gemini_part_extensions(
            json!({
                "functionCall": {
                    "id": id,
                    "name": name,
                    "args": gemini_function_args(input),
                }
            }),
            extensions,
        ))),
        CanonicalContentBlock::ToolResult {
            tool_use_id,
            name,
            output,
            content_text,
            extensions,
            ..
        } => Some(Some(gemini_function_response_part(
            tool_use_id,
            name.as_deref().unwrap_or(tool_use_id),
            output.as_ref(),
            content_text.as_deref(),
            extensions,
        ))),
        CanonicalContentBlock::Image {
            data,
            url,
            media_type,
            extensions,
            ..
        } => {
            let part = canonical_media_to_gemini_part(
                media_type.as_deref().unwrap_or("image/png"),
                data.as_deref(),
                url.as_deref(),
            );
            Some(Some(with_gemini_part_extensions(part, extensions)))
        }
        CanonicalContentBlock::File {
            data,
            file_url,
            media_type,
            extensions,
            ..
        } => {
            let part = canonical_media_to_gemini_part(
                media_type.as_deref().unwrap_or("application/octet-stream"),
                data.as_deref(),
                file_url.as_deref(),
            );
            Some(Some(with_gemini_part_extensions(part, extensions)))
        }
        CanonicalContentBlock::Audio {
            data,
            media_type,
            extensions,
            ..
        } => Some(data.as_ref().map(|data| {
            with_gemini_part_extensions(
                json!({
                    "inlineData": {
                        "mimeType": media_type.clone().unwrap_or_else(|| "audio/mpeg".to_string()),
                        "data": data,
                    }
                }),
                extensions,
            )
        })),
        CanonicalContentBlock::Unknown {
            payload,
            extensions,
            ..
        } => extensions
            .get("gemini")
            .and_then(Value::as_object)
            .map(|_| Some(payload.clone()))
            .or(Some(None)),
    }
}

/// Re-attach audited Gemini part extensions on same-format response rebuilds.
/// Cross-format validation runs before this emitter and blocks unrepresentable
/// provider fields, while native Gemini fields remain available to a Gemini
/// client instead of being silently discarded.
fn with_gemini_part_extensions(
    mut part: Value,
    extensions: &std::collections::BTreeMap<String, Value>,
) -> Value {
    if let Some(object) = part.as_object_mut() {
        let existing = object.clone();
        object.extend(namespace_extension_object(extensions, "gemini", &existing));
    }
    part
}

fn canonical_media_to_gemini_part(
    media_type: &str,
    data: Option<&str>,
    url: Option<&str>,
) -> Value {
    if let Some(data) = data.filter(|value| !value.is_empty()) {
        return json!({
            "inlineData": {
                "mimeType": media_type,
                "data": data,
            }
        });
    }
    json!({
        "fileData": {
            "mimeType": media_type,
            "fileUri": url.unwrap_or_default(),
        }
    })
}

fn gemini_function_args(input: &Value) -> Value {
    match input {
        Value::Object(_) => input.clone(),
        Value::Null => json!({}),
        other => json!({ "value": other.clone() }),
    }
}

fn gemini_function_response(output: Option<&Value>, content_text: Option<&str>) -> Value {
    match output {
        Some(Value::Object(object)) => Value::Object(object.clone()),
        Some(value) => json!({ "result": value }),
        None => json!({ "result": content_text.unwrap_or_default() }),
    }
}

fn gemini_function_response_part(
    tool_use_id: &str,
    name: &str,
    output: Option<&Value>,
    content_text: Option<&str>,
    extensions: &std::collections::BTreeMap<String, Value>,
) -> Value {
    let mut function_response = extensions
        .get("gemini")
        .and_then(|value| value.get("raw_function_response"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    function_response
        .entry("id".to_string())
        .or_insert_with(|| Value::String(tool_use_id.to_string()));
    function_response
        .entry("name".to_string())
        .or_insert_with(|| Value::String(name.to_string()));
    function_response
        .entry("response".to_string())
        .or_insert_with(|| gemini_function_response(output, content_text));
    let mut part = Map::new();
    part.insert(
        "functionResponse".to_string(),
        Value::Object(function_response),
    );
    let mut extra =
        crate::protocol::canonical::namespace_extension_object(extensions, "gemini", &part);
    extra.remove("raw_function_response");
    part.extend(extra);
    Value::Object(part)
}

fn canonical_stop_reason_to_gemini(reason: Option<&CanonicalStopReason>) -> Value {
    Value::String(
        match reason {
            Some(CanonicalStopReason::MaxTokens) => "MAX_TOKENS",
            Some(CanonicalStopReason::ContentFiltered) | Some(CanonicalStopReason::Refusal) => {
                "SAFETY"
            }
            Some(CanonicalStopReason::Unknown) => "OTHER",
            _ => "STOP",
        }
        .to_string(),
    )
}

fn canonical_usage_to_gemini_usage_metadata(usage: &CanonicalUsage) -> Value {
    let input_tokens = canonical_usage_total_input_tokens(usage);
    let mut out = usage
        .extensions
        .get("gemini")
        .and_then(Value::as_object)
        .and_then(|value| value.get("raw_usage"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    out.insert("promptTokenCount".to_string(), Value::from(input_tokens));
    out.insert(
        "candidatesTokenCount".to_string(),
        Value::from(usage.output_tokens.saturating_sub(usage.reasoning_tokens)),
    );
    out.insert(
        "totalTokenCount".to_string(),
        Value::from(canonical_usage_total_tokens_for_inclusive_input(
            usage,
            input_tokens,
        )),
    );
    if usage.cache_read_tokens > 0 {
        out.insert(
            "cachedContentTokenCount".to_string(),
            Value::from(usage.cache_read_tokens),
        );
    }
    if usage.reasoning_tokens > 0 {
        out.insert(
            "thoughtsTokenCount".to_string(),
            Value::from(usage.reasoning_tokens),
        );
    }
    if let Some(gemini) = usage.extensions.get("gemini").and_then(Value::as_object) {
        for (key, value) in gemini {
            if key != "raw_usage" {
                out.entry(key.clone()).or_insert(value.clone());
            }
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CanonicalContentBlock;

    /// 引用属于原始文本 part；签名控制块被过滤后仍保持目标与字符偏移，不生成重复引用。
    #[test]
    fn grounding_preserves_original_part_indices_and_snake_case_fields() {
        let body = json!({
            "candidates": [{
                "content": {"parts": [
                    {"thoughtSignature": "opaque-control"},
                    {"text": "前言"},
                    {"text": "今天是 2026 年"}
                ]},
                "finishReason": "STOP",
                "grounding_metadata": {
                    "webSearchQueries": ["current date"],
                    "grounding_chunks": [{"web": {"uri": "https://time.gov/", "title": "time.gov"}}],
                    "grounding_supports": [{
                        "segment": {"part_index": 2, "start_index": 0, "end_index": 15},
                        "grounding_chunk_indices": [0]
                    }]
                }
            }]
        });
        let canonical = from_raw(&body).expect("canonical");
        let blocks = &canonical.outputs[0].content;
        let first = blocks
            .iter()
            .find_map(|block| match block {
                CanonicalContentBlock::Text { text, extensions } if text == "前言" => {
                    Some(extensions)
                }
                _ => None,
            })
            .unwrap();
        assert!(!first.contains_key(OPENAI_RESPONSES_EXTENSION_NAMESPACE));
        let cited = blocks
            .iter()
            .find_map(|block| match block {
                CanonicalContentBlock::Text { text, extensions } if text == "今天是 2026 年" => {
                    Some(extensions)
                }
                _ => None,
            })
            .unwrap();
        let annotations = cited[OPENAI_RESPONSES_EXTENSION_NAMESPACE]["annotations"]
            .as_array()
            .unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0]["end_index"], 9);
        assert_eq!(
            cited[CLAUDE_EXTENSION_NAMESPACE]["citations"][0]["cited_text"],
            "今天是 2026"
        );
        let responses =
            crate::formats::openai::responses::response::to_raw(&canonical, &json!({}), false);
        let text = responses["output"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["type"] == "message")
            .unwrap();
        assert_eq!(text["content"][0]["annotations"][0]["start_index"], 2);
        assert_eq!(text["content"][0]["annotations"][0]["end_index"], 11);
    }

    /// Gemini omits `groundingSupports` when it cannot anchor the answer to a
    /// span. The sources are still real, so they must survive unanchored
    /// rather than be dropped for lacking offsets.
    #[test]
    fn grounding_without_supports_still_yields_unanchored_citations() {
        let body = json!({
            "responseId": "resp-unanchored",
            "candidates": [{
                "content": {"role": "model", "parts": [{"text": "Rust 1.95 is current."}]},
                "finishReason": "STOP",
                "groundingMetadata": {
                    "groundingChunks": [
                        {"web": {"uri": "https://blog.rust-lang.org/", "title": "Rust Blog"}},
                        {"web": {"title": "no uri here"}}
                    ]
                }
            }]
        });

        let canonical = from_raw(&body).expect("canonical");
        let CanonicalContentBlock::Text { extensions, .. } = &canonical.outputs[0].content[0]
        else {
            panic!("expected a text block");
        };

        assert_eq!(
            extensions["claude"]["citations"],
            json!([{
                "type": "web_search_result_location",
                "url": "https://blog.rust-lang.org/",
                "title": "Rust Blog",
            }])
        );
        assert_eq!(
            extensions["openai_responses"]["annotations"],
            json!([{
                "type": "url_citation",
                "url": "https://blog.rust-lang.org/",
                "title": "Rust Blog",
            }])
        );
    }

    #[test]
    fn gemini_response_without_visible_parts_is_not_success() {
        let body = json!({
            "candidates": [{
                "content": {"role": "model"},
                "finishReason": "MAX_TOKENS"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 1,
                "thoughtsTokenCount": 25,
                "totalTokenCount": 34
            },
            "modelVersion": "gemini-3-flash-preview",
            "responseId": "resp-empty"
        });

        assert!(from_raw(&body).is_none());
    }

    #[test]
    fn gemini_response_with_only_thought_parts_is_valid() {
        let body = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "hidden plan", "thought": true}]
                },
                "finishReason": "MAX_TOKENS"
            }],
            "modelVersion": "gemini-3-flash-preview",
            "responseId": "resp-thought-only"
        });

        let canonical = from_raw(&body).expect("thought-only response should remain valid");
        assert!(matches!(
            canonical.content.first(),
            Some(CanonicalContentBlock::Thinking { text, .. }) if text == "hidden plan"
        ));
        assert!(matches!(
            canonical.stop_reason,
            Some(CanonicalStopReason::MaxTokens)
        ));

        let openai = crate::canonical_to_openai_chat_response(&canonical);
        assert_eq!(
            openai["choices"][0]["message"]["reasoning_content"],
            "hidden plan"
        );
        assert_eq!(openai["choices"][0]["finish_reason"], "length");
    }

    #[test]
    fn gemini_response_with_signature_only_reasoning_exhaustion_is_success() {
        let body = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "",
                        "thoughtSignature": "opaque-thought-signature"
                    }]
                },
                "finishReason": "MAX_TOKENS"
            }],
            "usageMetadata": {
                "promptTokenCount": 22,
                "thoughtsTokenCount": 29,
                "totalTokenCount": 51
            },
            "modelVersion": "gemini-3.7-flash-tiered",
            "responseId": "resp-signature-only"
        });

        let canonical = from_raw(&body).expect("reasoning exhaustion is a valid terminal");
        assert!(matches!(
            canonical.content.first(),
            Some(CanonicalContentBlock::Thinking {
                text,
                signature: Some(signature),
                ..
            }) if text.is_empty() && signature == "opaque-thought-signature"
        ));
        assert!(matches!(
            canonical.stop_reason,
            Some(CanonicalStopReason::MaxTokens)
        ));
        assert_eq!(
            canonical.usage.as_ref().map(|usage| usage.reasoning_tokens),
            Some(29)
        );

        let openai = crate::canonical_to_openai_chat_response(&canonical);
        assert_eq!(openai["choices"][0]["finish_reason"], "length");
        assert_eq!(
            openai["usage"]["completion_tokens_details"]["reasoning_tokens"],
            29
        );
    }

    #[test]
    fn gemini_signature_only_terminal_without_reasoning_usage_is_not_success() {
        let body = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "",
                        "thoughtSignature": "opaque-thought-signature"
                    }]
                },
                "finishReason": "MAX_TOKENS"
            }],
            "usageMetadata": {
                "promptTokenCount": 22,
                "thoughtsTokenCount": 0,
                "totalTokenCount": 22
            }
        });

        assert!(from_raw(&body).is_none());
    }

    #[test]
    fn gemini_response_with_function_call_is_visible_output() {
        let body = json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "lookup",
                            "args": {"query": "weather"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "modelVersion": "gemini-3-flash-preview",
            "responseId": "resp-tool"
        });

        let canonical = from_raw(&body).expect("function call should be visible output");
        assert!(matches!(
            canonical.content.first(),
            Some(CanonicalContentBlock::ToolUse { name, .. }) if name == "lookup"
        ));
    }
}
