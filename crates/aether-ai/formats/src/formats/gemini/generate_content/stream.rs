use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::partial_args::VertexPartialArgsAssembler;
use crate::formats::gemini::generate_content::response::{
    gemini_candidate_grounding, gemini_grounding_citations,
};
use crate::formats::shared::response::{build_generated_tool_call_id, canonicalize_tool_arguments};
use crate::formats::shared::sse::encode_json_sse;
use crate::formats::shared::stream_core::common::*;
use crate::formats::shared::{AiSurfaceFinalizeError, GeminiStreamWireMode};
use crate::protocol::canonical::audit_gemini_cross_format_response_part;

/// Vertex 分片参数调用（`functionCall.partialArgs`）的组装与输出状态。
#[derive(Default)]
struct GeminiPartialCallState {
    assembler: VertexPartialArgsAssembler,
    /// Start 事件发出前缓冲的参数增量。
    pending_deltas: String,
    /// 已输出的 thoughtSignature；签名仍须先于 Start，因此可能延后补发。
    signature_emitted: String,
    /// 调用级 willContinue 已结束；之后不再接受同一调用的分片。
    finalized: bool,
    /// 该分片调用所属候选；用于调用结束后的活动关联恢复。
    candidate_key: Option<u64>,
}

#[derive(Default)]
/// 累积单个 Gemini provider 工具调用的流式身份、参数与签名状态。
struct GeminiProviderToolState {
    call_id: String,
    name: String,
    arguments: String,
    /// 最近一次已发出的函数调用 thoughtSignature；用于去重并保证先于调用开始事件。
    thought_signature: String,
    started_emitted: bool,
    /// 分片参数调用的组装状态；完整 `args` 调用保持为 None。
    partial: Option<GeminiPartialCallState>,
}

#[derive(Default)]
struct GeminiProviderToolResultState {
    content: String,
    emitted: bool,
}

#[derive(Default)]
pub struct GeminiProviderState {
    terminal_observation_only: bool,
    observed_tool_calls: bool,
    response_id: Option<String>,
    model: Option<String>,
    started: bool,
    finished: bool,
    text_parts: BTreeMap<usize, String>,
    reasoning_parts: BTreeMap<usize, String>,
    reasoning_signatures: BTreeMap<usize, String>,
    content_parts: BTreeMap<usize, CanonicalContentPart>,
    tool_calls: BTreeMap<usize, GeminiProviderToolState>,
    tool_results: BTreeMap<usize, GeminiProviderToolResultState>,
    /// 下一个工具调用索引；只增不减，保证分片续写与完整调用的索引稳定。
    next_tool_call_index: usize,
    /// 显式 id -> 分片调用索引，仅用于把后续分片关联到同一调用。
    partial_call_ids: BTreeMap<String, usize>,
    /// 候选索引 -> 该候选当前仍在延续的无 id 分片调用。
    active_partial_calls: BTreeMap<u64, usize>,
    /// 已观测到的规范 usage；Gemini usageMetadata 为累计值，只覆盖非零字段。
    known_usage: Option<CanonicalUsage>,
    /// Last `groundingMetadata` seen. Gemini resends it cumulatively, so the
    /// newest copy is the complete one; citations are emitted once at finish,
    /// when the answer text they index into is whole.
    grounding: Option<Value>,
}

impl GeminiProviderState {
    pub(crate) fn terminal_observation() -> Self {
        Self {
            terminal_observation_only: true,
            ..Self::default()
        }
    }

    fn identity(&self, report_context: &Value) -> (String, String) {
        resolve_identity(
            self.response_id.as_deref(),
            self.model.as_deref(),
            report_context,
            "resp-local-stream",
        )
    }

    /// 已观测到的规范 usage（最近一次含 usage 的 Gemini 事件，非零字段合并）。
    pub(crate) fn known_usage(&self) -> Option<CanonicalUsage> {
        self.known_usage.clone()
    }

    fn ensure_started(&mut self, report_context: &Value, out: &mut Vec<CanonicalStreamFrame>) {
        if self.started {
            return;
        }
        let (id, model) = self.identity(report_context);
        out.push(CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::Start,
        });
        self.started = true;
    }

    /// Turn the grounding metadata collected over the stream into citations.
    ///
    /// The offsets Gemini reports index into the finished answer, so this can
    /// only run once the text is complete — hence a single frame just ahead of
    /// `Finish` rather than a delta per chunk.
    fn push_citations_frame(&mut self, id: &str, model: &str, out: &mut Vec<CanonicalStreamFrame>) {
        let Some(grounding) = self.grounding.take() else {
            return;
        };
        // 流式客户端合并文本输出，因此将各 part 的字符区间平移到完整答案。
        let mut citations = Vec::new();
        let mut text_offset = 0;
        for (part_index, text) in &self.text_parts {
            let mut part_citations = gemini_grounding_citations(&grounding, text, *part_index);
            let unanchored = !part_citations.is_empty()
                && part_citations
                    .iter()
                    .all(|citation| citation.get("start_index").is_none());
            for citation in &mut part_citations {
                for key in ["start_index", "end_index"] {
                    if let Some(index) = citation.get(key).and_then(Value::as_u64) {
                        citation[key] = Value::from(index + text_offset);
                    }
                }
            }
            citations.extend(part_citations);
            if unanchored {
                break;
            }
            text_offset += text.chars().count() as u64;
        }
        if citations.is_empty() {
            return;
        }
        out.push(CanonicalStreamFrame {
            id: id.to_string(),
            model: model.to_string(),
            event: CanonicalStreamEvent::Citations(citations),
        });
    }

    fn unknown_frame(&self, report_context: &Value, payload: Value) -> CanonicalStreamFrame {
        let (id, model) = self.identity(report_context);
        CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::UnknownEvent(payload),
        }
    }

    /// 解析一行 Gemini provider 数据；函数调用签名先于调用事件输出，已知工具终止错误完整透传。
    pub fn push_line(
        &mut self,
        report_context: &Value,
        line: Vec<u8>,
    ) -> Result<Vec<CanonicalStreamFrame>, AiSurfaceFinalizeError> {
        let Some(value) = decode_json_data_line(&line) else {
            return Ok(Vec::new());
        };
        let Some(raw_event_object) = value.as_object() else {
            return Ok(Vec::new());
        };
        if let Some(id) = raw_event_object.get("responseId").and_then(Value::as_str) {
            self.response_id = Some(id.to_string());
        }
        let event_object = raw_event_object
            .get("response")
            .and_then(Value::as_object)
            .filter(|response| response.contains_key("candidates"))
            .unwrap_or(raw_event_object);
        if let Some(id) = event_object
            .get("responseId")
            .or_else(|| event_object.get("_v1internal_response_id"))
            .and_then(Value::as_str)
        {
            self.response_id = Some(id.to_string());
        }
        if let Some(version) = event_object.get("modelVersion").and_then(Value::as_str) {
            self.model = Some(version.to_string());
        }
        // usageMetadata 在事件解析前记录（含 terminal observation 模式），
        // 供 stream_core 直接读取，避免对同一数据行二次 JSON 解码。
        if let Some(usage) = canonical_usage_from_gemini_usage(event_object.get("usageMetadata"))
            .or_else(|| canonical_usage_from_gemini_usage(raw_event_object.get("usageMetadata")))
        {
            merge_known_usage(&mut self.known_usage, usage);
        }

        let mut out = Vec::new();
        // 同一事件可能包含多个候选：先处理完所有候选的关闭标记，再统一做终态校验。
        let mut pending_finish_reasons: Vec<String> = Vec::new();
        let cross_format = report_context
            .get("client_api_format")
            .and_then(Value::as_str)
            .map(crate::formats::id::normalize_api_format_alias)
            .is_some_and(|format| format != "gemini:generate_content");
        if cross_format
            && event_object.keys().any(|field| {
                !matches!(
                    field.as_str(),
                    "responseId"
                        | "_v1internal_response_id"
                        | "modelVersion"
                        | "candidates"
                        | "usageMetadata"
                )
            })
        {
            out.push(self.unknown_frame(report_context, Value::Object(event_object.clone())));
            return Ok(out);
        }
        if cross_format {
            if let Some(usage) = event_object.get("usageMetadata") {
                let Some(usage) = usage.as_object() else {
                    out.push(self.unknown_frame(report_context, usage.clone()));
                    return Ok(out);
                };
                if usage.keys().any(|field| {
                    !matches!(
                        field.as_str(),
                        "promptTokenCount"
                            | "prompt_token_count"
                            | "toolUsePromptTokenCount"
                            | "tool_use_prompt_token_count"
                            | "cachedContentTokenCount"
                            | "cached_content_token_count"
                            | "candidatesTokenCount"
                            | "candidates_token_count"
                            | "thoughtsTokenCount"
                            | "thoughts_token_count"
                            | "totalTokenCount"
                            | "total_token_count"
                    )
                }) {
                    out.push(self.unknown_frame(report_context, Value::Object(usage.clone())));
                    return Ok(out);
                }
            }
        }
        let Some(candidates) = event_object.get("candidates").and_then(Value::as_array) else {
            out.push(self.unknown_frame(report_context, value.clone()));
            return Ok(out);
        };

        for (candidate_position, candidate) in candidates.iter().enumerate() {
            let Some(candidate_object) = candidate.as_object() else {
                out.push(self.unknown_frame(report_context, candidate.clone()));
                return Ok(out);
            };
            // 分片工具调用按候选关联；缺 index 的候选退回出现顺序。
            let candidate_key = candidate_object
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(candidate_position as u64);
            if cross_format
                && candidate_object.keys().any(|field| {
                    !matches!(
                        field.as_str(),
                        "index"
                            | "content"
                            | "finishReason"
                            | "finish_reason"
                            // Gemini 会随异常终止原因附带说明；它是已知元数据，不应触发未知字段拒绝。
                            | "finishMessage"
                            | "finish_message"
                            // 联网搜索来源已映射为各客户端的引用事件。
                            | "groundingMetadata"
                            | "grounding_metadata"
                    )
                })
            {
                out.push(self.unknown_frame(report_context, candidate.clone()));
                return Ok(out);
            }
            let (response_id, response_model) = self.identity(report_context);
            let terminal_error = gemini_stream_terminal_error_payload(
                candidate_object,
                response_id.as_str(),
                response_model.as_str(),
                event_object.get("usageMetadata"),
            );
            if !self.terminal_observation_only {
                if let Some(grounding) = gemini_candidate_grounding(candidate_object) {
                    self.grounding = Some(grounding.clone());
                }
            }
            let empty_content = Map::new();
            let content = match candidate_object.get("content") {
                Some(content) => {
                    let Some(content) = content.as_object() else {
                        // 已知终态错误载荷比整候选未知帧更权威；无终态错误时仍按未知结构 fail closed。
                        if let Some(payload) = terminal_error {
                            out.push(self.unknown_frame(report_context, payload));
                            self.finished = true;
                            continue;
                        }
                        out.push(self.unknown_frame(report_context, candidate.clone()));
                        return Ok(out);
                    };
                    content
                }
                None => &empty_content,
            };
            if cross_format
                && content
                    .keys()
                    .any(|field| !matches!(field.as_str(), "role" | "parts"))
            {
                out.push(self.unknown_frame(report_context, Value::Object(content.clone())));
                return Ok(out);
            }
            let empty_parts = Vec::new();
            let parts = match content.get("parts") {
                Some(parts) => {
                    let Some(parts) = parts.as_array() else {
                        out.push(
                            self.unknown_frame(report_context, Value::Object(content.clone())),
                        );
                        return Ok(out);
                    };
                    parts
                }
                None => &empty_parts,
            };
            if !parts.is_empty() {
                self.ensure_started(report_context, &mut out);
            }
            let (id, model) = self.identity(report_context);
            for (index, part) in parts.iter().enumerate() {
                let Some(part_object) = part.as_object() else {
                    out.push(self.unknown_frame(report_context, part.clone()));
                    return Ok(out);
                };
                if cross_format
                    && audit_gemini_cross_format_response_part(part_object, true).is_err()
                {
                    // 已知错误终态比同帧不可回放的空签名 part 更权威；否则仍按未知字段 fail closed。
                    if terminal_error.is_none() {
                        out.push(self.unknown_frame(report_context, part.clone()));
                        break;
                    }
                    continue;
                }
                let reasoning_signature = if self.terminal_observation_only {
                    None
                } else {
                    part_object
                        .get("thoughtSignature")
                        .or_else(|| part_object.get("thought_signature"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                };
                if let Some(text) = render_gemini_part_as_text(part_object) {
                    if self.terminal_observation_only {
                        continue;
                    }
                    let is_reasoning = part_object
                        .get("thought")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        || (text.trim().is_empty() && reasoning_signature.is_some());
                    let previous = if is_reasoning {
                        self.reasoning_parts.entry(index).or_default()
                    } else {
                        self.text_parts.entry(index).or_default()
                    };
                    let delta = if text.starts_with(previous.as_str()) {
                        text[previous.len()..].to_string()
                    } else if previous.as_str() == text {
                        String::new()
                    } else {
                        text.to_string()
                    };
                    *previous = text;
                    if !delta.is_empty() {
                        out.push(CanonicalStreamFrame {
                            id: id.clone(),
                            model: model.clone(),
                            event: if is_reasoning {
                                CanonicalStreamEvent::ReasoningDelta(delta)
                            } else {
                                CanonicalStreamEvent::TextDelta(delta)
                            },
                        });
                    }
                    if is_reasoning {
                        if let Some(signature) = reasoning_signature.as_ref() {
                            let previous_signature =
                                self.reasoning_signatures.entry(index).or_default();
                            if previous_signature.as_str() != signature.as_str() {
                                *previous_signature = signature.clone();
                                out.push(CanonicalStreamFrame {
                                    id: id.clone(),
                                    model: model.clone(),
                                    event: CanonicalStreamEvent::ReasoningSignature(
                                        signature.clone(),
                                    ),
                                });
                            }
                        }
                    }
                    continue;
                }
                if let Some(function_response) = part_object
                    .get("functionResponse")
                    .or_else(|| part_object.get("function_response"))
                    .and_then(Value::as_object)
                {
                    if self.terminal_observation_only {
                        continue;
                    }
                    if function_response
                        .keys()
                        .any(|field| !matches!(field.as_str(), "id" | "name" | "response"))
                    {
                        out.push(self.unknown_frame(report_context, part.clone()));
                        continue;
                    }
                    let tool_use_id = function_response
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| build_generated_tool_call_id(index));
                    let name = function_response
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned);
                    let content = gemini_function_response_content(
                        function_response.get("response").unwrap_or(&Value::Null),
                    );
                    let state = self.tool_results.entry(index).or_default();
                    let delta = if !state.emitted {
                        content.clone()
                    } else if content.starts_with(&state.content) {
                        content[state.content.len()..].to_string()
                    } else if state.content == content {
                        String::new()
                    } else {
                        content.clone()
                    };
                    if !delta.is_empty() || !state.emitted {
                        state.emitted = true;
                        state.content.push_str(&delta);
                        out.push(CanonicalStreamFrame {
                            id: id.clone(),
                            model: model.clone(),
                            event: CanonicalStreamEvent::ToolResultDelta {
                                index,
                                tool_use_id,
                                name,
                                content: delta,
                            },
                        });
                    }
                    continue;
                }
                let Some(function_call) = part_object
                    .get("functionCall")
                    .or_else(|| part_object.get("function_call"))
                    .and_then(Value::as_object)
                else {
                    if let Some(content_part) = canonical_content_part_from_gemini_part(part_object)
                    {
                        if self.terminal_observation_only {
                            continue;
                        }
                        let should_emit = self
                            .content_parts
                            .get(&index)
                            .map(|existing| existing != &content_part)
                            .unwrap_or(true);
                        if should_emit {
                            self.content_parts.insert(index, content_part.clone());
                            out.push(CanonicalStreamFrame {
                                id: id.clone(),
                                model: model.clone(),
                                event: CanonicalStreamEvent::ContentPart(content_part),
                            });
                        }
                    } else {
                        out.push(
                            self.unknown_frame(report_context, Value::Object(part_object.clone())),
                        );
                    }
                    continue;
                };
                if self.terminal_observation_only {
                    self.observed_tool_calls = true;
                    continue;
                }
                // Vertex 分片参数（functionCall.partialArgs）由 jsonPath 组装器负责；
                // 完整 args 调用继续走下面的既有路径。
                match self.try_handle_partial_function_call(
                    candidate_key,
                    function_call,
                    reasoning_signature.as_deref(),
                    &id,
                    &model,
                    &mut out,
                ) {
                    Ok(true) => {
                        // 观测器对任何 functionCall part 都记为“见过工具调用”，
                        // 正常路径同样记录，保证 finish reason 与观测结果一致。
                        self.observed_tool_calls = true;
                        continue;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        // 分片参数非法：输出可路由的失败终态，而不是直接抛出截断流。
                        out.push(self.partial_args_failure_frame(
                            report_context,
                            &error.0,
                            event_object.get("usageMetadata"),
                        ));
                        self.finished = true;
                        return Ok(out);
                    }
                }
                // Gemini streams are incremental and every complete-args functionCall
                // part is a full call, so parallel calls arriving in separate chunks all
                // sit at parts[0]. Key calls by arrival order, not part position.
                // Ids cannot disambiguate: they are optional, and the Antigravity
                // envelope synthesizes per-chunk ids that repeat across chunks.
                let index = self.next_tool_call_index;
                self.next_tool_call_index += 1;
                let tool_state = self.tool_calls.entry(index).or_default();
                tool_state.call_id = function_call
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(tool_state.call_id.as_str())
                    .to_string();
                tool_state.name = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(tool_state.name.as_str())
                    .to_string();
                if let Some(signature) = reasoning_signature {
                    if tool_state.thought_signature != signature {
                        tool_state.thought_signature = signature.clone();
                        out.push(CanonicalStreamFrame {
                            id: id.clone(),
                            model: model.clone(),
                            event: CanonicalStreamEvent::ToolCallSignature { index, signature },
                        });
                    }
                }
                if !tool_state.started_emitted {
                    out.push(CanonicalStreamFrame {
                        id: id.clone(),
                        model: model.clone(),
                        event: CanonicalStreamEvent::ToolCallStart {
                            index,
                            call_id: if tool_state.call_id.is_empty() {
                                build_generated_tool_call_id(index)
                            } else {
                                tool_state.call_id.clone()
                            },
                            // canonical 保留空名：缺名不再伪造 "unknown"，
                            // 由目标客户端 emitter 在终态统一 fail-closed。
                            name: tool_state.name.clone(),
                        },
                    });
                    tool_state.started_emitted = true;
                }
                let arguments = canonicalize_tool_arguments(function_call.get("args").cloned());
                let delta = if arguments.starts_with(&tool_state.arguments) {
                    arguments[tool_state.arguments.len()..].to_string()
                } else if tool_state.arguments == arguments {
                    String::new()
                } else {
                    arguments.clone()
                };
                tool_state.arguments = arguments;
                if !delta.is_empty() {
                    out.push(CanonicalStreamFrame {
                        id: id.clone(),
                        model: model.clone(),
                        event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                            index,
                            arguments: delta,
                        },
                    });
                }
            }
            if let Some(payload) = terminal_error {
                out.push(self.unknown_frame(report_context, payload));
                self.finished = true;
                continue;
            }
            if let Some(finish_reason) = candidate_object
                .get("finishReason")
                .or_else(|| candidate_object.get("finish_reason"))
                .and_then(Value::as_str)
            {
                // 同事件的其他候选可能还携带本事件的关闭标记：延后到全部候选处理完再终态，
                // 避免把仍将在同一事件内结束的分片调用误判为截断。
                pending_finish_reasons.push(finish_reason.to_string());
            }
        }

        if !pending_finish_reasons.is_empty() {
            // 权威终态错误已置 finished 时不再追加二次失败；否则此时所有候选的关闭标记
            // 都已消费，可以安全地全局校验是否仍有未结束的分片调用。
            if !self.finished {
                if let Err(error) = self.ensure_partial_calls_closed() {
                    out.push(self.partial_args_failure_frame(
                        report_context,
                        &error.0,
                        event_object.get("usageMetadata"),
                    ));
                    self.finished = true;
                    return Ok(out);
                }
            }
            for finish_reason in pending_finish_reasons {
                let (id, model) = self.identity(report_context);
                let has_tool_calls = self.observed_tool_calls || !self.tool_calls.is_empty();
                let mut finish_reason =
                    normalize_openai_finish_reason(map_gemini_stream_finish_reason(&finish_reason));
                if has_tool_calls && finish_reason.as_deref().is_none_or(|value| value == "stop") {
                    finish_reason = Some("tool_calls".to_string());
                }
                self.push_citations_frame(&id, &model, &mut out);
                out.push(CanonicalStreamFrame {
                    id,
                    model,
                    event: CanonicalStreamEvent::Finish {
                        finish_reason,
                        usage: canonical_usage_from_gemini_usage(event_object.get("usageMetadata")),
                    },
                });
                self.finished = true;
            }
        }

        Ok(out)
    }

    /// 处理 Vertex 分片参数 functionCall；返回 true 表示已由本路径消费。
    ///
    /// 关联规则：显式 id 只匹配同 id 的分片调用；无 id 分片只续写该候选当前仍在
    /// 延续的调用。已结束调用不再接受分片，避免把新的并行调用并进旧调用。
    fn try_handle_partial_function_call(
        &mut self,
        candidate_key: u64,
        function_call: &Map<String, Value>,
        reasoning_signature: Option<&str>,
        frame_id: &str,
        frame_model: &str,
        out: &mut Vec<CanonicalStreamFrame>,
    ) -> Result<bool, AiSurfaceFinalizeError> {
        let partial_args = function_call
            .get("partialArgs")
            .or_else(|| function_call.get("partial_args"));
        let call_will_continue = match function_call
            .get("willContinue")
            .or_else(|| function_call.get("will_continue"))
        {
            Some(value) => Some(value.as_bool().ok_or_else(|| {
                AiSurfaceFinalizeError::new("Gemini functionCall.willContinue 必须是布尔值")
            })?),
            None => None,
        };
        if function_call.contains_key("args") {
            if partial_args.is_some() {
                return Err(AiSurfaceFinalizeError::new(
                    "Gemini functionCall 不能同时携带 args 与 partialArgs",
                ));
            }
            return Ok(false);
        }
        let explicit_id = function_call
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let name = function_call
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        // 只有 id/name、没有参数与 willContinue 的 functionCall：可能是一次分片调用的
        // 终结标记（仅 id），也可能是既有语义下的无参完整调用（带 name）。
        let bare =
            partial_args.is_none() && call_will_continue.is_none() && !function_call.is_empty();

        let mut target = match explicit_id {
            Some(id) => self.partial_call_ids.get(id).copied(),
            None => self.active_partial_calls.get(&candidate_key).copied(),
        };
        if target
            .and_then(|index| self.tool_calls.get(&index))
            .and_then(|state| state.partial.as_ref())
            .is_none_or(|partial| partial.finalized)
        {
            // 目标已结束或不是分片调用：显式 id 可能被上游逐块复用，按新调用处理。
            target = None;
        }

        if bare {
            if target.is_none() {
                // 没有活动分片调用：带 name 的沿用既有（无参）完整调用语义输出 `{}`，
                // 无 name 的游离标记忽略，绝不产出空成功调用。
                return Ok(name.is_none());
            }
            // 命中活动分片调用时，即使重复带 name 也只是该调用的终结标记。
        } else if target.is_none() && partial_args.is_none() && call_will_continue != Some(true) {
            // 没有活动分片调用时的终结标记：带 name 的是合法无参完整调用，交由既有路径
            // 输出 `{}`；无 name 的游离标记不伪造成成功调用。
            return Ok(name.is_none());
        }
        let index = match target {
            Some(index) => index,
            None => self.start_partial_call(candidate_key, explicit_id, name),
        };

        let (name_conflict, id_changed, started_emitted) = {
            let state = self
                .tool_calls
                .get_mut(&index)
                .ok_or_else(|| AiSurfaceFinalizeError::new("Gemini 分片函数调用状态丢失"))?;
            let mut name_conflict = false;
            let mut id_changed = false;
            if let Some(name) = name {
                if state.name.is_empty() {
                    state.name = name.to_string();
                } else if state.name != name {
                    name_conflict = true;
                }
            }
            if state.call_id.is_empty() && explicit_id.is_some() {
                state.call_id = explicit_id.unwrap_or_default().to_string();
                id_changed = true;
            }
            if let Some(signature) = reasoning_signature {
                state.thought_signature = signature.to_string();
            }
            (name_conflict, id_changed, state.started_emitted)
        };
        if name_conflict {
            return Err(AiSurfaceFinalizeError::new(
                "Gemini 分片函数调用在流中改名为不一致的函数",
            ));
        }
        if id_changed {
            if let Some(id) = explicit_id {
                self.partial_call_ids.insert(id.to_string(), index);
            }
        }
        if started_emitted {
            // Start 已输出时签名可以立即追加；否则由 flush 保证签名先于 Start。
            self.emit_partial_signature(index, frame_id, frame_model, out);
        }

        if let Some(entries_value) = partial_args {
            let entries = entries_value.as_array().ok_or_else(|| {
                AiSurfaceFinalizeError::new("Gemini functionCall.partialArgs 必须是数组")
            })?;
            let delta = {
                let partial = self
                    .tool_calls
                    .get_mut(&index)
                    .and_then(|state| state.partial.as_mut())
                    .ok_or_else(|| AiSurfaceFinalizeError::new("Gemini 分片函数调用状态丢失"))?;
                partial
                    .assembler
                    .push_entries(entries)
                    .map_err(AiSurfaceFinalizeError::new)?
            };
            if !delta.is_empty() {
                let partial = self
                    .tool_calls
                    .get_mut(&index)
                    .and_then(|state| state.partial.as_mut())
                    .ok_or_else(|| AiSurfaceFinalizeError::new("Gemini 分片函数调用状态丢失"))?;
                partial.pending_deltas.push_str(&delta);
            }
        }

        let open_string = self
            .tool_calls
            .get(&index)
            .and_then(|state| state.partial.as_ref())
            .is_some_and(|partial| partial.assembler.has_open_strings());
        // 调用级 willContinue 缺失时按单块完整调用处理；空 functionCall 与 willContinue:false
        // 是显式终结标记，仍在延续的字符串会因此在终态校验中失败。
        let should_finalize = match call_will_continue {
            Some(true) => false,
            Some(false) => true,
            None => function_call.is_empty() || bare || !open_string,
        };

        self.flush_partial_call_frames(index, should_finalize, frame_id, frame_model, out);
        if should_finalize {
            self.finalize_partial_call(index, frame_id, frame_model, out)?;
        }
        Ok(true)
    }

    fn start_partial_call(
        &mut self,
        candidate_key: u64,
        explicit_id: Option<&str>,
        name: Option<&str>,
    ) -> usize {
        let index = self.next_tool_call_index;
        self.next_tool_call_index += 1;
        let state = self.tool_calls.entry(index).or_default();
        state.call_id = explicit_id.unwrap_or_default().to_string();
        state.name = name.unwrap_or_default().to_string();
        state.partial = Some(GeminiPartialCallState {
            candidate_key: Some(candidate_key),
            ..GeminiPartialCallState::default()
        });
        if let Some(id) = explicit_id {
            self.partial_call_ids.insert(id.to_string(), index);
        }
        self.active_partial_calls.insert(candidate_key, index);
        index
    }

    /// 补发尚未输出的函数调用签名；同一签名只输出一次。
    fn emit_partial_signature(
        &mut self,
        index: usize,
        frame_id: &str,
        frame_model: &str,
        out: &mut Vec<CanonicalStreamFrame>,
    ) {
        let Some(state) = self.tool_calls.get_mut(&index) else {
            return;
        };
        if state.thought_signature.is_empty() {
            return;
        }
        let Some(partial) = state.partial.as_mut() else {
            return;
        };
        if partial.signature_emitted == state.thought_signature {
            return;
        }
        let signature = state.thought_signature.clone();
        partial.signature_emitted = signature.clone();
        out.push(CanonicalStreamFrame {
            id: frame_id.to_string(),
            model: frame_model.to_string(),
            event: CanonicalStreamEvent::ToolCallSignature { index, signature },
        });
    }

    /// Start 在函数名已知前不发射；参数增量在其后统一补发，保证事件顺序稳定。
    fn flush_partial_call_frames(
        &mut self,
        index: usize,
        force_start: bool,
        frame_id: &str,
        frame_model: &str,
        out: &mut Vec<CanonicalStreamFrame>,
    ) {
        let (has_pending, will_start) = match self.tool_calls.get(&index) {
            Some(state) => (
                state
                    .partial
                    .as_ref()
                    .is_some_and(|partial| !partial.pending_deltas.is_empty()),
                !state.started_emitted && (force_start || !state.name.trim().is_empty()),
            ),
            None => return,
        };
        if !has_pending {
            return;
        }
        if will_start
            || self
                .tool_calls
                .get(&index)
                .is_some_and(|state| state.started_emitted)
        {
            // 签名必须早于调用开始事件，即使 Start 因缺名被延后。
            self.emit_partial_signature(index, frame_id, frame_model, out);
        }
        let Some(state) = self.tool_calls.get_mut(&index) else {
            return;
        };
        if !state.started_emitted && will_start {
            out.push(CanonicalStreamFrame {
                id: frame_id.to_string(),
                model: frame_model.to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index,
                    call_id: if state.call_id.is_empty() {
                        build_generated_tool_call_id(index)
                    } else {
                        state.call_id.clone()
                    },
                    name: state.name.clone(),
                },
            });
            state.started_emitted = true;
        }
        if state.started_emitted {
            let Some(partial) = state.partial.as_mut() else {
                return;
            };
            let arguments = std::mem::take(&mut partial.pending_deltas);
            out.push(CanonicalStreamFrame {
                id: frame_id.to_string(),
                model: frame_model.to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta { index, arguments },
            });
        }
    }

    /// 终结分片调用：补齐闭合结构并输出终态参数增量。
    /// 显式终结的零参数调用是合法无参调用，输出 `{}`；调用未终结（截断）由
    /// `ensure_partial_calls_closed` 拒绝，绝不补成成功调用。
    fn finalize_partial_call(
        &mut self,
        index: usize,
        frame_id: &str,
        frame_model: &str,
        out: &mut Vec<CanonicalStreamFrame>,
    ) -> Result<(), AiSurfaceFinalizeError> {
        let candidate_key = {
            let partial = self
                .tool_calls
                .get_mut(&index)
                .and_then(|state| state.partial.as_mut())
                .ok_or_else(|| AiSurfaceFinalizeError::new("Gemini 分片函数调用状态丢失"))?;
            if partial.finalized {
                return Ok(());
            }
            // 显式终结的零参数调用是合法无参调用：空组装结果序列化为 `{}` 正常发射。
            let suffix = partial
                .assembler
                .finish()
                .map_err(AiSurfaceFinalizeError::new)?;
            partial.finalized = true;
            partial.pending_deltas.push_str(&suffix);
            partial.candidate_key
        };
        if let Some(candidate_key) = candidate_key {
            self.restore_active_partial_call(candidate_key);
        }
        self.flush_partial_call_frames(index, true, frame_id, frame_model, out);
        self.partial_call_ids.retain(|_, value| *value != index);
        Ok(())
    }

    /// 把分片参数错误编码为 stream_core 可路由的失败终态。
    ///
    /// 直接返回错误会让上游流被截断而没有终态事件；这里生成 Responses 形状的
    /// `response.failed` 载荷，由共享 rewriter 转成各客户端格式的失败终态。
    fn partial_args_failure_frame(
        &self,
        report_context: &Value,
        message: &str,
        usage_metadata: Option<&Value>,
    ) -> CanonicalStreamFrame {
        let (id, model) = self.identity(report_context);
        let mut response = json!({
            "id": id.clone(),
            "object": "response",
            "model": model.clone(),
            "status": "failed",
            "error": {
                "type": "upstream_gemini_partial_args_error",
                "code": "INVALID_PARTIAL_ARGS",
                "message": message,
                "upstream_status": 200,
            }
        });
        if let Some(usage) = canonical_usage_from_gemini_usage(usage_metadata)
            .map(|usage| openai_responses_usage_from_usage(&usage))
        {
            response["usage"] = usage;
        }
        CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::UnknownEvent(json!({
                "type": "response.failed",
                "response": response,
            })),
        }
    }

    /// 调用结束后恢复候选的活动分片关联：仅当该候选仍恰好有一个未结束调用时关联，
    /// 多个仍开放则无法无歧义关联无 id 分片，保持不猜测。
    fn restore_active_partial_call(&mut self, candidate_key: u64) {
        self.active_partial_calls.remove(&candidate_key);
        let open_calls = self
            .tool_calls
            .iter()
            .filter(|(_, state)| {
                state.partial.as_ref().is_some_and(|partial| {
                    !partial.finalized && partial.candidate_key == Some(candidate_key)
                })
            })
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        if let [only] = open_calls.as_slice() {
            self.active_partial_calls.insert(candidate_key, *only);
        }
    }

    /// 终态前仍有未结束的分片调用：判定为截断输入并终止。
    fn ensure_partial_calls_closed(&self) -> Result<(), AiSurfaceFinalizeError> {
        if self.tool_calls.values().any(|state| {
            state
                .partial
                .as_ref()
                .is_some_and(|partial| !partial.finalized)
        }) {
            return Err(AiSurfaceFinalizeError::new(
                "Gemini 流式函数调用分片参数在终态前仍未结束",
            ));
        }
        Ok(())
    }

    pub fn finish(
        &mut self,
        report_context: &Value,
    ) -> Result<Vec<CanonicalStreamFrame>, AiSurfaceFinalizeError> {
        if !self.started || self.finished {
            return Ok(Vec::new());
        }
        if let Err(error) = self.ensure_partial_calls_closed() {
            // EOF 时仍存在未结束的分片调用：输出失败终态，绝不补成正常 Finish。
            self.finished = true;
            return Ok(vec![self.partial_args_failure_frame(
                report_context,
                &error.0,
                None,
            )]);
        }
        self.finished = true;
        let (id, model) = self.identity(report_context);
        let mut out = Vec::new();
        self.push_citations_frame(&id, &model, &mut out);
        out.push(CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::Finish {
                finish_reason: None,
                usage: None,
            },
        });
        Ok(out)
    }
}

/// 把 Gemini 五类工具调用终止原因编码为完整 Responses 失败事件，并保留可用 usage。
fn gemini_stream_terminal_error_payload(
    candidate: &Map<String, Value>,
    response_id: &str,
    model: &str,
    usage_metadata: Option<&Value>,
) -> Option<Value> {
    let finish_reason = candidate
        .get("finishReason")
        .or_else(|| candidate.get("finish_reason"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| {
            matches!(
                *value,
                "MALFORMED_FUNCTION_CALL"
                    | "UNEXPECTED_TOOL_CALL"
                    | "TOO_MANY_TOOL_CALLS"
                    | "MISSING_THOUGHT_SIGNATURE"
                    | "MALFORMED_RESPONSE"
            )
        })?;
    let message = candidate
        .get("finishMessage")
        .or_else(|| candidate.get("finish_message"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Gemini stream ended with {finish_reason}"));

    let mut response = json!({
        "id": response_id,
        "object": "response",
        "model": model,
        "status": "failed",
        "error": {
            "type": "upstream_gemini_finish_error",
            "code": finish_reason,
            "message": message,
            "upstream_status": 200
        }
    });
    if let Some(usage) = canonical_usage_from_gemini_usage(usage_metadata)
        .map(|usage| openai_responses_usage_from_usage(&usage))
    {
        response["usage"] = usage;
    }

    Some(json!({
        "type": "response.failed",
        "response": response
    }))
}

/// 将 Gemini 正常终止原因映射到规范 finish reason，未知值留给上层 fail-closed。
fn map_gemini_stream_finish_reason(value: &str) -> Option<&str> {
    match value.trim().to_ascii_uppercase().as_str() {
        "STOP" => Some("stop"),
        "MAX_TOKENS" => Some("length"),
        "SAFETY"
        | "ESCALATION"
        | "RECITATION"
        | "LANGUAGE"
        | "BLOCKLIST"
        | "PROHIBITED_CONTENT"
        | "SPII"
        | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT"
        | "IMAGE_RECITATION" => Some("content_filter"),
        _ => Some(value),
    }
}

#[derive(Default)]
/// 累积向 Gemini 客户端回写的工具调用，直到参数足够完整再输出 part。
struct GeminiClientToolState {
    call_id: String,
    name: String,
    arguments: String,
    /// 随工具调用回放的 Gemini thoughtSignature；为空时不写入 wire part。
    thought_signature: String,
    emitted: bool,
}

#[derive(Default)]
pub struct GeminiClientEmitter {
    response_id: Option<String>,
    model: Option<String>,
    finished: bool,
    tool_calls: BTreeMap<usize, GeminiClientToolState>,
    wire_mode: GeminiStreamWireMode,
    json_array_started: bool,
    json_array_closed: bool,
    /// 终态校验失败后的毒化标记；一旦置位，后续 emit/finish 持续返回同一错误，
    /// 确保失败后绝不再输出正常 STOP 或补发工具调用。
    failure: Option<String>,
}

impl GeminiClientEmitter {
    /// Build a Gemini client emitter for the selected public wire framing.
    /// Upstream frames are identical; only the outer SSE/JSON-array envelope
    /// changes.
    pub fn with_wire_mode(wire_mode: GeminiStreamWireMode) -> Self {
        Self {
            wire_mode,
            ..Self::default()
        }
    }

    pub fn wire_mode(&self) -> GeminiStreamWireMode {
        self.wire_mode
    }

    fn update_identity(&mut self, frame: &CanonicalStreamFrame) {
        self.response_id = Some(frame.id.clone());
        self.model = Some(frame.model.clone());
    }

    fn emit_candidate(
        &mut self,
        parts: Vec<Value>,
        finish_reason: Option<&str>,
        usage: Option<CanonicalUsage>,
    ) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut candidate = Map::new();
        candidate.insert(
            "content".to_string(),
            json!({
                "role": "model",
                "parts": parts,
            }),
        );
        candidate.insert("index".to_string(), Value::from(0_u64));
        if let Some(finish_reason) = finish_reason {
            candidate.insert(
                "finishReason".to_string(),
                Value::String(map_openai_finish_reason_to_gemini(Some(finish_reason)).to_string()),
            );
        }
        let mut response = Map::new();
        response.insert(
            "responseId".to_string(),
            Value::String(
                self.response_id
                    .clone()
                    .unwrap_or_else(|| "resp-local-stream".to_string()),
            ),
        );
        response.insert(
            "modelVersion".to_string(),
            Value::String(self.model.clone().unwrap_or_else(|| "unknown".to_string())),
        );
        response.insert(
            "candidates".to_string(),
            Value::Array(vec![Value::Object(candidate)]),
        );
        if let Some(usage) = usage {
            response.insert(
                "usageMetadata".to_string(),
                gemini_usage_metadata_from_usage(&usage),
            );
        }
        self.encode_response_value(&Value::Object(response))
    }

    fn encode_response_value(&mut self, value: &Value) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        if self.wire_mode == GeminiStreamWireMode::Sse {
            return encode_json_sse(None, value);
        }

        let payload = serde_json::to_vec(value)?;
        let mut output = Vec::with_capacity(payload.len().saturating_add(1));
        if self.json_array_started {
            output.push(b',');
        } else {
            output.push(b'[');
            self.json_array_started = true;
        }
        output.extend(payload);
        Ok(output)
    }

    fn close_json_array(&mut self) -> Vec<u8> {
        if self.wire_mode != GeminiStreamWireMode::JsonArray || self.json_array_closed {
            return Vec::new();
        }
        self.json_array_closed = true;
        if self.json_array_started {
            b"]".to_vec()
        } else {
            self.json_array_started = true;
            b"[]".to_vec()
        }
    }

    /// Encode a structured client-facing error and terminate the selected
    /// wire envelope. This keeps JSON-array responses valid even when a
    /// conversion fails after one or more response objects were emitted.
    pub fn emit_error(&mut self, error_body: Value) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        if self.finished && self.json_array_closed {
            return Ok(Vec::new());
        }
        let mut output = self.encode_response_value(&error_body)?;
        self.finished = true;
        output.extend(self.close_json_array());
        Ok(output)
    }

    /// 记录终态校验失败并毒化后续输出；重复调用持续返回同一错误，
    /// 保证失败之后不会再输出正常 STOP 或补发任何工具调用。
    fn fail_closed(&mut self, message: String) -> AiSurfaceFinalizeError {
        self.failure = Some(message.clone());
        AiSurfaceFinalizeError::new(message)
    }

    /// 流式累积参数已成完整 JSON 对象时才允许提前发射；
    /// 标量（数字续写如 1→123）、数组、未闭合 JSON 一律继续缓冲到终态，
    /// 避免把可延续的值过早定型后又被增量改写。
    fn complete_object_arguments(arguments: &str) -> Option<Value> {
        let trimmed = arguments.trim();
        if trimmed.is_empty() {
            return None;
        }
        match parse_json_arguments_value(trimmed) {
            Some(value @ Value::Object(_)) => Some(value),
            _ => None,
        }
    }

    /// name 与完整对象参数齐备时构造待发射 part；任一不满足则继续缓冲。
    /// name 未到时缓冲而非报错，直到终态确认缺失才拒绝（分片次序无关）。
    fn ready_tool_call_part(index: usize, state: &GeminiClientToolState) -> Option<Value> {
        if state.emitted || state.name.trim().is_empty() {
            return None;
        }
        let args_value = Self::complete_object_arguments(&state.arguments)?;
        let mut part = json!({
            "functionCall": {
                "id": if state.call_id.is_empty() {
                    build_generated_tool_call_id(index)
                } else {
                    state.call_id.clone()
                },
                "name": state.name.clone(),
                "args": args_value,
            }
        });
        if !state.thought_signature.is_empty() {
            part["thoughtSignature"] = Value::String(state.thought_signature.clone());
        }
        Some(part)
    }

    /// 终态校验并刷新尚未输出的工具调用；签名附在对应 Gemini part 根节点。
    /// 缺名或参数非法（截断/非对象 JSON）时 fail-closed，绝不伪造 "unknown" 或 {}。
    fn flush_pending_tool_calls(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut pending = Vec::new();
        let mut failure = None;
        for (index, tool_call) in &mut self.tool_calls {
            if tool_call.emitted {
                continue;
            }
            // 终态仍缺失函数名：不得伪造身份，交由既有错误机制传给客户端。
            if tool_call.name.trim().is_empty() {
                failure = Some(format!(
                    "Gemini 流式工具调用（索引 {index}）在终态仍未获得函数名"
                ));
                break;
            }
            // 空白参数 = 合法无参调用输出 {}；非空参数必须是完整 JSON 对象
            // （Gemini functionCall.args 为 Struct），截断/标量/数组一律拒绝。
            let args_value = if tool_call.arguments.trim().is_empty() {
                Value::Object(Map::new())
            } else {
                match Self::complete_object_arguments(&tool_call.arguments) {
                    Some(value) => value,
                    None => {
                        // 错误消息不携带原始参数内容，仅定位 index 与原因。
                        failure = Some(format!(
                            "Gemini 流式工具调用（索引 {index}）终态参数不是完整 JSON 对象或已被截断"
                        ));
                        break;
                    }
                }
            };
            tool_call.emitted = true;
            let mut part = json!({
                "functionCall": {
                    "id": if tool_call.call_id.is_empty() {
                        build_generated_tool_call_id(*index)
                    } else {
                        tool_call.call_id.clone()
                    },
                    "name": tool_call.name.clone(),
                    "args": args_value,
                }
            });
            if !tool_call.thought_signature.is_empty() {
                part["thoughtSignature"] = Value::String(tool_call.thought_signature.clone());
            }
            pending.push(part);
        }
        if let Some(message) = failure {
            return Err(self.fail_closed(message));
        }
        let mut out = Vec::new();
        for part in pending {
            out.extend(self.emit_candidate(vec![part], None, None)?);
        }
        Ok(out)
    }

    /// 将规范流帧写为 Gemini 客户端事件；工具签名仅更新同索引调用状态。
    pub fn emit(&mut self, frame: CanonicalStreamFrame) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        // 已 fail-closed 的流不再处理任何帧，持续返回同一错误，杜绝错误后正常 STOP。
        if self.failure.is_some() {
            return Err(AiSurfaceFinalizeError::new(
                self.failure.clone().unwrap_or_default(),
            ));
        }
        self.update_identity(&frame);
        match frame.event {
            CanonicalStreamEvent::Start => Ok(Vec::new()),
            CanonicalStreamEvent::TextDelta(text) => {
                self.emit_candidate(vec![json!({ "text": text })], None, None)
            }
            CanonicalStreamEvent::ReasoningDelta(text) => {
                self.emit_candidate(vec![json!({ "text": text, "thought": true })], None, None)
            }
            CanonicalStreamEvent::ReasoningSignature(signature) => self.emit_candidate(
                vec![json!({
                    "text": "",
                    "thought": true,
                    "thoughtSignature": signature,
                })],
                None,
                None,
            ),
            CanonicalStreamEvent::ContentPart(part) => self.emit_candidate(
                vec![gemini_part_from_canonical_content_part(part)],
                None,
                None,
            ),
            CanonicalStreamEvent::ImageGenerationCall { item, .. } => {
                let Some(part) = content_part_from_openai_image_generation_item(&item) else {
                    return Ok(Vec::new());
                };
                self.emit_candidate(
                    vec![gemini_part_from_canonical_content_part(part)],
                    None,
                    None,
                )
            }
            CanonicalStreamEvent::OpenAiResponsesOutputItem { .. } => Err(
                AiSurfaceFinalizeError::new(
                    "OpenAI Responses output items cannot be converted losslessly to Gemini GenerateContent",
                ),
            ),
            CanonicalStreamEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => {
                let emitted_part = {
                    let state = self.tool_calls.entry(index).or_default();
                    state.call_id = call_id;
                    state.name = name;
                    // name 可能晚于参数分片到达：若参数已成完整对象则此刻补发。
                    let part = Self::ready_tool_call_part(index, state);
                    if part.is_some() {
                        state.emitted = true;
                    }
                    part
                };
                let Some(part) = emitted_part else {
                    return Ok(Vec::new());
                };
                self.emit_candidate(vec![part], None, None)
            }
            CanonicalStreamEvent::ToolCallSignature { index, signature } => {
                self.tool_calls.entry(index).or_default().thought_signature = signature;
                Ok(Vec::new())
            }
            CanonicalStreamEvent::ToolCallArgumentsDelta { index, arguments } => {
                let finished_before = self.finished;
                let (emitted_part, trailing_failure) = {
                    let state = self.tool_calls.entry(index).or_default();
                    state.arguments.push_str(&arguments);
                    if state.emitted {
                        // 早发射完整 JSON 对象后又出现增量：完整对象无法被合法延续，
                        // 重新校验累积串；解析失败说明存在将被静默丢弃的尾随内容，
                        // 中途 fail-closed。已发正常 STOP 后不再报错。
                        let trailing = !finished_before
                            && Self::complete_object_arguments(&state.arguments).is_none();
                        (
                            Option::<Value>::None,
                            trailing.then(|| {
                                format!(
                                    "Gemini 流式工具调用（索引 {index}）在已发射完整 JSON 对象后收到尾随增量参数"
                                )
                            }),
                        )
                    } else {
                        let part = Self::ready_tool_call_part(index, state);
                        if part.is_some() {
                            state.emitted = true;
                        }
                        (part, None)
                    }
                };
                if let Some(message) = trailing_failure {
                    return Err(self.fail_closed(message));
                }
                let Some(part) = emitted_part else {
                    return Ok(Vec::new());
                };
                self.emit_candidate(vec![part], None, None)
            }
            CanonicalStreamEvent::ToolResultDelta {
                tool_use_id,
                name,
                content,
                ..
            } => self.emit_candidate(
                vec![gemini_function_response_part(tool_use_id, name, content)],
                None,
                None,
            ),
            // Only Gemini produces citations today, and a Gemini-to-Gemini
            // stream keeps its own `groundingMetadata` on the passthrough path.
            CanonicalStreamEvent::Citations(_) => Ok(Vec::new()),
            CanonicalStreamEvent::UnknownEvent(_) => Ok(Vec::new()),
            CanonicalStreamEvent::Finish {
                finish_reason,
                usage,
            } => {
                if self.finished {
                    return Ok(Vec::new());
                }
                let mut out = self.flush_pending_tool_calls()?;
                out.extend(self.emit_candidate(vec![], finish_reason.as_deref(), usage)?);
                self.finished = true;
                Ok(out)
            }
            CanonicalStreamEvent::ReasoningSummaryDone => Ok(Vec::new()),
            // Gemini 没有 redacted thinking 载体：必须失败，不得静默丢弃或伪造 thoughtSignature。
            CanonicalStreamEvent::ReasoningRedacted(_) => Err(AiSurfaceFinalizeError::new(
                "Claude redacted thinking cannot be represented in Gemini GenerateContent",
            )),
        }
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        // 已 fail-closed 的流不再补发内容；持续返回同一错误。
        if self.failure.is_some() {
            return Err(AiSurfaceFinalizeError::new(
                self.failure.clone().unwrap_or_default(),
            ));
        }
        let mut out = Vec::new();
        if !self.finished {
            out.extend(self.flush_pending_tool_calls()?);
            self.finished = true;
        }
        out.extend(self.close_json_array());
        Ok(out)
    }
}

fn gemini_function_response_part(
    tool_use_id: String,
    name: Option<String>,
    content: String,
) -> Value {
    json!({
        "functionResponse": {
            "id": tool_use_id,
            "name": name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            "response": gemini_function_response_value(&content),
        }
    })
}

fn gemini_function_response_value(content: &str) -> Value {
    if content.trim().is_empty() {
        return Value::Object(Map::new());
    }
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(value) => json!({ "output": value }),
        Err(_) => json!({ "output": content }),
    }
}

fn gemini_function_response_content(response: &Value) -> String {
    match response {
        Value::Object(object) => object
            .get("result")
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()))
            .to_string(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}

fn render_gemini_part_as_text(part: &Map<String, Value>) -> Option<String> {
    if let Some(text) = part.get("text").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    if let Some(code) = part.get("executableCode").and_then(Value::as_object) {
        let language = code
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let source = code.get("code").and_then(Value::as_str).unwrap_or_default();
        return Some(format!("```{language}\n{source}\n```"));
    }
    if let Some(result) = part.get("codeExecutionResult").and_then(Value::as_object) {
        let output = result
            .get("output")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Some(format!("```output\n{output}\n```"));
    }
    None
}

fn canonical_content_part_from_gemini_part(
    part: &Map<String, Value>,
) -> Option<CanonicalContentPart> {
    if let Some(image_url) = extract_gemini_image_url(part) {
        return Some(CanonicalContentPart::ImageUrl(image_url));
    }
    if let Some(inline_data) = part
        .get("inlineData")
        .or_else(|| part.get("inline_data"))
        .and_then(Value::as_object)
    {
        let mime_type = inline_data
            .get("mimeType")
            .or_else(|| inline_data.get("mime_type"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let data = inline_data
            .get("data")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        if let Some(format) = mime_type.strip_prefix("audio/") {
            return Some(CanonicalContentPart::Audio {
                data: data.to_string(),
                format: format.to_string(),
            });
        }
        return Some(CanonicalContentPart::File {
            file_data: Some(format!("data:{mime_type};base64,{data}")),
            reference: None,
            mime_type: Some(mime_type.to_string()),
            filename: None,
        });
    }
    let file_data = part
        .get("fileData")
        .or_else(|| part.get("file_data"))
        .and_then(Value::as_object)?;
    let reference = file_data
        .get("fileUri")
        .or_else(|| file_data.get("file_uri"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(CanonicalContentPart::File {
        file_data: None,
        reference: Some(reference.to_string()),
        mime_type: file_data
            .get("mimeType")
            .or_else(|| file_data.get("mime_type"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        filename: None,
    })
}

fn gemini_part_from_canonical_content_part(part: CanonicalContentPart) -> Value {
    match part {
        CanonicalContentPart::ImageUrl(url) => {
            if let Some((mime_type, data)) = parse_data_url(url.as_str()) {
                json!({
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": data,
                    }
                })
            } else {
                json!({
                    "fileData": {
                        "fileUri": url.clone(),
                        "mimeType": guess_media_type_from_reference(url.as_str(), "image/jpeg"),
                    }
                })
            }
        }
        CanonicalContentPart::File {
            file_data,
            reference,
            mime_type,
            ..
        } => {
            if let Some(file_data) = file_data {
                if let Some((mime_type, data)) = parse_data_url(file_data.as_str()) {
                    json!({
                        "inlineData": {
                            "mimeType": mime_type,
                            "data": data,
                        }
                    })
                } else {
                    json!({ "text": "[File]" })
                }
            } else if let Some(reference) = reference {
                json!({
                    "fileData": {
                        "fileUri": reference.clone(),
                        "mimeType": mime_type.unwrap_or_else(|| {
                            guess_media_type_from_reference(reference.as_str(), "application/octet-stream")
                        }),
                    }
                })
            } else {
                json!({ "text": "[File]" })
            }
        }
        CanonicalContentPart::Audio { data, format } => json!({
            "inlineData": {
                "mimeType": format!("audio/{format}"),
                "data": data,
            }
        }),
    }
}

fn extract_gemini_image_url(part: &Map<String, Value>) -> Option<String> {
    let inline_data = part
        .get("inlineData")
        .or_else(|| part.get("inline_data"))
        .and_then(Value::as_object)?;
    if !inline_data
        .get("mimeType")
        .or_else(|| inline_data.get("mime_type"))
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("image/"))
    {
        return None;
    }
    if let Some(data) = inline_data
        .get("data")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mime_type = inline_data
            .get("mimeType")
            .or_else(|| inline_data.get("mime_type"))
            .and_then(Value::as_str)
            .unwrap_or("image/jpeg");
        return Some(format!("data:{mime_type};base64,{data}"));
    }
    let file_data = part
        .get("fileData")
        .or_else(|| part.get("file_data"))
        .and_then(Value::as_object)?;
    if !file_data
        .get("mimeType")
        .or_else(|| file_data.get("mime_type"))
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("image/"))
    {
        return None;
    }
    file_data
        .get("fileUri")
        .or_else(|| file_data.get("file_uri"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn parse_data_url(value: &str) -> Option<(String, String)> {
    let rest = value.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime_type = meta.strip_suffix(";base64")?;
    if mime_type.trim().is_empty() || data.trim().is_empty() {
        return None;
    }
    Some((mime_type.to_string(), data.to_string()))
}

fn guess_media_type_from_reference(reference: &str, default_mime: &str) -> String {
    let normalized = reference
        .split('?')
        .next()
        .unwrap_or(reference)
        .to_ascii_lowercase();
    if normalized.ends_with(".png") {
        "image/png".to_string()
    } else if normalized.ends_with(".gif") {
        "image/gif".to_string()
    } else if normalized.ends_with(".webp") {
        "image/webp".to_string()
    } else if normalized.ends_with(".jpg") || normalized.ends_with(".jpeg") {
        "image/jpeg".to_string()
    } else if normalized.ends_with(".pdf") {
        "application/pdf".to_string()
    } else {
        default_mime.to_string()
    }
}

/// 合并分片到达的 Gemini usage：usageMetadata 是累计值，非零字段覆盖旧值，
/// 零值不覆盖已有值；与 stream_core 的 known usage 合并语义保持一致。
fn merge_known_usage(target: &mut Option<CanonicalUsage>, incoming: CanonicalUsage) {
    let Some(current) = target.as_mut() else {
        *target = Some(incoming);
        return;
    };
    if incoming.input_tokens > 0 {
        current.input_tokens = incoming.input_tokens;
        current.input_tokens_include_cache = incoming.input_tokens_include_cache;
    }
    if incoming.output_tokens > 0 {
        current.output_tokens = incoming.output_tokens;
    }
    if incoming.cache_read_tokens > 0 {
        current.cache_read_tokens = incoming.cache_read_tokens;
    }
    if incoming.cache_creation_tokens > 0 {
        current.cache_creation_tokens = incoming.cache_creation_tokens;
    }
    if incoming.cache_creation_ephemeral_5m_tokens > 0 {
        current.cache_creation_ephemeral_5m_tokens = incoming.cache_creation_ephemeral_5m_tokens;
    }
    if incoming.cache_creation_ephemeral_1h_tokens > 0 {
        current.cache_creation_ephemeral_1h_tokens = incoming.cache_creation_ephemeral_1h_tokens;
    }
    if incoming.reasoning_tokens > 0 {
        current.reasoning_tokens = incoming.reasoning_tokens;
    }
    if incoming.total_tokens > 0 {
        current.total_tokens = incoming.total_tokens;
    } else if current.total_tokens == 0 {
        current.total_tokens = current.input_tokens.saturating_add(current.output_tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data_line(value: Value) -> Vec<u8> {
        format!("data: {}\n", value).into_bytes()
    }

    fn terminal_frames(frames: Vec<CanonicalStreamFrame>) -> Vec<CanonicalStreamFrame> {
        frames
            .into_iter()
            .filter(|frame| {
                matches!(
                    frame.event,
                    CanonicalStreamEvent::Start
                        | CanonicalStreamEvent::Finish { .. }
                        | CanonicalStreamEvent::UnknownEvent(_)
                )
            })
            .collect()
    }

    fn observation_record(parts: Vec<Value>, finish_reason: Option<&str>) -> Value {
        let mut record = json!({
            "responseId": "resp_observation",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{"content": {"parts": parts}}],
            "usageMetadata": {
                "promptTokenCount": 22,
                "cachedContentTokenCount": 7,
                "candidatesTokenCount": 13,
                "thoughtsTokenCount": 5,
                "totalTokenCount": 40
            }
        });
        if let Some(finish_reason) = finish_reason {
            record["candidates"][0]["finishReason"] = json!(finish_reason);
        }
        record
    }

    fn assert_terminal_record_matches(
        normal: &mut GeminiProviderState,
        observer: &mut GeminiProviderState,
        record: Value,
    ) -> Vec<CanonicalStreamFrame> {
        let context = json!({"mapped_model": "fallback-model"});
        let expected = terminal_frames(
            normal
                .push_line(&context, data_line(record.clone()))
                .expect("normal provider parser"),
        );
        let actual = observer
            .push_line(&context, data_line(record))
            .expect("terminal provider parser");
        assert_eq!(
            actual, expected,
            "terminal frames must retain their full payloads"
        );
        actual
    }

    fn assert_observer_has_no_content_buffers(observer: &GeminiProviderState) {
        assert!(observer.text_parts.is_empty());
        assert!(observer.reasoning_parts.is_empty());
        assert!(observer.reasoning_signatures.is_empty());
        assert!(observer.content_parts.is_empty());
        assert!(observer.tool_calls.is_empty());
        assert!(observer.tool_results.is_empty());
    }

    #[test]
    fn gemini_terminal_observation_does_not_retain_long_stream_content() {
        let mut normal = GeminiProviderState::default();
        let mut observer = GeminiProviderState::terminal_observation();
        let media = "YQ==".repeat(4_096);
        for index in 0..32 {
            let text = format!("record-{index}:{}", "x".repeat(16_384));
            assert_terminal_record_matches(
                &mut normal,
                &mut observer,
                observation_record(
                    vec![
                        json!({"text": text}),
                        json!({"text": text, "thought": true, "thoughtSignature": media}),
                        json!({"functionCall": {"id": format!("call-{index}"), "name": "lookup", "args": {"value": text}}}),
                        json!({"functionResponse": {"name": "lookup", "response": {"result": text}}}),
                        json!({"inlineData": {"mimeType": "image/png", "data": media}}),
                        json!({"inline_data": {"mime_type": "audio/wav", "data": media}}),
                        json!({"inlineData": {"mimeType": "application/pdf", "data": media}}),
                    ],
                    None,
                ),
            );
            assert_observer_has_no_content_buffers(&observer);
            assert!(observer.observed_tool_calls);
        }
        assert!(!normal.text_parts.is_empty());
        assert!(!normal.reasoning_parts.is_empty());
        assert!(!normal.reasoning_signatures.is_empty());
        assert!(!normal.content_parts.is_empty());
        assert!(!normal.tool_calls.is_empty());
        assert!(normal
            .tool_results
            .values()
            .any(|state| state.content.len() > 16_384));
        let frames = assert_terminal_record_matches(
            &mut normal,
            &mut observer,
            observation_record(Vec::new(), Some("STOP")),
        );
        assert!(frames.iter().any(|frame| matches!(
            &frame.event,
            CanonicalStreamEvent::Finish { finish_reason: Some(reason), usage: Some(usage) }
                if reason == "tool_calls" && usage.input_tokens == 22 && usage.output_tokens == 18
                    && usage.cache_read_tokens == 7 && usage.reasoning_tokens == 5 && usage.total_tokens == 40
        )));
        assert_observer_has_no_content_buffers(&observer);
    }

    #[test]
    fn gemini_terminal_observation_matches_known_and_unknown_part_classification() {
        let parts = vec![
            Value::Null,
            json!({}),
            json!({"text": ""}),
            json!({"text": 17}),
            json!({"text": "", "thought_signature": "signature"}),
            json!({"thoughtSignature": "signature"}),
            json!({"executableCode": {"language": "python", "code": "print(1)"}}),
            json!({"codeExecutionResult": {"output": "1"}}),
            json!({"functionResponse": {}}),
            json!({"function_response": {"response": ["result"]}}),
            json!({"functionCall": {}}),
            json!({"functionCall": null}),
            json!({"inlineData": {"mimeType": "image/png", "data": "YQ=="}}),
            json!({"inline_data": {"mime_type": "audio/wav", "data": "YQ=="}}),
            json!({"inlineData": {"mimeType": "application/pdf", "data": "YQ=="}}),
            json!({"inlineData": {"mimeType": "", "data": "YQ=="}}),
            json!({"inlineData": {"mimeType": "image/png", "data": ""}}),
            json!({"file_data": {"file_uri": "gs://test/file", "mime_type": "application/pdf"}}),
            json!({"fileData": {"fileUri": ""}}),
            json!({"futurePart": {"kept": true}}),
        ];
        for part in parts {
            let mut normal = GeminiProviderState::default();
            let mut observer = GeminiProviderState::terminal_observation();
            assert_terminal_record_matches(
                &mut normal,
                &mut observer,
                json!({"responseId": "outer", "response": observation_record(vec![part], Some("STOP"))}),
            );
            assert_eq!(observer.response_id.as_deref(), Some("resp_observation"));
            assert_observer_has_no_content_buffers(&observer);
        }
    }

    #[test]
    fn gemini_terminal_observation_preserves_finish_reasons_and_eof() {
        for has_tool in [false, true] {
            for reason in [
                None,
                Some("STOP"),
                Some("MAX_TOKENS"),
                Some("RECITATION"),
                Some("FUTURE_REASON"),
            ] {
                let mut normal = GeminiProviderState::default();
                let mut observer = GeminiProviderState::terminal_observation();
                let part = if has_tool {
                    json!({"functionCall": {"name": "lookup", "args": {"query": "test"}}})
                } else {
                    json!({"text": "partial output"})
                };
                assert_terminal_record_matches(
                    &mut normal,
                    &mut observer,
                    observation_record(vec![part], None),
                );
                if let Some(reason) = reason {
                    assert_terminal_record_matches(
                        &mut normal,
                        &mut observer,
                        observation_record(Vec::new(), Some(reason)),
                    );
                }
                let context = json!({});
                assert_eq!(
                    observer.finish(&context).expect("observer EOF"),
                    terminal_frames(normal.finish(&context).expect("normal EOF"))
                );
                assert!(observer
                    .finish(&context)
                    .expect("idempotent EOF")
                    .is_empty());
                assert_observer_has_no_content_buffers(&observer);
            }
        }
    }

    #[test]
    fn gemini_terminal_observation_preserves_failure_payloads_with_usage() {
        for reason in [
            "MALFORMED_FUNCTION_CALL",
            "UNEXPECTED_TOOL_CALL",
            "TOO_MANY_TOOL_CALLS",
            "MISSING_THOUGHT_SIGNATURE",
            "MALFORMED_RESPONSE",
        ] {
            for content in [
                Value::Null,
                json!({}),
                json!({"parts": []}),
                json!({"parts": [{"text": ""}]}),
            ] {
                let mut normal = GeminiProviderState::default();
                let mut observer = GeminiProviderState::terminal_observation();
                let mut record = observation_record(Vec::new(), Some(reason));
                record["candidates"][0]["content"] = content;
                record["candidates"][0]["finishMessage"] = json!("provider failure details");
                let frames = assert_terminal_record_matches(&mut normal, &mut observer, record);
                assert!(frames.iter().any(|frame| matches!(
                    &frame.event,
                    CanonicalStreamEvent::UnknownEvent(payload)
                        if payload["response"]["error"]["code"] == reason
                            && payload["response"]["error"]["message"] == "provider failure details"
                            && payload["response"]["usage"]["input_tokens"] == 22
                )));
                assert!(observer.finish(&json!({})).expect("failed EOF").is_empty());
                assert_observer_has_no_content_buffers(&observer);
            }
        }
    }

    #[test]
    fn gemini_provider_state_tracks_known_usage_from_parsed_events() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        state
            .push_line(
                &context,
                data_line(json!({
                    "responseId": "resp_usage",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{"index": 0, "content": {"parts": [{"text": "hi"}]}}],
                    "usageMetadata": {
                        "promptTokenCount": 10,
                        "candidatesTokenCount": 4,
                        "totalTokenCount": 14
                    }
                })),
            )
            .expect("usage chunk parses");
        let usage = state.known_usage().expect("usage recorded");
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.total_tokens, 14);

        // 累计值分片：零值不覆盖已有字段，非零字段覆盖。
        state
            .push_line(
                &context,
                data_line(json!({
                    "candidates": [{"index": 0, "content": {"parts": []}}],
                    "usageMetadata": {
                        "promptTokenCount": 0,
                        "candidatesTokenCount": 7,
                        "totalTokenCount": 17
                    }
                })),
            )
            .expect("cumulative usage chunk parses");
        let usage = state.known_usage().expect("usage still recorded");
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.total_tokens, 17);

        // terminal observation 模式同样记录 usage（不保留内容缓冲）。
        let mut observer = GeminiProviderState::terminal_observation();
        observer
            .push_line(
                &context,
                data_line(json!({
                    "candidates": [],
                    "usageMetadata": {"promptTokenCount": 3, "totalTokenCount": 3}
                })),
            )
            .expect("observer usage chunk parses");
        assert_eq!(
            observer.known_usage().expect("observer usage").input_tokens,
            3
        );
    }

    #[test]
    fn gemini_provider_state_emits_unknown_events_for_unknown_parts() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_unknown_123",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "parts": [
                                {
                                    "futurePart": {
                                        "kept": true
                                    }
                                }
                            ]
                        }
                    }]
                })),
            )
            .expect("unknown part should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::UnknownEvent(ref payload)
                if payload.get("futurePart").is_some()
        )));
    }

    #[test]
    fn gemini_provider_state_parses_thoughts_code_and_content_filter_finish() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_123",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{
                        "index": 0,
                        "finishReason": "RECITATION",
                        "content": {
                            "parts": [
                                { "text": "reason", "thought": true, "thoughtSignature": "sig_123" },
                                { "executableCode": { "language": "python", "code": "print(1)" } }
                            ]
                        }
                    }],
                    "usageMetadata": {
                        "promptTokenCount": 5,
                        "cachedContentTokenCount": 4,
                        "candidatesTokenCount": 2,
                        "thoughtsTokenCount": 4,
                        "totalTokenCount": 11
                    }
                })),
            )
            .expect("chunk should parse");

        assert!(matches!(frames[0].event, CanonicalStreamEvent::Start));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ReasoningDelta(ref text) if text == "reason"
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ReasoningSignature(ref signature) if signature == "sig_123"
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::TextDelta(ref text) if text == "```python\nprint(1)\n```"
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::Finish { ref finish_reason, .. }
                if finish_reason.as_deref() == Some("content_filter")
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::Finish {
                usage: Some(CanonicalUsage {
                    input_tokens: 5,
                    input_tokens_include_cache: true,
                    output_tokens: 6,
                    cache_read_tokens: 4,
                    reasoning_tokens: 4,
                    total_tokens: 11,
                    ..
                }),
                ..
            }
        )));
    }

    /// 验证畸形函数调用产生带身份与 usage 的完整失败事件，且不再补写成功 Finish。
    #[test]
    fn gemini_provider_state_emits_terminal_error_for_malformed_function_call() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "response": {
                        "responseId": "resp_malformed_tool_call",
                        "modelVersion": "gemini-3.7-flash-tiered",
                        "candidates": [{
                            "index": 0,
                            "content": {
                                "role": "model",
                                "parts": [{
                                    "text": "",
                                    "thoughtSignature": "opaque-thought-signature"
                                }]
                            },
                            "finishReason": "MALFORMED_FUNCTION_CALL",
                            "finishMessage": "Malformed function call: Function call is empty - no input to parse."
                        }],
                        "usageMetadata": {
                            "promptTokenCount": 206744,
                            "cachedContentTokenCount": 203947,
                            "thoughtsTokenCount": 1130,
                            "totalTokenCount": 207874
                        }
                    }
                })),
            )
            .expect("malformed function call terminal should parse");

        assert!(frames.iter().any(|frame| matches!(
            &frame.event,
            CanonicalStreamEvent::UnknownEvent(payload)
                if payload["type"] == "response.failed"
                    && payload["response"]["status"] == "failed"
                    && payload["response"]["id"] == "resp_malformed_tool_call"
                    && payload["response"]["model"] == "gemini-3.7-flash-tiered"
                    && payload["response"]["error"]["code"] == "MALFORMED_FUNCTION_CALL"
                    && payload["response"]["error"]["message"]
                        == "Malformed function call: Function call is empty - no input to parse."
                    && payload["response"]["usage"]["input_tokens"] == 206744
                    && payload["response"]["usage"]["output_tokens"] == 1130
                    && payload["response"]["usage"]["total_tokens"] == 207874
        )));
        assert!(!frames
            .iter()
            .any(|frame| matches!(frame.event, CanonicalStreamEvent::Finish { .. })));
        assert!(state
            .finish(&report_context)
            .expect("finished error stream should not synthesize success")
            .is_empty());
    }

    /// 验证跨格式签名单独耗尽保留 usage，但不生成可见文本或未知事件错误。
    #[test]
    fn gemini_provider_state_preserves_signature_only_reasoning_terminal() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({"client_api_format": "openai:responses"});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "response": {
                        "responseId": "resp_signature_only_123",
                        "modelVersion": "gemini-3.7-flash-tiered",
                        "candidates": [{
                            "index": 0,
                            "finishReason": "MAX_TOKENS",
                            "content": {
                                "role": "model",
                                "parts": [{
                                    "text": "",
                                    "thoughtSignature": "opaque-thought-signature"
                                }]
                            }
                        }],
                        "usageMetadata": {
                            "promptTokenCount": 22,
                            "thoughtsTokenCount": 29,
                            "totalTokenCount": 51
                        }
                    },
                    "traceId": "trace-signature-only"
                })),
            )
            .expect("signature-only reasoning terminal should parse");

        assert!(!frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::TextDelta(_)
                | CanonicalStreamEvent::ReasoningDelta(_)
                | CanonicalStreamEvent::UnknownEvent(_)
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ReasoningSignature(ref signature)
                if signature == "opaque-thought-signature"
        )));
        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::Finish {
                ref finish_reason,
                usage: Some(CanonicalUsage {
                    reasoning_tokens: 29,
                    ..
                }),
            } if finish_reason.as_deref() == Some("length")
        )));
    }

    #[test]
    fn gemini_provider_state_parses_function_response_as_tool_result() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_tool_result_123",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "parts": [{
                                "functionResponse": {
                                    "id": "call_123",
                                    "name": "lookup",
                                    "response": {"ok": true}
                                }
                            }]
                        }
                    }]
                })),
            )
            .expect("function response should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ToolResultDelta {
                index: 0,
                ref tool_use_id,
                name: Some(ref name),
                ref content,
            } if tool_use_id == "call_123" && name == "lookup" && content == "{\"ok\":true}"
        )));
    }

    /// 验证跨格式流不会把带扩展字段的 Gemini 工具结果误当成可安全转换结果。
    #[test]
    fn gemini_provider_state_rejects_extended_function_response_in_cross_format_streams() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_extended_tool_result",
                    "modelVersion": "gemini-3-pro",
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "parts": [{
                                "functionResponse": {
                                    "id": "call_123",
                                    "name": "lookup",
                                    "response": {"ok": true},
                                    "scheduling": "INTERRUPT",
                                    "willContinue": true
                                }
                            }]
                        }
                    }]
                })),
            )
            .expect("extended function response should produce a visible unsupported frame");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::UnknownEvent(ref payload)
                if payload.get("functionResponse").is_some()
        )));
        assert!(!frames
            .iter()
            .any(|frame| matches!(frame.event, CanonicalStreamEvent::ToolResultDelta { .. })));
    }

    /// 验证 Gemini 函数调用的 thoughtSignature 先于工具调用事件进入规范流。
    #[test]
    fn gemini_provider_state_preserves_function_call_thought_signature() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_signed_tool_123",
                    "modelVersion": "gemini-3-flash-preview",
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "parts": [{
                                "functionCall": {
                                    "id": "call_123",
                                    "name": "lookup",
                                    "args": {"query": "rust"}
                                },
                                "thoughtSignature": "opaque-tool-signature"
                            }]
                        }
                    }]
                })),
            )
            .expect("signed function call should parse");

        let signature_index = frames
            .iter()
            .position(|frame| {
                matches!(
                    frame.event,
                    CanonicalStreamEvent::ToolCallSignature {
                        index: 0,
                        ref signature,
                    } if signature == "opaque-tool-signature"
                )
            })
            .expect("tool signature event");
        let call_index = frames
            .iter()
            .position(|frame| {
                matches!(
                    frame.event,
                    CanonicalStreamEvent::ToolCallStart { index: 0, .. }
                )
            })
            .expect("tool call start event");
        assert!(signature_index < call_index);
    }

    #[test]
    fn gemini_provider_state_keeps_parallel_function_calls_from_separate_chunks() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let chunk = |call: Value| {
            data_line(json!({
                "responseId": "resp_parallel_123",
                "modelVersion": "gemini-3.8-flash",
                "candidates": [{
                    "index": 0,
                    "content": {"role": "model", "parts": [{"functionCall": call}]}
                }]
            }))
        };
        let mut frames = Vec::new();
        for call in [
            json!({"id": "call_a", "name": "get_weather", "args": {"city": "Paris"}}),
            json!({"id": "call_b", "name": "get_weather", "args": {"city": "Tokyo"}}),
            json!({"name": "get_time", "args": {"city": "Paris"}}),
            json!({"name": "get_time", "args": {"city": "Tokyo"}}),
        ] {
            frames.extend(
                state
                    .push_line(&report_context, chunk(call))
                    .expect("function call chunk should parse"),
            );
        }

        let starts = frames
            .iter()
            .filter_map(|frame| match &frame.event {
                CanonicalStreamEvent::ToolCallStart {
                    index,
                    call_id,
                    name,
                } => Some((*index, call_id.clone(), name.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 4);
        assert_eq!(
            starts
                .iter()
                .map(|(index, _, _)| *index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(starts[0].1, "call_a");
        assert_eq!(starts[1].1, "call_b");
        assert_eq!(
            starts
                .iter()
                .map(|(_, _, name)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["get_weather", "get_weather", "get_time", "get_time"]
        );
        assert_ne!(starts[2].1, starts[3].1);

        let mut arguments = BTreeMap::<usize, String>::new();
        for frame in &frames {
            if let CanonicalStreamEvent::ToolCallArgumentsDelta {
                index,
                arguments: delta,
            } = &frame.event
            {
                arguments.entry(*index).or_default().push_str(delta);
            }
        }
        assert_eq!(arguments[&0], "{\"city\":\"Paris\"}");
        assert_eq!(arguments[&1], "{\"city\":\"Tokyo\"}");
        assert_eq!(arguments[&2], "{\"city\":\"Paris\"}");
        assert_eq!(arguments[&3], "{\"city\":\"Tokyo\"}");
    }

    #[test]
    fn gemini_client_emitter_rejects_redacted_reasoning() {
        let mut emitter = GeminiClientEmitter::default();
        let error = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_redacted".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ReasoningRedacted("opaque".to_string()),
            })
            .expect_err("Gemini has no redacted thinking carrier");
        assert!(error.0.contains("redacted thinking"));
    }

    #[test]
    fn gemini_client_emitter_marks_reasoning_parts_as_thoughts() {
        let mut emitter = GeminiClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_123".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ReasoningDelta("reason".to_string()),
            })
            .expect("reasoning should encode");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_123".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ReasoningSignature("sig_123".to_string()),
                })
                .expect("signature should encode"),
        );
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_123".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::Finish {
                        finish_reason: Some("stop".to_string()),
                        usage: Some(CanonicalUsage {
                            input_tokens: 1,
                            output_tokens: 3,
                            reasoning_tokens: 1,
                            total_tokens: 4,
                            cache_read_tokens: 5,
                            ..CanonicalUsage::default()
                        }),
                    },
                })
                .expect("finish should encode"),
        );

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"thought\":true"));
        assert!(sse.contains("\"thoughtSignature\":\"sig_123\""));
        assert!(sse.contains("\"promptTokenCount\":6"));
        assert!(sse.contains("\"thoughtsTokenCount\":1"));
        assert!(sse.contains("\"candidatesTokenCount\":2"));
        assert!(sse.contains("\"cachedContentTokenCount\":5"));
        assert!(sse.contains("\"totalTokenCount\":9"));
        assert!(sse.contains("\"finishReason\":\"STOP\""));
    }

    #[test]
    fn gemini_provider_state_parses_inline_image_parts() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_media_123",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "parts": [
                                { "inlineData": { "mimeType": "image/png", "data": "iVBORw0KGgo=" } }
                            ]
                        }
                    }]
                })),
            )
            .expect("chunk should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ContentPart(CanonicalContentPart::ImageUrl(ref url))
                if url == "data:image/png;base64,iVBORw0KGgo="
        )));
    }

    #[test]
    fn gemini_client_emitter_emits_inline_image_parts() {
        let mut emitter = GeminiClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_img_123".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ContentPart(CanonicalContentPart::ImageUrl(
                    "data:image/png;base64,iVBORw0KGgo=".to_string(),
                )),
            })
            .expect("image should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(
            sse.contains("\"inlineData\":{\"mimeType\":\"image/png\",\"data\":\"iVBORw0KGgo=\"}")
        );
    }

    #[test]
    fn gemini_client_emitter_emits_function_response_for_tool_results() {
        let mut emitter = GeminiClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_tool_result_123".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolResultDelta {
                    index: 2,
                    tool_use_id: "call_123".to_string(),
                    name: Some("lookup".to_string()),
                    content: "{\"ok\":true}".to_string(),
                },
            })
            .expect("tool result should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"functionResponse\""));
        assert!(sse.contains("\"id\":\"call_123\""));
        assert!(sse.contains("\"name\":\"lookup\""));
        assert!(sse.contains("\"response\":{\"ok\":true}"));
    }

    #[test]
    fn gemini_client_emitter_fails_closed_on_blank_function_name() {
        let mut emitter = GeminiClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_blank_name".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_blank".to_string(),
                    name: " ".to_string(),
                },
            })
            .expect("blank-name start should buffer without emitting");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_blank_name".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: "{\"q\":1}".to_string(),
                    },
                })
                .expect("complete arguments stay buffered while name is blank"),
        );
        let sse = String::from_utf8(bytes).expect("utf8");
        assert!(!sse.contains("unknown"));
        assert!(!sse.contains("functionCall"));

        let error = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_blank_name".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                },
            })
            .expect_err("terminal state confirms missing name and must fail closed");
        assert!(error.0.contains("仍未获得函数名"));

        // 失败后毒化：finish 不得再输出正常 STOP 或补发调用。
        let poisoned = emitter
            .finish()
            .expect_err("finish after failure must keep failing");
        assert!(poisoned.0.contains("仍未获得函数名"));
    }

    #[test]
    fn gemini_client_emitter_fails_closed_on_truncated_arguments() {
        let mut emitter = GeminiClientEmitter::default();
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_truncated".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_truncated".to_string(),
                    name: "lookup".to_string(),
                },
            })
            .expect("start should buffer");
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_truncated".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"q\":".to_string(),
                },
            })
            .expect("truncated arguments stay buffered");
        let sse = String::from_utf8(bytes).expect("utf8");
        assert!(!sse.contains("functionCall"));
        assert!(!sse.contains("\"args\":{}"));

        let error = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_truncated".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                },
            })
            .expect_err("truncated non-empty arguments must not be fabricated as {}");
        assert!(error.0.contains("不是完整 JSON 对象"));
        // 错误消息不得回显原始参数内容。
        assert!(!error.0.contains("{\"q\":"));

        let poisoned = emitter
            .finish()
            .expect_err("finish after failure must keep failing");
        assert!(poisoned.0.contains("不是完整 JSON 对象"));
    }

    #[test]
    fn gemini_client_emitter_fails_closed_on_non_object_arguments() {
        let mut emitter = GeminiClientEmitter::default();
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_scalar_args".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_scalar".to_string(),
                    name: "lookup".to_string(),
                },
            })
            .expect("start should buffer");
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_scalar_args".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "123".to_string(),
                },
            })
            .expect("scalar arguments stay buffered");
        let error = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_scalar_args".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                },
            })
            .expect_err("Gemini functionCall.args must be a JSON object");
        assert!(error.0.contains("不是完整 JSON 对象"));
    }

    #[test]
    fn gemini_client_emitter_outputs_empty_object_for_legal_no_argument_call() {
        let mut emitter = GeminiClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_no_args".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_noop".to_string(),
                    name: "noop".to_string(),
                },
            })
            .expect("start should buffer");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_no_args".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 1,
                        arguments: "{}".to_string(),
                    },
                })
                .expect("explicit empty object arguments should buffer without a name"),
        );
        let explicit_object = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_no_args".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 1,
                    call_id: "call_explicit".to_string(),
                    name: "explicit_empty".to_string(),
                },
            })
            .expect("late name should flush the buffered empty-object call");
        let explicit_sse = String::from_utf8(explicit_object).expect("utf8");
        assert!(explicit_sse.contains("\"name\":\"explicit_empty\""));
        assert!(explicit_sse.contains("\"args\":{}"));

        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_no_args".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::Finish {
                        finish_reason: Some("tool_calls".to_string()),
                        usage: None,
                    },
                })
                .expect("legal no-argument call should finish normally"),
        );
        let sse = String::from_utf8(bytes).expect("utf8");
        assert!(sse.contains("\"name\":\"noop\""));
        assert!(sse.contains("\"args\":{}"));
        assert!(sse.contains("\"finishReason\":\"STOP\""));
        assert!(!sse.contains("unknown"));
    }

    #[test]
    fn gemini_client_emitter_buffers_arguments_until_name_arrives() {
        let mut emitter = GeminiClientEmitter::default();
        // 参数先到、name 后到：缓冲而非报错，也绝不提前伪造身份。
        let first = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_reordered".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"q\":".to_string(),
                },
            })
            .expect("args-first fragment should buffer");
        let second = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_reordered".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "1}".to_string(),
                },
            })
            .expect("complete args without name should still buffer");
        assert!(first.is_empty());
        assert!(second.is_empty());

        let flushed = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_reordered".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_late_name".to_string(),
                    name: "lookup".to_string(),
                },
            })
            .expect("late name should flush the buffered call");
        let sse = String::from_utf8(flushed).expect("utf8");
        assert!(sse.contains("\"name\":\"lookup\""));
        assert!(sse.contains("\"args\":{\"q\":1}"));
        assert!(sse.contains("\"id\":\"call_late_name\""));
        assert!(!sse.contains("unknown"));

        let finish = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_reordered".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                },
            })
            .expect("finish should succeed after the late name");
        let finish_sse = String::from_utf8(finish).expect("utf8");
        assert!(finish_sse.contains("\"finishReason\":\"STOP\""));
    }

    #[test]
    fn gemini_client_emitter_preserves_object_arguments_and_signature() {
        let mut emitter = GeminiClientEmitter::default();
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_signature".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_sig".to_string(),
                    name: "search".to_string(),
                },
            })
            .expect("start should buffer");
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_signature".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallSignature {
                    index: 0,
                    signature: "sig_abc".to_string(),
                },
            })
            .expect("signature should cache on the call state");
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_signature".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"q\":".to_string(),
                },
            })
            .expect("incomplete fragment should buffer");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_signature".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: "\"gpu\"}".to_string(),
                    },
                })
                .expect("complete object should emit with signature"),
        );
        // 早发射后的纯空白增量不构成新内容，不应报错也不应重复发射。
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_signature".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: "  ".to_string(),
                    },
                })
                .expect("whitespace-only trailing increment is tolerated"),
        );
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_signature".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::Finish {
                        finish_reason: Some("tool_calls".to_string()),
                        usage: None,
                    },
                })
                .expect("legal call should finish normally"),
        );
        let sse = String::from_utf8(bytes).expect("utf8");
        assert!(sse.contains("\"name\":\"search\""));
        assert!(sse.contains("\"args\":{\"q\":\"gpu\"}"));
        assert!(sse.contains("\"thoughtSignature\":\"sig_abc\""));
        assert!(sse.contains("\"finishReason\":\"STOP\""));
        assert_eq!(sse.matches("\"functionCall\"").count(), 1);
    }

    #[test]
    fn gemini_client_emitter_fails_closed_on_trailing_increment_after_complete_arguments() {
        let mut emitter = GeminiClientEmitter::default();
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_trailing".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_trailing".to_string(),
                    name: "lookup".to_string(),
                },
            })
            .expect("start should buffer");
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_trailing".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"q\":1}".to_string(),
                },
            })
            .expect("complete object should emit");
        let error = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_trailing".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "junk".to_string(),
                },
            })
            .expect_err("trailing garbage after a complete object must not be dropped silently");
        assert!(error.0.contains("尾随增量参数"));

        let poisoned = emitter
            .finish()
            .expect_err("finish after failure must keep failing");
        assert!(poisoned.0.contains("尾随增量参数"));
    }

    #[test]
    fn gemini_client_emitter_ignores_post_finish_argument_deltas() {
        let mut emitter = GeminiClientEmitter::default();
        emitter
            .emit(CanonicalStreamFrame {
                id: "resp_post_stop".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "call_post".to_string(),
                    name: "noop".to_string(),
                },
            })
            .expect("start should buffer");
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "resp_post_stop".to_string(),
                model: "gemini-2.5-pro".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                },
            })
            .expect("finish should succeed");
        let sse = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(sse.contains("\"finishReason\":\"STOP\""));
        // 正常 STOP 已发出后，迟到的参数增量不得再追加错误事件。
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "resp_post_stop".to_string(),
                    model: "gemini-2.5-pro".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: "junk".to_string(),
                    },
                })
                .expect("post-finish deltas are ignored after a normal STOP"),
        );
        let after = String::from_utf8(bytes).expect("utf8");
        assert_eq!(after, sse);
    }

    #[test]
    fn gemini_provider_state_preserves_blank_function_call_name() {
        let mut state = GeminiProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "responseId": "resp_nameless",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [{
                        "content": {
                            "role": "model",
                            "parts": [{ "functionCall": { "args": { "q": 1 } } }]
                        }
                    }]
                })),
            )
            .expect("chunk should parse");
        let start = frames
            .iter()
            .find_map(|frame| match &frame.event {
                CanonicalStreamEvent::ToolCallStart { name, .. } => Some(name.clone()),
                _ => None,
            })
            .expect("tool call start should be emitted");
        // canonical 保留空名，不伪造 "unknown"；由目标 emitter 终态拒绝。
        assert_eq!(start, "");
    }

    #[test]
    fn openai_chat_to_gemini_stream_fails_closed_on_missing_tool_name() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let context = json!({
            "provider_api_format": "openai:chat",
            "client_api_format": "gemini:generate_content",
            "mapped_model": "test-model",
        });
        let mut matrix = StreamingStandardFormatMatrix::default();
        let chunks = [
            json!({"id": "chatcmpl_1", "object": "chat.completion.chunk", "model": "test-model",
            "choices": [{"index": 0, "delta": {
                "role": "assistant",
                "tool_calls": [{"index": 0, "id": "call_nameless", "function": {"arguments": ""}}]
            }}]}),
            json!({"id": "chatcmpl_1", "object": "chat.completion.chunk", "model": "test-model",
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "function": {"arguments": "{\"q\":1}"}}]
            }}]}),
            json!({"id": "chatcmpl_1", "object": "chat.completion.chunk", "model": "test-model",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ];
        let mut bytes = Vec::new();
        for chunk in chunks {
            bytes.extend(
                matrix
                    .transform_line(&context, data_line(chunk))
                    .expect("interim chunks convert without a tool name error"),
            );
        }
        let interim = String::from_utf8(bytes).expect("utf8");
        assert!(!interim.contains("functionCall"));
        assert!(!interim.contains("unknown"));
        // 终态确认缺名：源 parser 保留空名补发，Gemini emitter fail-closed。
        let error = matrix
            .finish(&context)
            .expect_err("nameless tool call must fail closed at the Gemini surface");
        assert!(error.0.contains("仍未获得函数名"));
    }

    #[test]
    fn openai_chat_to_gemini_stream_emits_named_tool_call() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let context = json!({
            "provider_api_format": "openai:chat",
            "client_api_format": "gemini:generate_content",
            "mapped_model": "test-model",
        });
        let mut matrix = StreamingStandardFormatMatrix::default();
        let chunks = [
            json!({"id": "chatcmpl_2", "object": "chat.completion.chunk", "model": "test-model",
            "choices": [{"index": 0, "delta": {
                "role": "assistant",
                "tool_calls": [{"index": 0, "id": "call_lookup", "function": {"name": "lookup", "arguments": ""}}]
            }}]}),
            json!({"id": "chatcmpl_2", "object": "chat.completion.chunk", "model": "test-model",
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "function": {"arguments": "{\"q\":"}}]
            }}]}),
            json!({"id": "chatcmpl_2", "object": "chat.completion.chunk", "model": "test-model",
            "choices": [{"index": 0, "delta": {
                "tool_calls": [{"index": 0, "function": {"arguments": "\"gpu\"}"}}]
            }}]}),
            json!({"id": "chatcmpl_2", "object": "chat.completion.chunk", "model": "test-model",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ];
        let mut bytes = Vec::new();
        for chunk in chunks {
            bytes.extend(
                matrix
                    .transform_line(&context, data_line(chunk))
                    .expect("chunks should convert"),
            );
        }
        bytes.extend(matrix.finish(&context).expect("finish should succeed"));
        let sse = String::from_utf8(bytes).expect("utf8");
        assert!(sse.contains("\"functionCall\""));
        assert!(sse.contains("\"id\":\"call_lookup\""));
        assert!(sse.contains("\"name\":\"lookup\""));
        assert!(sse.contains("\"args\":{\"q\":\"gpu\"}"));
        assert!(sse.contains("\"finishReason\":\"STOP\""));
    }

    #[test]
    fn claude_to_gemini_stream_fails_closed_on_missing_tool_name() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let context = json!({
            "provider_api_format": "claude:messages",
            "client_api_format": "gemini:generate_content",
            "mapped_model": "test-model",
        });
        let mut matrix = StreamingStandardFormatMatrix::default();
        let events = [
            json!({"type": "message_start", "message": {"id": "msg_1", "model": "test-model", "role": "assistant"}}),
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "tool_use", "id": "toolu_nameless", "input": {}
            }}),
            json!({"type": "content_block_delta", "index": 0, "delta": {
                "type": "input_json_delta", "partial_json": "{\"q\":1}"
            }}),
            json!({"type": "content_block_stop", "index": 0}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}}),
        ];
        let mut bytes = Vec::new();
        for event in &events[..4] {
            bytes.extend(
                matrix
                    .transform_line(&context, data_line(event.clone()))
                    .expect("interim events convert without a tool name error"),
            );
        }
        let interim = String::from_utf8(bytes).expect("utf8");
        assert!(!interim.contains("functionCall"));
        assert!(!interim.contains("unknown"));
        // claude message_delta 立即产生终态帧，Gemini emitter 在该行就 fail-closed。
        let error = matrix
            .transform_line(&context, data_line(events[4].clone()))
            .expect_err("nameless tool call must fail closed at the Gemini surface");
        assert!(error.0.contains("仍未获得函数名"));
    }

    fn gemini_partial_args_chunk(calls: Vec<Value>, finish_reason: Option<&str>) -> Vec<u8> {
        let parts = calls
            .into_iter()
            .map(|call| json!({"functionCall": call}))
            .collect::<Vec<_>>();
        let mut record = json!({
            "responseId": "resp_partial_args",
            "modelVersion": "gemini-2.5-pro",
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": parts}
            }]
        });
        if let Some(finish_reason) = finish_reason {
            record["candidates"][0]["finishReason"] = json!(finish_reason);
        }
        data_line(record)
    }

    fn tool_call_starts(frames: &[CanonicalStreamFrame]) -> Vec<(usize, String, String)> {
        frames
            .iter()
            .filter_map(|frame| match &frame.event {
                CanonicalStreamEvent::ToolCallStart {
                    index,
                    call_id,
                    name,
                } => Some((*index, call_id.clone(), name.clone())),
                _ => None,
            })
            .collect()
    }

    fn arguments_by_index(frames: &[CanonicalStreamFrame]) -> BTreeMap<usize, String> {
        let mut arguments = BTreeMap::<usize, String>::new();
        for frame in frames {
            if let CanonicalStreamEvent::ToolCallArgumentsDelta {
                index,
                arguments: delta,
            } = &frame.event
            {
                arguments.entry(*index).or_default().push_str(delta);
            }
        }
        arguments
    }

    /// 提取分片参数失败终态载荷（Responses 形状，由 stream_core 路由到各客户端）。
    fn terminal_failure_payload(frames: &[CanonicalStreamFrame]) -> Value {
        frames
            .iter()
            .find_map(|frame| match &frame.event {
                CanonicalStreamEvent::UnknownEvent(payload)
                    if payload["type"] == "response.failed" =>
                {
                    Some(payload.clone())
                }
                _ => None,
            })
            .expect("partial args failure frame")
    }

    fn failure_message(payload: &Value) -> String {
        payload["response"]["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn has_finish(frames: &[CanonicalStreamFrame]) -> bool {
        frames
            .iter()
            .any(|frame| matches!(frame.event, CanonicalStreamEvent::Finish { .. }))
    }

    #[test]
    fn gemini_provider_state_assembles_partial_args_across_chunks() {
        let mut state = GeminiProviderState::default();
        // 跨格式客户端下 partialArgs 也必须通过响应 part 审计。
        let context = json!({"client_api_format": "openai:chat"});
        let mut frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_1",
                        "name": "search",
                        "partialArgs": [{
                            "jsonPath": "$.query",
                            "stringValue": "line1",
                            "willContinue": true
                        }],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("first partial chunk parses");
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(
                        vec![json!({
                            "partialArgs": [
                                {"jsonPath": "$.query"},
                                {"jsonPath": "$.limit", "numberValue": 5}
                            ],
                            "willContinue": true
                        })],
                        None,
                    ),
                )
                .expect("id-less continuation attaches to the active call"),
        );
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(vec![json!({})], Some("STOP")),
                )
                .expect("empty marker ends the call and stream"),
        );

        assert_eq!(
            tool_call_starts(&frames),
            vec![(0, "call_1".to_string(), "search".to_string())]
        );
        assert_eq!(
            arguments_by_index(&frames)[&0],
            "{\"query\":\"line1\",\"limit\":5}"
        );
        let finish_reason = frames.iter().find_map(|frame| match &frame.event {
            CanonicalStreamEvent::Finish { finish_reason, .. } => Some(finish_reason.clone()),
            _ => None,
        });
        assert_eq!(finish_reason, Some(Some("tool_calls".to_string())));
    }

    #[test]
    fn gemini_provider_state_correlates_parallel_partial_calls_by_id() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let mut frames = Vec::new();
        for call in [
            json!({
                "id": "call_a",
                "name": "get_weather",
                "partialArgs": [{"jsonPath": "$.city", "stringValue": "Paris"}],
                "willContinue": true
            }),
            json!({
                "id": "call_b",
                "name": "get_weather",
                "partialArgs": [{"jsonPath": "$.city", "stringValue": "Tokyo"}],
                "willContinue": true
            }),
            json!({
                "id": "call_a",
                "partialArgs": [{"jsonPath": "$.unit", "stringValue": "c"}],
                "willContinue": false
            }),
            json!({"id": "call_b", "willContinue": false}),
        ] {
            frames.extend(
                state
                    .push_line(&context, gemini_partial_args_chunk(vec![call], None))
                    .expect("parallel partial chunk parses"),
            );
        }

        let starts = tool_call_starts(&frames);
        assert_eq!(
            starts
                .iter()
                .map(|(index, _, _)| *index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(starts[0].1, "call_a");
        assert_eq!(starts[1].1, "call_b");
        let arguments = arguments_by_index(&frames);
        assert_eq!(arguments[&0], "{\"city\":\"Paris\",\"unit\":\"c\"}");
        assert_eq!(arguments[&1], "{\"city\":\"Tokyo\"}");
    }

    #[test]
    fn gemini_provider_state_fails_closed_on_unterminated_partial_args() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "name": "search",
                        "partialArgs": [{"jsonPath": "$.q", "stringValue": "x"}],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("open call parses");
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(Vec::new(), Some("STOP")),
            )
            .expect("normal finish with an open call yields a failure frame");
        let payload = terminal_failure_payload(&frames);
        assert!(failure_message(&payload).contains("仍未结束"));
        assert_eq!(payload["response"]["status"], "failed");
        assert_eq!(payload["response"]["error"]["code"], "INVALID_PARTIAL_ARGS");
        assert!(!has_finish(&frames));

        let mut state = GeminiProviderState::default();
        state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_open",
                        "name": "search",
                        "partialArgs": [{
                            "jsonPath": "$.q",
                            "stringValue": "x",
                            "willContinue": true
                        }],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("open string parses");
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({"id": "call_open", "willContinue": false})],
                    None,
                ),
            )
            .expect("closing a call with an incomplete string yields a failure frame");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("未闭合字符串"));
        assert!(!has_finish(&frames));
    }

    #[test]
    fn gemini_provider_state_fails_eof_for_open_partial_call() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "name": "search",
                        "partialArgs": [{"jsonPath": "$.q", "stringValue": "x"}],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("open call parses");
        let frames = state
            .finish(&context)
            .expect("truncated stream yields a failure frame at EOF");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("仍未结束"));
        assert!(!has_finish(&frames));
    }

    #[test]
    fn gemini_provider_state_emits_empty_arguments_for_explicitly_completed_calls() {
        let context = json!({});
        // name + willContinue:false 是上游合法的无参调用，必须输出 `{}`。
        let mut state = GeminiProviderState::default();
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_ping",
                        "name": "ping",
                        "willContinue": false
                    })],
                    None,
                ),
            )
            .expect("valid no-argument call parses");
        assert_eq!(
            tool_call_starts(&frames),
            vec![(0, "call_ping".to_string(), "ping".to_string())]
        );
        assert_eq!(arguments_by_index(&frames)[&0], "{}");

        // 分片路径显式终结的零参数调用同样输出 `{}`。
        let mut state = GeminiProviderState::default();
        let mut frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_ping2",
                        "name": "ping",
                        "partialArgs": [],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("name-first partial call parses");
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(
                        vec![json!({"id": "call_ping2", "willContinue": false})],
                        None,
                    ),
                )
                .expect("explicit finalization emits the empty arguments"),
        );
        assert_eq!(
            tool_call_starts(&frames),
            vec![(0, "call_ping2".to_string(), "ping".to_string())]
        );
        assert_eq!(arguments_by_index(&frames)[&0], "{}");

        // 未终结的零参数调用是截断输入：失败终态，绝不输出空成功调用。
        let mut state = GeminiProviderState::default();
        state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({"name": "ping", "willContinue": true})],
                    None,
                ),
            )
            .expect("open zero-argument call parses");
        let frames = state.finish(&context).expect("EOF yields a failure frame");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("仍未结束"));
        assert!(!has_finish(&frames));
        assert!(tool_call_starts(&frames).is_empty());
        assert!(arguments_by_index(&frames).is_empty());
    }

    #[test]
    fn gemini_client_emitter_reassembles_partial_args_from_deltas() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let mut frames = Vec::new();
        for (call, finish) in [
            (
                json!({
                    "id": "call_1",
                    "name": "search",
                    "partialArgs": [{
                        "jsonPath": "$.query",
                        "stringValue": "line1\n",
                        "willContinue": true
                    }],
                    "willContinue": true
                }),
                None,
            ),
            (
                json!({
                    "partialArgs": [
                        {"jsonPath": "$.query", "stringValue": "line2", "willContinue": false},
                        {"jsonPath": "$.nested.list[0]", "numberValue": 1}
                    ],
                    "willContinue": true
                }),
                None,
            ),
            (
                json!({
                    "partialArgs": [{"jsonPath": "$.nested.list[1]", "numberValue": 2}],
                    "willContinue": false
                }),
                None,
            ),
        ] {
            frames.extend(
                state
                    .push_line(&context, gemini_partial_args_chunk(vec![call], finish))
                    .expect("partial chunk parses"),
            );
        }

        let mut emitter = GeminiClientEmitter::default();
        let mut output = Vec::new();
        for frame in frames {
            output.extend(emitter.emit(frame).expect("emitter consumes deltas"));
        }
        output.extend(emitter.finish().expect("emitter finishes"));

        let text = String::from_utf8(output).expect("utf8 sse");
        let mut function_calls = Vec::new();
        for line in text.lines() {
            let Some(payload) = line.strip_prefix("data: ") else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(payload) else {
                continue;
            };
            for part in value["candidates"][0]["content"]["parts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(call) = part.get("functionCall") {
                    function_calls.push(call.clone());
                }
            }
        }
        assert_eq!(function_calls.len(), 1, "partial call emits exactly once");
        assert_eq!(function_calls[0]["id"], "call_1");
        assert_eq!(function_calls[0]["name"], "search");
        assert_eq!(
            function_calls[0]["args"],
            json!({"query": "line1\nline2", "nested": {"list": [1, 2]}})
        );
    }

    #[test]
    fn gemini_partial_args_failure_becomes_response_failed_through_matrix() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let context = json!({
            "provider_api_format": "gemini:generate_content",
            "client_api_format": "openai:responses",
            "mapped_model": "test-model",
        });
        let mut matrix = StreamingStandardFormatMatrix::default();
        let bytes = matrix
            .transform_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "name": "search",
                        "partialArgs": [{"jsonPath": "query", "stringValue": "x"}]
                    })],
                    None,
                ),
            )
            .expect("malformed partial args yields a terminal failure event");
        let text = String::from_utf8(bytes).expect("utf8 sse");
        // 非法分片参数必须经真实 rewriter 变成失败终态，而不是抛错导致流被截断。
        assert!(text.contains("response.failed"));
        assert!(text.contains("\"status\":\"failed\""));
        assert!(text.contains("jsonPath"));
        assert!(!text.contains("response.completed"));
        assert!(!text.contains("response.incomplete"));
    }

    #[test]
    fn gemini_provider_state_bare_named_marker_finalizes_active_call() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let mut frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_a",
                        "name": "search",
                        "partialArgs": [{"jsonPath": "$.q", "stringValue": "x"}],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("open call parses");
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(
                        vec![json!({"id": "call_a", "name": "search"})],
                        None,
                    ),
                )
                .expect("bare named marker finalizes the active call"),
        );

        assert_eq!(
            tool_call_starts(&frames),
            vec![(0, "call_a".to_string(), "search".to_string())],
            "重复 name 的裸节点不得新建空调用"
        );
        assert_eq!(arguments_by_index(&frames)[&0], "{\"q\":\"x\"}");
        let frames = state
            .finish(&context)
            .expect("closed call finishes cleanly");
        assert!(has_finish(&frames));
    }

    #[test]
    fn gemini_provider_state_restores_remaining_open_call_after_parallel_finish() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let mut frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "id": "call_a",
                        "name": "search",
                        "partialArgs": [{"jsonPath": "$.q", "stringValue": "x"}],
                        "willContinue": true
                    })],
                    None,
                ),
            )
            .expect("open call A parses");
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(
                        vec![json!({
                            "id": "call_b",
                            "name": "ping",
                            "partialArgs": [],
                            "willContinue": false
                        })],
                        None,
                    ),
                )
                .expect("parallel call B starts and finishes"),
        );
        frames.extend(
            state
                .push_line(
                    &context,
                    gemini_partial_args_chunk(
                        vec![json!({
                            "partialArgs": [{"jsonPath": "$.limit", "numberValue": 1}],
                            "willContinue": false
                        })],
                        None,
                    ),
                )
                .expect("id-less fragment attaches to the still-open call A"),
        );

        let starts = tool_call_starts(&frames);
        assert_eq!(
            starts.len(),
            2,
            "无 id 分片必须续写 A，而不是新建第三个调用"
        );
        assert_eq!(starts[0].1, "call_a");
        assert_eq!(starts[1].1, "call_b");
        let arguments = arguments_by_index(&frames);
        assert_eq!(arguments[&0], "{\"q\":\"x\",\"limit\":1}");
        assert_eq!(arguments[&1], "{}");
        let frames = state.finish(&context).expect("all calls closed");
        assert!(has_finish(&frames));
    }

    #[test]
    fn gemini_provider_state_defers_terminal_validation_across_candidates() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let mut frames = state
            .push_line(
                &context,
                data_line(json!({
                    "responseId": "resp_multi",
                    "modelVersion": "gemini-2.5-pro",
                    "candidates": [
                        {"index": 0, "content": {"role": "model", "parts": [{"functionCall": {
                            "id": "call_a",
                            "name": "search",
                            "partialArgs": [{"jsonPath": "$.q", "stringValue": "a"}],
                            "willContinue": true
                        }}]}},
                        {"index": 1, "content": {"role": "model", "parts": [{"functionCall": {
                            "id": "call_b",
                            "name": "ping",
                            "partialArgs": [{"jsonPath": "$.q", "stringValue": "b"}],
                            "willContinue": true
                        }}]}}
                    ]
                })),
            )
            .expect("two candidates open distinct calls");
        frames.extend(
            state
                .push_line(
                    &context,
                    data_line(json!({
                        "candidates": [
                            {
                                "index": 0,
                                "finishReason": "STOP",
                                "content": {"role": "model", "parts": [
                                    {"functionCall": {"id": "call_a", "willContinue": false}}
                                ]}
                            },
                            {
                                "index": 1,
                                "finishReason": "STOP",
                                "content": {"role": "model", "parts": [
                                    {"functionCall": {"id": "call_b", "willContinue": false}}
                                ]}
                            }
                        ]
                    })),
                )
                .expect("simultaneous candidate closure is valid"),
        );

        assert!(
            frames
                .iter()
                .all(|frame| !matches!(&frame.event, CanonicalStreamEvent::UnknownEvent(_))),
            "同一事件内的合法同时关闭不得产生失败终态"
        );
        let starts = tool_call_starts(&frames);
        assert_eq!(
            starts
                .iter()
                .map(|(_, call_id, _)| call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["call_a", "call_b"]
        );
        let finishes = frames
            .iter()
            .filter_map(|frame| match &frame.event {
                CanonicalStreamEvent::Finish { finish_reason, .. } => Some(finish_reason.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(finishes.len(), 2);
        assert!(finishes
            .iter()
            .all(|reason| reason.as_deref() == Some("tool_calls")));
        let arguments = arguments_by_index(&frames);
        assert_eq!(arguments[&0], "{\"q\":\"a\"}");
        assert_eq!(arguments[&1], "{\"q\":\"b\"}");
    }

    #[test]
    fn gemini_provider_state_rejects_malformed_partial_args() {
        let mut state = GeminiProviderState::default();
        let context = json!({});
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "name": "search",
                        "partialArgs": [{"jsonPath": "query", "stringValue": "x"}]
                    })],
                    None,
                ),
            )
            .expect("malformed jsonPath yields a failure frame");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("jsonPath"));
        assert!(!has_finish(&frames));

        let mut state = GeminiProviderState::default();
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({
                        "name": "search",
                        "args": {},
                        "partialArgs": []
                    })],
                    None,
                ),
            )
            .expect("args and partialArgs together yield a failure frame");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("partialArgs"));

        let mut state = GeminiProviderState::default();
        let frames = state
            .push_line(
                &context,
                gemini_partial_args_chunk(
                    vec![json!({"name": "search", "willContinue": "yes"})],
                    None,
                ),
            )
            .expect("non-boolean willContinue yields a failure frame");
        assert!(failure_message(&terminal_failure_payload(&frames)).contains("willContinue"));
    }
}
