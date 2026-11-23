use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use crate::formats::shared::citations::canonical_citations_to_claude_citations;
use crate::formats::shared::response::{
    build_generated_tool_call_id, canonicalize_tool_arguments,
    remove_empty_pages_from_tool_arguments,
};
use crate::formats::shared::sse::{encode_json_sse, map_claude_stop_reason};
use crate::formats::shared::stream_core::common::*;
use crate::formats::shared::AiSurfaceFinalizeError;

#[derive(Default)]
struct ClaudeProviderToolState {
    call_id: String,
    name: String,
    started_emitted: bool,
}

#[derive(Default)]
pub struct ClaudeProviderState {
    pub(crate) observed_end: bool,
    message_id: Option<String>,
    model: Option<String>,
    started: bool,
    finished: bool,
    usage: Option<CanonicalUsage>,
    tool_calls: BTreeMap<usize, ClaudeProviderToolState>,
    /// True while we are inside a thinking content block. Used to emit
    /// `ReasoningSummaryDone` when the block closes, so that downstream
    /// emitters can insert paragraph separators between distinct thinking
    /// blocks (CPA strategy).
    in_thinking_block: bool,
}

impl ClaudeProviderState {
    fn identity(&self, report_context: &Value) -> (String, String) {
        resolve_identity(
            self.message_id.as_deref(),
            self.model.as_deref(),
            report_context,
            "msg-local-stream",
        )
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

    fn unknown_frame(&self, report_context: &Value, payload: Value) -> CanonicalStreamFrame {
        let (id, model) = self.identity(report_context);
        CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::UnknownEvent(payload),
        }
    }

    pub fn push_line(
        &mut self,
        report_context: &Value,
        line: Vec<u8>,
    ) -> Result<Vec<CanonicalStreamFrame>, AiSurfaceFinalizeError> {
        let Some(value) = decode_json_data_line(&line) else {
            return Ok(Vec::new());
        };
        let Some(event_object) = value.as_object() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        if claude_stream_cross_format(report_context)
            && audit_claude_cross_format_stream_event(event_object).is_some()
        {
            out.push(self.unknown_frame(report_context, value.clone()));
            return Ok(out);
        }
        match event_object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "message_start" => {
                if let Some(message) = event_object.get("message").and_then(Value::as_object) {
                    self.message_id = message
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    self.model = message
                        .get("model")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    self.merge_usage(canonical_usage_from_claude_usage(message.get("usage")));
                }
                self.ensure_started(report_context, &mut out);
            }
            "content_block_delta" => {
                let index = event_object
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(0);
                let Some(delta) = event_object.get("delta").and_then(Value::as_object) else {
                    return Ok(out);
                };
                match delta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
                    "text_delta" => {
                        let Some(piece) = delta.get("text").and_then(Value::as_str) else {
                            return Ok(out);
                        };
                        if piece.is_empty() {
                            return Ok(out);
                        }
                        self.ensure_started(report_context, &mut out);
                        let (id, model) = self.identity(report_context);
                        out.push(CanonicalStreamFrame {
                            id,
                            model,
                            event: CanonicalStreamEvent::TextDelta(piece.to_string()),
                        });
                    }
                    "input_json_delta" => {
                        let Some(partial_json) = delta.get("partial_json").and_then(Value::as_str)
                        else {
                            return Ok(out);
                        };
                        if partial_json.is_empty() {
                            return Ok(out);
                        }
                        self.ensure_started(report_context, &mut out);
                        let (id, model) = self.identity(report_context);
                        let tool_state = self.tool_calls.entry(index).or_default();
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
                                    // canonical 保留空名：缺名不伪造 "unknown"，
                                    // 由目标客户端 emitter 在终态统一 fail-closed。
                                    name: tool_state.name.clone(),
                                },
                            });
                            tool_state.started_emitted = true;
                        }
                        out.push(CanonicalStreamFrame {
                            id,
                            model,
                            event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                                index,
                                arguments: partial_json.to_string(),
                            },
                        });
                    }
                    "thinking_delta" => {
                        let Some(piece) = delta
                            .get("thinking")
                            .and_then(Value::as_str)
                            .or_else(|| delta.get("text").and_then(Value::as_str))
                        else {
                            return Ok(out);
                        };
                        if piece.is_empty() {
                            return Ok(out);
                        }
                        self.ensure_started(report_context, &mut out);
                        let (id, model) = self.identity(report_context);
                        out.push(CanonicalStreamFrame {
                            id,
                            model,
                            event: CanonicalStreamEvent::ReasoningDelta(piece.to_string()),
                        });
                    }
                    "signature_delta" => {
                        let Some(signature) = delta.get("signature").and_then(Value::as_str) else {
                            return Ok(out);
                        };
                        if signature.is_empty() {
                            return Ok(out);
                        }
                        self.ensure_started(report_context, &mut out);
                        let (id, model) = self.identity(report_context);
                        out.push(CanonicalStreamFrame {
                            id,
                            model,
                            event: CanonicalStreamEvent::ReasoningSignature(signature.to_string()),
                        });
                    }
                    _ => {
                        out.push(
                            self.unknown_frame(report_context, Value::Object(event_object.clone())),
                        );
                    }
                }
            }
            "content_block_start" => {
                let index = event_object
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(0);
                let Some(block) = event_object.get("content_block").and_then(Value::as_object)
                else {
                    return Ok(out);
                };
                let block_type = block
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if block_type == "redacted_thinking" {
                    let Some(data) = block.get("data").and_then(Value::as_str) else {
                        return Ok(out);
                    };
                    if data.is_empty() {
                        return Ok(out);
                    }
                    self.ensure_started(report_context, &mut out);
                    let (id, model) = self.identity(report_context);
                    out.push(CanonicalStreamFrame {
                        id,
                        model,
                        event: CanonicalStreamEvent::ReasoningRedacted(data.to_string()),
                    });
                    return Ok(out);
                }
                if block_type == "thinking" {
                    self.in_thinking_block = true;
                    let piece = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .or_else(|| block.get("text").and_then(Value::as_str))
                        .unwrap_or_default();
                    let signature = block
                        .get("signature")
                        .and_then(Value::as_str)
                        .filter(|signature| !signature.is_empty());
                    if piece.is_empty() && signature.is_none() {
                        return Ok(out);
                    }
                    self.ensure_started(report_context, &mut out);
                    let (id, model) = self.identity(report_context);
                    if !piece.is_empty() {
                        out.push(CanonicalStreamFrame {
                            id: id.clone(),
                            model: model.clone(),
                            event: CanonicalStreamEvent::ReasoningDelta(piece.to_string()),
                        });
                    }
                    // 部分 provider 在 content_block_start 就带 signature；必须在本块关闭前
                    // 输出，否则终态 carrier 丢失签名。
                    if let Some(signature) = signature {
                        out.push(CanonicalStreamFrame {
                            id,
                            model,
                            event: CanonicalStreamEvent::ReasoningSignature(signature.to_string()),
                        });
                    }
                    return Ok(out);
                }
                if block_type == "text" {
                    let Some(text) = block.get("text").and_then(Value::as_str) else {
                        return Ok(out);
                    };
                    if text.is_empty() {
                        return Ok(out);
                    }
                    self.ensure_started(report_context, &mut out);
                    let (id, model) = self.identity(report_context);
                    out.push(CanonicalStreamFrame {
                        id,
                        model,
                        event: CanonicalStreamEvent::TextDelta(text.to_string()),
                    });
                    return Ok(out);
                }
                if let Some(part) = canonical_content_part_from_claude_block(block) {
                    self.ensure_started(report_context, &mut out);
                    let (id, model) = self.identity(report_context);
                    out.push(CanonicalStreamFrame {
                        id,
                        model,
                        event: CanonicalStreamEvent::ContentPart(part),
                    });
                    return Ok(out);
                }
                if block_type != "tool_use" {
                    out.push(
                        self.unknown_frame(report_context, Value::Object(event_object.clone())),
                    );
                    return Ok(out);
                }
                self.ensure_started(report_context, &mut out);
                let (id, model) = self.identity(report_context);
                let tool_state = self.tool_calls.entry(index).or_default();
                tool_state.call_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(tool_state.call_id.as_str())
                    .to_string();
                tool_state.name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(tool_state.name.as_str())
                    .to_string();
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
                            // canonical 保留空名：缺名不伪造 "unknown"，
                            // 由目标客户端 emitter 在终态统一 fail-closed。
                            name: tool_state.name.clone(),
                        },
                    });
                    tool_state.started_emitted = true;
                }
                let arguments = canonicalize_tool_arguments(block.get("input").cloned());
                if !arguments.is_empty() && arguments != "{}" {
                    out.push(CanonicalStreamFrame {
                        id,
                        model,
                        event: CanonicalStreamEvent::ToolCallArgumentsDelta { index, arguments },
                    });
                }
            }
            "message_delta" => {
                self.ensure_started(report_context, &mut out);
                let Some(delta) = event_object.get("delta").and_then(Value::as_object) else {
                    return Ok(out);
                };
                if !delta
                    .get("stop_reason")
                    .and_then(Value::as_str)
                    .is_some_and(|reason| !reason.is_empty())
                {
                    self.merge_usage(canonical_usage_from_claude_usage(event_object.get("usage")));
                    return Ok(out);
                }
                self.observed_end = true;
                let finish_reason = map_claude_stop_reason(
                    delta.get("stop_reason").and_then(Value::as_str),
                    delta.get("stop_reason").and_then(Value::as_str) == Some("tool_use"),
                );
                let (id, model) = self.identity(report_context);
                out.push(CanonicalStreamFrame {
                    id,
                    model,
                    event: CanonicalStreamEvent::Finish {
                        finish_reason,
                        usage: self.finish_usage(canonical_usage_from_claude_usage(
                            event_object.get("usage"),
                        )),
                    },
                });
                self.finished = true;
            }
            "content_block_stop" => {
                // CPA strategy: when a thinking block closes, emit
                // ReasoningSummaryDone so downstream emitters can insert
                // paragraph separators between distinct thinking blocks.
                if self.in_thinking_block {
                    self.in_thinking_block = false;
                    self.ensure_started(report_context, &mut out);
                    let (id, model) = self.identity(report_context);
                    out.push(CanonicalStreamFrame {
                        id,
                        model,
                        event: CanonicalStreamEvent::ReasoningSummaryDone,
                    });
                }
            }
            "message_stop" => self.observed_end = true,
            "ping" => {}
            _ => {
                out.push(self.unknown_frame(report_context, value.clone()));
            }
        }
        Ok(out)
    }

    pub fn finish(
        &mut self,
        report_context: &Value,
    ) -> Result<Vec<CanonicalStreamFrame>, AiSurfaceFinalizeError> {
        if !self.started || self.finished {
            return Ok(Vec::new());
        }
        self.finished = true;
        let (id, model) = self.identity(report_context);
        Ok(vec![CanonicalStreamFrame {
            id,
            model,
            event: CanonicalStreamEvent::Finish {
                finish_reason: None,
                usage: self.usage.take(),
            },
        }])
    }

    /// 当前已合并的 usage；失败终态由矩阵读取。
    pub(crate) fn known_usage(&self) -> Option<CanonicalUsage> {
        self.usage.clone()
    }

    fn merge_usage(&mut self, usage: Option<CanonicalUsage>) {
        let Some(usage) = usage else {
            return;
        };
        let current = self.usage.take().unwrap_or_default();
        self.usage = Some(merge_claude_usage(current, usage));
    }

    fn finish_usage(&mut self, usage: Option<CanonicalUsage>) -> Option<CanonicalUsage> {
        self.merge_usage(usage);
        self.usage.clone()
    }
}

enum ClaudeOpenBlock {
    Text {
        block_index: usize,
    },
    Thinking {
        block_index: usize,
    },
    Tool {
        tool_index: usize,
        block_index: usize,
    },
}

#[derive(Default)]
struct ClaudeClientToolState {
    call_id: String,
    name: String,
    buffered_arguments: String,
}

#[derive(Default)]
pub struct ClaudeClientEmitter {
    message_id: Option<String>,
    model: Option<String>,
    started: bool,
    finished: bool,
    next_block_index: usize,
    open_block: Option<ClaudeOpenBlock>,
    active_tool: ClaudeClientToolState,
    pending_tools: BTreeMap<usize, ClaudeClientToolState>,
    closed_tools: BTreeSet<usize>,
    agent_bridge_handle: Option<String>,
}

impl ClaudeClientEmitter {
    pub fn with_agent_bridge_handle(mut self, handle: Option<&str>) -> Self {
        self.agent_bridge_handle = handle.map(ToOwned::to_owned);
        self
    }

    fn update_identity(&mut self, frame: &CanonicalStreamFrame) {
        self.message_id = Some(frame.id.clone());
        self.model = Some(frame.model.clone());
    }

    fn ensure_started(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        if self.started {
            return Ok(Vec::new());
        }
        self.started = true;
        let mut out = encode_json_sse(
            Some("message_start"),
            &json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id.as_deref().unwrap_or("msg-local-stream"),
                    "type": "message",
                    "role": "assistant",
                    "model": self.model.as_deref().unwrap_or("unknown"),
                    "content": [],
                    "stop_reason": Value::Null,
                    "stop_sequence": Value::Null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0,
                    },
                }
            }),
        )?;
        if let Some(handle) = self.agent_bridge_handle.as_deref() {
            let block_index = self.next_block_index;
            self.next_block_index += 1;
            out.extend(encode_json_sse(
                Some("content_block_start"),
                &json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "redacted_thinking",
                        "data": handle,
                    }
                }),
            )?);
            out.extend(encode_json_sse(
                Some("content_block_stop"),
                &json!({
                    "type": "content_block_stop",
                    "index": block_index,
                }),
            )?);
        }
        Ok(out)
    }

    fn close_open_block(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = self.close_current_block()?;
        for (index, tool) in std::mem::take(&mut self.pending_tools) {
            let block_index = self.next_block_index;
            out.extend(self.open_tool(index, tool)?);
            if self.active_tool.name != "Read" && !self.active_tool.buffered_arguments.is_empty() {
                out.extend(Self::tool_delta(
                    block_index,
                    &self.active_tool.buffered_arguments,
                )?);
            }
            out.extend(self.close_current_block()?);
        }
        Ok(out)
    }

    fn close_current_block(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let Some(open_block) = self.open_block.take() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let block_index = match open_block {
            ClaudeOpenBlock::Text { block_index } => block_index,
            ClaudeOpenBlock::Thinking { block_index } => block_index,
            ClaudeOpenBlock::Tool {
                tool_index,
                block_index,
            } => {
                self.closed_tools.insert(tool_index);
                let state = std::mem::take(&mut self.active_tool);
                if state.name == "Read" && !state.buffered_arguments.is_empty() {
                    let arguments = remove_empty_pages_from_tool_arguments(
                        &state.name,
                        &state.buffered_arguments,
                    );
                    if !arguments.is_empty() {
                        out.extend(Self::tool_delta(block_index, &arguments)?);
                    }
                }
                block_index
            }
        };
        out.extend(encode_json_sse(
            Some("content_block_stop"),
            &json!({
                "type": "content_block_stop",
                "index": block_index,
            }),
        )?);
        Ok(out)
    }

    fn ensure_text_block(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = Vec::new();
        if let Some(ClaudeOpenBlock::Text { .. }) = self.open_block {
            return Ok(out);
        }
        out.extend(self.close_open_block()?);
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        self.open_block = Some(ClaudeOpenBlock::Text { block_index });
        out.extend(encode_json_sse(
            Some("content_block_start"),
            &json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "text",
                    "text": "",
                }
            }),
        )?);
        Ok(out)
    }

    fn ensure_thinking_block(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = Vec::new();
        if let Some(ClaudeOpenBlock::Thinking { .. }) = self.open_block {
            return Ok(out);
        }
        out.extend(self.close_open_block()?);
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        self.open_block = Some(ClaudeOpenBlock::Thinking { block_index });
        out.extend(encode_json_sse(
            Some("content_block_start"),
            &json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "thinking",
                    "thinking": "",
                }
            }),
        )?);
        Ok(out)
    }

    fn open_tool(
        &mut self,
        tool_index: usize,
        tool: ClaudeClientToolState,
    ) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        self.active_tool = tool;
        self.open_block = Some(ClaudeOpenBlock::Tool {
            tool_index,
            block_index,
        });
        encode_json_sse(
            Some("content_block_start"),
            &json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": self.active_tool.call_id,
                    "name": self.active_tool.name,
                    "input": {},
                }
            }),
        )
    }

    fn tool_delta(index: usize, arguments: &str) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        encode_json_sse(
            Some("content_block_delta"),
            &json!({
                "type": "content_block_delta",
                "index": index,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": arguments,
                }
            }),
        )
    }

    fn start_tool(
        &mut self,
        index: usize,
        call_id: String,
        name: String,
    ) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = self.ensure_started()?;
        if self.closed_tools.contains(&index) {
            return Err(AiSurfaceFinalizeError::new(
                "received a tool call after its Claude content block closed",
            ));
        }
        if let Some(ClaudeOpenBlock::Tool { tool_index, .. }) = self.open_block {
            let existing = if tool_index == index {
                Some(&self.active_tool)
            } else {
                self.pending_tools.get(&index)
            };
            if let Some(tool) = existing {
                if tool.call_id != call_id || tool.name != name {
                    return Err(AiSurfaceFinalizeError::new(
                        "conflicting identity for a streamed tool call",
                    ));
                }
                return Ok(out);
            }
        }
        let tool = ClaudeClientToolState {
            call_id,
            name,
            buffered_arguments: String::new(),
        };
        // Chat can interleave tool arguments. Claude needs one consecutive block per
        // call. Only switch immediately if the active call already has a full value.
        if matches!(self.open_block, Some(ClaudeOpenBlock::Tool { .. }))
            && (!self.pending_tools.is_empty()
                || serde_json::from_str::<serde::de::IgnoredAny>(
                    &self.active_tool.buffered_arguments,
                )
                .is_err())
        {
            self.pending_tools.insert(index, tool);
            return Ok(out);
        }
        out.extend(self.close_current_block()?);
        out.extend(self.open_tool(index, tool)?);
        Ok(out)
    }

    fn emit_tool_result_block(
        &mut self,
        index: usize,
        tool_use_id: String,
        name: Option<String>,
        content: String,
    ) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = self.ensure_started()?;
        out.extend(self.close_open_block()?);
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        let mut content_block = Map::new();
        content_block.insert("type".to_string(), Value::String("tool_result".to_string()));
        content_block.insert("tool_use_id".to_string(), Value::String(tool_use_id));
        if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
            content_block.insert("name".to_string(), Value::String(name));
        }
        content_block.insert("content".to_string(), Value::String(content));
        out.extend(encode_json_sse(
            Some("content_block_start"),
            &json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": Value::Object(content_block),
            }),
        )?);
        out.extend(encode_json_sse(
            Some("content_block_stop"),
            &json!({
                "type": "content_block_stop",
                "index": block_index,
                "canonical_index": index,
            }),
        )?);
        Ok(out)
    }

    /// 将规范帧投影为 Claude SSE；Gemini 专属工具签名没有 Claude 对等字段，故安全忽略。
    pub fn emit(&mut self, frame: CanonicalStreamFrame) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        self.update_identity(&frame);
        match frame.event {
            CanonicalStreamEvent::Start => self.ensure_started(),
            CanonicalStreamEvent::TextDelta(text) => {
                let mut out = self.ensure_started()?;
                out.extend(self.ensure_text_block()?);
                let block_index = match self.open_block {
                    Some(ClaudeOpenBlock::Text { block_index }) => block_index,
                    _ => return Ok(out),
                };
                out.extend(encode_json_sse(
                    Some("content_block_delta"),
                    &json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "text_delta",
                            "text": text,
                        }
                    }),
                )?);
                Ok(out)
            }
            CanonicalStreamEvent::ReasoningDelta(text) => {
                let mut out = self.ensure_started()?;
                out.extend(self.ensure_thinking_block()?);
                let block_index = match self.open_block {
                    Some(ClaudeOpenBlock::Thinking { block_index }) => block_index,
                    _ => return Ok(out),
                };
                out.extend(encode_json_sse(
                    Some("content_block_delta"),
                    &json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "thinking_delta",
                            "thinking": text,
                        }
                    }),
                )?);
                Ok(out)
            }
            CanonicalStreamEvent::ReasoningSignature(signature) => {
                let mut out = self.ensure_started()?;
                out.extend(self.ensure_thinking_block()?);
                let block_index = match self.open_block {
                    Some(ClaudeOpenBlock::Thinking { block_index }) => block_index,
                    _ => return Ok(out),
                };
                out.extend(encode_json_sse(
                    Some("content_block_delta"),
                    &json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "signature_delta",
                            "signature": signature,
                        }
                    }),
                )?);
                Ok(out)
            }
            CanonicalStreamEvent::ReasoningRedacted(data) => {
                let mut out = self.ensure_started()?;
                out.extend(self.close_open_block()?);
                let block_index = self.next_block_index;
                self.next_block_index += 1;
                out.extend(encode_json_sse(
                    Some("content_block_start"),
                    &json!({
                        "type": "content_block_start",
                        "index": block_index,
                        "content_block": {
                            "type": "redacted_thinking",
                            "data": data,
                        }
                    }),
                )?);
                out.extend(encode_json_sse(
                    Some("content_block_stop"),
                    &json!({
                        "type": "content_block_stop",
                        "index": block_index,
                    }),
                )?);
                Ok(out)
            }
            CanonicalStreamEvent::ContentPart(part) => self.emit_content_part(part),
            CanonicalStreamEvent::ImageGenerationCall { item, .. } => {
                let Some(part) = content_part_from_openai_image_generation_item(&item) else {
                    return Ok(Vec::new());
                };
                self.emit_content_part(part)
            }
            CanonicalStreamEvent::OpenAiResponsesOutputItem { .. } => Err(
                AiSurfaceFinalizeError::new(
                    "OpenAI Responses output items cannot be converted losslessly to Claude Messages",
                ),
            ),
            CanonicalStreamEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => self.start_tool(index, call_id, name),
            CanonicalStreamEvent::ToolCallSignature { .. } => Ok(Vec::new()),
            CanonicalStreamEvent::ToolCallArgumentsDelta { index, arguments } => {
                if arguments.is_empty() {
                    return Ok(Vec::new());
                }
                let mut out = Vec::new();
                if !matches!(self.open_block, Some(ClaudeOpenBlock::Tool { tool_index, .. }) if tool_index == index)
                    && !self.pending_tools.contains_key(&index)
                {
                    if self.closed_tools.contains(&index) && arguments.trim().is_empty() {
                        return Ok(out);
                    }
                    out.extend(self.start_tool(
                        index,
                        format!("tool_{index}"),
                        "unknown".to_string(),
                    )?);
                }
                if let Some(tool) = self.pending_tools.get_mut(&index) {
                    tool.buffered_arguments.push_str(&arguments);
                } else if let Some(ClaudeOpenBlock::Tool { block_index, .. }) = self.open_block {
                    self.active_tool.buffered_arguments.push_str(&arguments);
                    if self.active_tool.name != "Read" {
                        out.extend(Self::tool_delta(block_index, &arguments)?);
                    }
                }
                Ok(out)
            }
            CanonicalStreamEvent::ToolResultDelta {
                index,
                tool_use_id,
                name,
                content,
            } => self.emit_tool_result_block(index, tool_use_id, name, content),
            CanonicalStreamEvent::Citations(citations) => {
                let citations = canonical_citations_to_claude_citations(&citations);
                if citations.is_empty() {
                    return Ok(Vec::new());
                }
                // Citations belong to the answer text. If a tool call or a
                // thinking block closed it, open a fresh text block rather than
                // hang the evidence off an unrelated one.
                let mut out = self.ensure_text_block()?;
                let Some(ClaudeOpenBlock::Text { block_index }) = self.open_block else {
                    return Ok(out);
                };
                for citation in citations {
                    out.extend(encode_json_sse(
                        Some("content_block_delta"),
                        &json!({
                            "type": "content_block_delta",
                            "index": block_index,
                            "delta": {
                                "type": "citations_delta",
                                "citation": citation,
                            }
                        }),
                    )?);
                }
                Ok(out)
            }
            CanonicalStreamEvent::UnknownEvent(_) => Ok(Vec::new()),
            CanonicalStreamEvent::Finish {
                finish_reason,
                usage,
            } => {
                if self.finished {
                    return Ok(Vec::new());
                }
                if !self.pending_tools.is_empty()
                    && !matches!(finish_reason.as_deref(), Some("length" | "content_filter"))
                {
                    for tool in std::iter::once(&self.active_tool).chain(self.pending_tools.values())
                    {
                        if !tool.buffered_arguments.is_empty()
                            && serde_json::from_str::<serde::de::IgnoredAny>(
                                &tool.buffered_arguments,
                            )
                            .is_err()
                        {
                            self.finished = true;
                            return Err(AiSurfaceFinalizeError::new(
                                "upstream completed an interleaved tool call with invalid JSON arguments",
                            ));
                        }
                    }
                }
                let mut out = self.ensure_started()?;
                out.extend(self.close_open_block()?);
                let mut payload = Map::new();
                payload.insert(
                    "type".to_string(),
                    Value::String("message_delta".to_string()),
                );
                payload.insert(
                    "delta".to_string(),
                    json!({
                        "stop_reason": map_openai_finish_reason_to_claude(
                            finish_reason.as_deref()
                        ),
                        "stop_sequence": Value::Null,
                    }),
                );
                let usage = usage.unwrap_or_default();
                payload.insert("usage".to_string(), claude_usage_from_usage(&usage));
                out.extend(encode_json_sse(
                    Some("message_delta"),
                    &Value::Object(payload),
                )?);
                out.extend(encode_json_sse(
                    Some("message_stop"),
                    &json!({
                        "type": "message_stop",
                    }),
                )?);
                self.finished = true;
                Ok(out)
            }
            CanonicalStreamEvent::ReasoningSummaryDone => {
                // CPA strategy: close the current thinking block so the next
                // ReasoningDelta opens a fresh one.  Each reasoning paragraph
                // becomes its own thinking block in Claude's wire format.
                Ok(self.close_open_block().unwrap_or_default())
            }
        }
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        if !self.started || self.finished {
            return Ok(Vec::new());
        }
        self.emit(CanonicalStreamFrame {
            id: self
                .message_id
                .clone()
                .unwrap_or_else(|| "msg-local-stream".to_string()),
            model: self.model.clone().unwrap_or_else(|| "unknown".to_string()),
            event: CanonicalStreamEvent::Finish {
                finish_reason: None,
                usage: None,
            },
        })
    }

    fn emit_content_part(
        &mut self,
        part: CanonicalContentPart,
    ) -> Result<Vec<u8>, AiSurfaceFinalizeError> {
        let mut out = self.ensure_started()?;
        out.extend(self.close_open_block()?);
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        let content_block = match part {
            CanonicalContentPart::ImageUrl(url) => {
                if let Some((media_type, data)) = parse_data_url(url.as_str()) {
                    json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        }
                    })
                } else {
                    json!({
                        "type": "image",
                        "source": {
                            "type": "url",
                            "url": url,
                        }
                    })
                }
            }
            CanonicalContentPart::File {
                file_data,
                reference,
                mime_type: _,
                filename: _,
            } => {
                if let Some(file_data) = file_data {
                    if let Some((media_type, data)) = parse_data_url(file_data.as_str()) {
                        json!({
                            "type": "document",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": data,
                            }
                        })
                    } else {
                        json!({
                            "type": "text",
                            "text": "[File]",
                        })
                    }
                } else if let Some(reference) = reference {
                    json!({
                        "type": "document",
                        "source": {
                            "type": "url",
                            "url": reference,
                        }
                    })
                } else {
                    json!({
                        "type": "text",
                        "text": "[File]",
                    })
                }
            }
            CanonicalContentPart::Audio { data, format } => json!({
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": format!("audio/{format}"),
                    "data": data,
                }
            }),
        };
        out.extend(encode_json_sse(
            Some("content_block_start"),
            &json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": content_block,
            }),
        )?);
        out.extend(encode_json_sse(
            Some("content_block_stop"),
            &json!({
                "type": "content_block_stop",
                "index": block_index,
            }),
        )?);
        Ok(out)
    }
}

fn merge_claude_usage(mut current: CanonicalUsage, next: CanonicalUsage) -> CanonicalUsage {
    current.input_tokens_include_cache |= next.input_tokens_include_cache;
    if next.input_tokens > 0 {
        current.input_tokens = next.input_tokens;
    }
    if next.output_tokens > 0 {
        current.output_tokens = next.output_tokens;
    }
    if next.cache_creation_tokens > 0 {
        current.cache_creation_tokens = next.cache_creation_tokens;
    }
    if next.cache_creation_ephemeral_5m_tokens > 0 {
        current.cache_creation_ephemeral_5m_tokens = next.cache_creation_ephemeral_5m_tokens;
    }
    if next.cache_creation_ephemeral_1h_tokens > 0 {
        current.cache_creation_ephemeral_1h_tokens = next.cache_creation_ephemeral_1h_tokens;
    }
    if next.cache_read_tokens > 0 {
        current.cache_read_tokens = next.cache_read_tokens;
    }
    if next.reasoning_tokens > 0 {
        current.reasoning_tokens = next.reasoning_tokens;
    }
    let cache_input_tokens = current
        .cache_creation_tokens
        .saturating_add(current.cache_read_tokens);
    let input_tokens = if current.input_tokens_include_cache {
        current.input_tokens
    } else {
        current.input_tokens.saturating_add(cache_input_tokens)
    };
    current.total_tokens = input_tokens.saturating_add(current.output_tokens);
    current
}

fn claude_stream_cross_format(report_context: &Value) -> bool {
    let Some(client_format) = report_context
        .get("client_api_format")
        .and_then(Value::as_str)
    else {
        return false;
    };
    crate::formats::id::normalize_api_format_alias(client_format) != "claude:messages"
}

/// Return the first provider-specific field path that the canonical stream
/// state machine would otherwise ignore. Same-format Claude streams bypass
/// this audit and retain their existing native passthrough behavior.
fn audit_claude_cross_format_stream_event(event: &Map<String, Value>) -> Option<String> {
    let Some(event_type) = event.get("type").and_then(Value::as_str) else {
        return Some("type".to_string());
    };
    let root_allowed: &[&str] = match event_type {
        "message_start" => &["type", "message"],
        "content_block_start" => &["type", "index", "content_block"],
        "content_block_delta" => &["type", "index", "delta"],
        "message_delta" => &["type", "delta", "usage"],
        "content_block_stop" => &["type", "index", "canonical_index"],
        "message_stop" | "ping" => &["type"],
        "error" => &["type", "error"],
        _ => return Some("type".to_string()),
    };
    if let Some(field) = first_unallowed_key(event, root_allowed) {
        return Some(field.to_string());
    }
    match event_type {
        "message_start" => audit_claude_stream_message(event.get("message")),
        "content_block_start" => audit_claude_stream_content_block(event.get("content_block")),
        "content_block_delta" => audit_claude_stream_delta(event.get("delta")),
        "message_delta" => {
            audit_claude_stream_delta_object(event.get("delta"), &["stop_reason", "stop_sequence"])
                .or_else(|| {
                    event
                        .get("delta")
                        .and_then(Value::as_object)
                        .and_then(|delta| {
                            delta
                                .get("stop_sequence")
                                .filter(|value| !value.is_null())
                                .map(|_| "delta.stop_sequence".to_string())
                        })
                })
                .or_else(|| audit_claude_stream_usage(event.get("usage")))
        }
        "error" => event
            .get("error")
            .map(|error| {
                let Some(error) = error.as_object() else {
                    return Some("error".to_string());
                };
                first_unallowed_key(error, &["type", "message"]).map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| Some("error".to_string())),
        _ => None,
    }
}

fn audit_claude_stream_message(message: Option<&Value>) -> Option<String> {
    let Some(message_value) = message else {
        return Some("message".to_string());
    };
    let Some(message) = message_value.as_object() else {
        return Some("message".to_string());
    };
    if let Some(field) = first_unallowed_key(
        message,
        &[
            "id",
            "type",
            "role",
            "model",
            "content",
            "stop_reason",
            "stop_sequence",
            "usage",
            "container",
            "stop_details",
        ],
    ) {
        return Some(format!("message.{field}"));
    }
    if message
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|content| !content.is_empty())
    {
        return Some("message.content".to_string());
    }
    if message
        .get("stop_sequence")
        .is_some_and(|value| !value.is_null())
    {
        return Some("message.stop_sequence".to_string());
    }
    for field in ["container", "stop_details"] {
        if message.get(field).is_some_and(|value| !value.is_null()) {
            return Some(format!("message.{field}"));
        }
    }
    audit_claude_stream_usage(message.get("usage")).map(|field| format!("message.{field}"))
}

fn audit_claude_stream_content_block(block: Option<&Value>) -> Option<String> {
    let Some(block_value) = block else {
        return Some("content_block".to_string());
    };
    let Some(block) = block_value.as_object() else {
        return Some("content_block".to_string());
    };
    let Some(block_type) = block.get("type").and_then(Value::as_str) else {
        return Some("content_block.type".to_string());
    };
    let allowed: &[&str] = match block_type {
        "text" => &["type", "text"],
        "thinking" => &["type", "thinking", "text", "signature"],
        "redacted_thinking" => &["type", "data"],
        "image" | "document" => &["type", "source"],
        "tool_use" => &["type", "id", "name", "input"],
        _ => return Some(format!("content_block.type ({block_type})")),
    };
    if let Some(field) = first_unallowed_key(block, allowed) {
        return Some(format!("content_block.{field}"));
    }
    if matches!(block_type, "image" | "document") {
        let Some(source_value) = block.get("source") else {
            return Some("content_block.source".to_string());
        };
        let Some(source) = source_value.as_object() else {
            return Some("content_block.source".to_string());
        };
        if let Some(field) = first_unallowed_key(source, &["type", "media_type", "data", "url"]) {
            return Some(format!("content_block.source.{field}"));
        }
    }
    None
}

fn audit_claude_stream_delta(delta: Option<&Value>) -> Option<String> {
    let Some(delta_value) = delta else {
        return Some("delta".to_string());
    };
    let Some(delta) = delta_value.as_object() else {
        return Some("delta".to_string());
    };
    let Some(delta_type) = delta.get("type").and_then(Value::as_str) else {
        return Some("delta.type".to_string());
    };
    let allowed: &[&str] = match delta_type {
        "text_delta" => &["type", "text"],
        "input_json_delta" => &["type", "partial_json"],
        "thinking_delta" => &["type", "thinking", "text"],
        "signature_delta" => &["type", "signature"],
        _ => return Some(format!("delta.type ({delta_type})")),
    };
    first_unallowed_key(delta, allowed).map(|field| format!("delta.{field}"))
}

fn audit_claude_stream_delta_object(delta: Option<&Value>, allowed: &[&str]) -> Option<String> {
    let Some(delta_value) = delta else {
        return Some("delta".to_string());
    };
    let Some(delta) = delta_value.as_object() else {
        return Some("delta".to_string());
    };
    first_unallowed_key(delta, allowed).map(|field| format!("delta.{field}"))
}

fn audit_claude_stream_usage(usage: Option<&Value>) -> Option<String> {
    let usage_value = usage?;
    let Some(usage) = usage_value.as_object() else {
        return Some("usage".to_string());
    };
    let allowed = [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
        "cache_creation",
        "output_tokens_details",
        "reasoning_tokens",
        "server_tool_use",
        "service_tier",
        "inference_geo",
    ];
    for field in usage.keys() {
        if !allowed.contains(&field.as_str())
            && usage.get(field).is_some_and(|value| !value.is_null())
        {
            return Some(field.to_string());
        }
    }
    for (field, allowed) in [
        (
            "cache_creation",
            &["ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"][..],
        ),
        (
            "output_tokens_details",
            &["thinking_tokens", "reasoning_tokens"][..],
        ),
    ] {
        let Some(details) = usage.get(field).and_then(Value::as_object) else {
            continue;
        };
        for detail in details.keys() {
            if !allowed.contains(&detail.as_str())
                && details.get(detail).is_some_and(|value| !value.is_null())
            {
                return Some(format!("{field}.{detail}"));
            }
        }
    }
    None
}

fn first_unallowed_key<'a>(object: &'a Map<String, Value>, allowed: &[&str]) -> Option<&'a str> {
    object
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
        .map(String::as_str)
}

fn canonical_content_part_from_claude_block(
    block: &Map<String, Value>,
) -> Option<CanonicalContentPart> {
    match block
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "image" => {
            let source = block.get("source")?.as_object()?;
            match source.get("type")?.as_str()? {
                "base64" => {
                    let media_type = source
                        .get("media_type")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    let data = source
                        .get("data")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    Some(CanonicalContentPart::ImageUrl(format!(
                        "data:{media_type};base64,{data}"
                    )))
                }
                "url" => source
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| CanonicalContentPart::ImageUrl(value.to_string())),
                _ => None,
            }
        }
        "document" => {
            let source = block.get("source")?.as_object()?;
            match source.get("type")?.as_str()? {
                "base64" => {
                    let media_type = source
                        .get("media_type")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    let data = source
                        .get("data")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?;
                    if let Some(format) = media_type.strip_prefix("audio/") {
                        Some(CanonicalContentPart::Audio {
                            data: data.to_string(),
                            format: format.to_string(),
                        })
                    } else {
                        Some(CanonicalContentPart::File {
                            file_data: Some(format!("data:{media_type};base64,{data}")),
                            reference: None,
                            mime_type: Some(media_type.to_string()),
                            filename: None,
                        })
                    }
                }
                "url" => source
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| CanonicalContentPart::File {
                        file_data: None,
                        reference: Some(value.to_string()),
                        mime_type: None,
                        filename: None,
                    }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn parse_data_url(value: &str) -> Option<(String, String)> {
    let rest = value.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let media_type = meta.strip_suffix(";base64")?;
    if media_type.trim().is_empty() || data.trim().is_empty() {
        return None;
    }
    Some((media_type.to_string(), data.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data_line(value: Value) -> Vec<u8> {
        format!("data: {}\n", value).into_bytes()
    }

    #[test]
    fn claude_provider_state_emits_unknown_events_for_unknown_stream_types() {
        let mut state = ClaudeProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "future_event",
                    "payload": {
                        "kept": true
                    }
                })),
            )
            .expect("unknown stream event should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::UnknownEvent(ref payload)
                if payload.get("type").and_then(Value::as_str) == Some("future_event")
        )));
    }

    #[test]
    fn claude_provider_state_parses_thinking_deltas() {
        let mut state = ClaudeProviderState::default();
        let report_context = json!({});
        let _ = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "message_start",
                    "message": {
                        "id": "msg_123",
                        "model": "claude-sonnet-4-5"
                    }
                })),
            )
            .expect("message_start should parse");
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "thinking_delta",
                        "thinking": "step by step"
                    }
                })),
            )
            .expect("thinking delta should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ReasoningDelta(ref text) if text == "step by step"
        )));
    }

    #[test]
    fn claude_provider_state_parses_signature_deltas() {
        let mut state = ClaudeProviderState::default();
        let report_context = json!({});
        let _ = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "message_start",
                    "message": {
                        "id": "msg_123",
                        "model": "claude-sonnet-4-5"
                    }
                })),
            )
            .expect("message_start should parse");
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "signature_delta",
                        "signature": "sig_123"
                    }
                })),
            )
            .expect("signature delta should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::ReasoningSignature(ref signature) if signature == "sig_123"
        )));
    }

    #[test]
    fn claude_provider_state_preserves_signature_from_block_start() {
        let mut state = ClaudeProviderState::default();
        let report_context = json!({});
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "content_block_start",
                    "index": 2,
                    "content_block": {
                        "type": "thinking",
                        "thinking": "plan 二",
                        "signature": "signed-two"
                    }
                })),
            )
            .expect("block start with signature should parse");
        let events = frames.iter().map(|frame| &frame.event).collect::<Vec<_>>();
        // 只校验投影语义:start 携带的文本先于签名,且签名不丢失;不固化 Start 等附带事件。
        let delta = events
            .iter()
            .position(|event| {
                matches!(event, CanonicalStreamEvent::ReasoningDelta(text) if text.as_str() == "plan 二")
            })
            .expect("start thinking text projected as reasoning delta");
        let signature = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    CanonicalStreamEvent::ReasoningSignature(signature)
                        if signature == "signed-two"
                )
            })
            .expect("start signature projected as reasoning signature");
        assert!(delta < signature, "signature must follow the block text");
        // 块关闭不得丢弃 start 携带的签名。
        let stop = state
            .push_line(
                &report_context,
                data_line(json!({"type": "content_block_stop", "index": 2})),
            )
            .expect("block stop should parse");
        assert!(!stop
            .iter()
            .any(|frame| matches!(frame.event, CanonicalStreamEvent::ReasoningSignature(_))));
    }

    #[test]
    fn claude_provider_state_merges_start_and_delta_usage() {
        let mut state = ClaudeProviderState::default();
        let report_context = json!({});
        let _ = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "message_start",
                    "message": {
                        "id": "msg_123",
                        "model": "claude-sonnet-4-5",
                        "usage": {
                            "input_tokens": 5,
                            "cache_creation_input_tokens": 59_573,
                            "cache_read_input_tokens": 0,
                            "output_tokens": 0,
                        },
                    },
                })),
            )
            .expect("message_start should parse");
        let frames = state
            .push_line(
                &report_context,
                data_line(json!({
                    "type": "message_delta",
                    "delta": {
                        "stop_reason": "end_turn",
                    },
                    "usage": {
                        "output_tokens": 162,
                    },
                })),
            )
            .expect("message_delta should parse");

        assert!(frames.iter().any(|frame| matches!(
            frame.event,
            CanonicalStreamEvent::Finish {
                usage: Some(CanonicalUsage {
                    input_tokens: 5,
                    output_tokens: 162,
                    cache_creation_tokens: 59_573,
                    cache_read_tokens: 0,
                    ..
                }),
                ..
            }
        )));
    }

    #[test]
    fn claude_usage_only_delta_waits_for_message_stop_and_keeps_tokens() {
        let mut state = ClaudeProviderState::default();
        let context = json!({});
        let frames = state
            .push_line(
                &context,
                data_line(json!({
                    "type": "message_delta", "delta": {}, "usage": {"output_tokens": 7}
                })),
            )
            .unwrap();
        assert!(!frames
            .iter()
            .any(|f| matches!(f.event, CanonicalStreamEvent::Finish { .. })));
        state
            .push_line(&context, data_line(json!({"type":"message_stop"})))
            .unwrap();
        let frames = state.finish(&context).unwrap();
        assert!(matches!(
            frames.as_slice(),
            [CanonicalStreamFrame {
                event: CanonicalStreamEvent::Finish {
                    usage: Some(CanonicalUsage {
                        output_tokens: 7,
                        ..
                    }),
                    ..
                },
                ..
            }]
        ));
    }

    #[test]
    fn merge_claude_usage_preserves_inclusive_input_semantics() {
        let merged = super::merge_claude_usage(
            CanonicalUsage::default(),
            CanonicalUsage {
                input_tokens: 110_161,
                input_tokens_include_cache: true,
                output_tokens: 691,
                cache_read_tokens: 107_008,
                ..CanonicalUsage::default()
            },
        );

        assert!(merged.input_tokens_include_cache);
        assert_eq!(merged.total_tokens, 110_852);
    }

    #[test]
    fn claude_client_emitter_preserves_tool_identity_and_emits_thinking_blocks() {
        let mut emitter = ClaudeClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_123".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::Start,
            })
            .expect("start should encode");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ReasoningDelta("step by step".to_string()),
                })
                .expect("reasoning should encode"),
        );
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ReasoningSignature("sig_123".to_string()),
                })
                .expect("signature should encode"),
        );
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ToolCallStart {
                        index: 0,
                        call_id: "toolu_1".to_string(),
                        name: "lookup".to_string(),
                    },
                })
                .expect("tool start should encode"),
        );
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: "{\"city\":\"Shanghai\"}".to_string(),
                    },
                })
                .expect("tool delta should encode"),
        );

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"type\":\"thinking\""));
        assert!(sse.contains("\"type\":\"thinking_delta\""));
        assert!(sse.contains("\"type\":\"signature_delta\""));
        assert!(sse.contains("\"signature\":\"sig_123\""));
        assert!(sse.contains("\"id\":\"toolu_1\""));
        assert!(sse.contains("\"name\":\"lookup\""));
        assert!(sse.contains("\"partial_json\":\"{\\\"city\\\":\\\"Shanghai\\\"}\""));
        assert!(sse.contains("\"usage\":{\"input_tokens\":0,\"output_tokens\":0}"));
    }

    #[test]
    fn claude_client_emitter_removes_empty_pages_from_read_tool_arguments() {
        let mut emitter = ClaudeClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_123".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "toolu_read".to_string(),
                    name: "Read".to_string(),
                },
            })
            .expect("tool start should encode");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: r#"{"file_path":"/tmp/a.txt","offset":1,"limit":20,"pages":""}"#
                            .to_string(),
                    },
                })
                .expect("tool delta should encode"),
        );

        let pending_sse = String::from_utf8(bytes.clone()).expect("sse should be utf8");
        assert!(!pending_sse.contains("\\\"pages\\\":\\\"\\\""));

        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::Finish {
                        finish_reason: Some("tool_calls".to_string()),
                        usage: None,
                    },
                })
                .expect("finish should close read tool block"),
        );

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"partial_json\":\"{\\\"file_path\\\":\\\"/tmp/a.txt\\\",\\\"offset\\\":1,\\\"limit\\\":20}\""));
        assert!(!sse.contains("\\\"pages\\\":\\\"\\\""));
    }

    #[test]
    fn claude_client_emitter_preserves_empty_pages_for_other_tool_arguments() {
        let mut emitter = ClaudeClientEmitter::default();
        let mut bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_123".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "toolu_search".to_string(),
                    name: "Search".to_string(),
                },
            })
            .expect("tool start should encode");
        bytes.extend(
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_123".to_string(),
                    model: "claude-sonnet-4-5".to_string(),
                    event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                        index: 0,
                        arguments: r#"{"query":"","pages":""}"#.to_string(),
                    },
                })
                .expect("tool delta should encode"),
        );

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(
            sse.contains("\"partial_json\":\"{\\\"query\\\":\\\"\\\",\\\"pages\\\":\\\"\\\"}\"")
        );
    }

    #[test]
    fn claude_client_emitter_injects_default_usage_into_finish_events() {
        let mut emitter = ClaudeClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_456".to_string(),
                model: "gpt-5.4".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("stop".to_string()),
                    usage: None,
                },
            })
            .expect("finish should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("event: message_start"));
        assert!(sse.contains("event: message_delta"));
        assert!(sse.contains("\"stop_reason\":\"end_turn\""));
        assert!(sse.contains("\"usage\":{\"input_tokens\":0,\"output_tokens\":0}"));
    }

    #[test]
    fn claude_client_emitter_includes_cache_usage_in_finish_events() {
        let mut emitter = ClaudeClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_cache".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("stop".to_string()),
                    usage: Some(CanonicalUsage {
                        input_tokens: 10,
                        output_tokens: 2,
                        total_tokens: 12,
                        cache_creation_tokens: 5,
                        cache_read_tokens: 4,
                        ..CanonicalUsage::default()
                    }),
                },
            })
            .expect("finish should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"input_tokens\":10"));
        assert!(sse.contains("\"cache_creation_input_tokens\":5"));
        assert!(sse.contains("\"cache_read_input_tokens\":4"));
    }

    #[test]
    fn claude_client_emitter_subtracts_cache_when_input_tokens_include_cache() {
        let mut emitter = ClaudeClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_cache".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::Finish {
                    finish_reason: Some("stop".to_string()),
                    usage: Some(CanonicalUsage {
                        input_tokens: 110_161,
                        input_tokens_include_cache: true,
                        output_tokens: 691,
                        total_tokens: 110_852,
                        cache_read_tokens: 107_008,
                        reasoning_tokens: 516,
                        ..CanonicalUsage::default()
                    }),
                },
            })
            .expect("finish should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"input_tokens\":3153"));
        assert!(sse.contains("\"cache_read_input_tokens\":107008"));
        assert!(sse.contains("\"output_tokens\":691"));
    }

    #[test]
    fn claude_client_emitter_emits_image_blocks_for_media_parts() {
        let mut emitter = ClaudeClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_img_123".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::ContentPart(CanonicalContentPart::ImageUrl(
                    "data:image/png;base64,iVBORw0KGgo=".to_string(),
                )),
            })
            .expect("image should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"type\":\"image\""));
        assert!(sse.contains("\"media_type\":\"image/png\""));
        assert!(sse.contains("\"data\":\"iVBORw0KGgo=\""));
        assert!(sse.contains("event: content_block_stop"));
    }

    #[test]
    fn claude_client_emitter_emits_tool_result_blocks() {
        let mut emitter = ClaudeClientEmitter::default();
        let bytes = emitter
            .emit(CanonicalStreamFrame {
                id: "msg_tool_result_123".to_string(),
                model: "claude-sonnet-4-5".to_string(),
                event: CanonicalStreamEvent::ToolResultDelta {
                    index: 2,
                    tool_use_id: "toolu_1".to_string(),
                    name: Some("lookup".to_string()),
                    content: "{\"ok\":true}".to_string(),
                },
            })
            .expect("tool result should encode");

        let sse = String::from_utf8(bytes).expect("sse should be utf8");
        assert!(sse.contains("\"type\":\"tool_result\""));
        assert!(sse.contains("\"tool_use_id\":\"toolu_1\""));
        assert!(sse.contains("\"name\":\"lookup\""));
        assert!(sse.contains("\"content\":\"{\\\"ok\\\":true}\""));
        assert!(sse.contains("\"canonical_index\":2"));
    }

    fn parse_claude_events(bytes: &[u8]) -> Vec<Value> {
        std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect()
    }

    #[test]
    fn interleaved_chat_tools_emit_each_claude_block_once() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let ctx =
            json!({"provider_api_format":"openai:chat","client_api_format":"claude:messages"});
        let mut matrix = StreamingStandardFormatMatrix::default();
        let mut events = Vec::<Value>::new();
        let calls = [
            json!({"index":0,"id":"call-a","function":{"name":"shell","arguments":"{\"cmd\":"}}),
            json!({"index":1,"id":"call-b","function":{"name":"Read","arguments":"{\"file_path\":"}}),
            json!({"index":2,"id":"call-c","function":{"name":"search","arguments":"{}"}}),
            json!({"index":1,"id":"call-b","function":{"arguments":"\"x\",\"pages\":\"\"}"}}),
            json!({"index":0,"id":"call-a","function":{"arguments":"\"a}b\"}"}}),
        ];
        for (i, call) in calls.iter().enumerate() {
            let chunk = json!({"id":"m","choices":[{"delta":{"tool_calls":[call]}}]});
            let bytes = matrix.transform_line(&ctx, data_line(chunk)).unwrap();
            if i == 0 {
                assert!(String::from_utf8_lossy(&bytes).contains("partial_json"));
            }
            events.extend(parse_claude_events(&bytes));
        }
        let finish = json!({"id":"m","choices":[{"delta":{},"finish_reason":"tool_calls"}]});
        events.extend(parse_claude_events(
            &matrix.transform_line(&ctx, data_line(finish)).unwrap(),
        ));
        events.extend(parse_claude_events(&matrix.finish(&ctx).unwrap()));
        let mut calls = Vec::new();
        let mut open: Option<(String, String, String)> = None;
        for event in events {
            match event["type"].as_str().unwrap() {
                "content_block_start" => {
                    assert!(open.is_none());
                    assert_eq!(event["index"], calls.len());
                    let block = &event["content_block"];
                    open = Some((
                        block["id"].as_str().unwrap().to_owned(),
                        block["name"].as_str().unwrap().to_owned(),
                        String::new(),
                    ));
                }
                "content_block_delta" => {
                    assert_eq!(event["index"], calls.len());
                    open.as_mut()
                        .unwrap()
                        .2
                        .push_str(event["delta"]["partial_json"].as_str().unwrap());
                }
                "content_block_stop" => {
                    assert_eq!(event["index"], calls.len());
                    let (id, name, args) = open.take().unwrap();
                    calls.push(json!([
                        id,
                        name,
                        serde_json::from_str::<Value>(&args).unwrap()
                    ]));
                }
                _ => {}
            }
        }
        assert!(open.is_none());
        assert_eq!(
            calls,
            [
                json!(["call-a", "shell", {"cmd":"a}b"}]),
                json!(["call-b", "Read", {"file_path":"x"}]),
                json!(["call-c", "search", {}]),
            ]
        );
    }

    #[test]
    fn chat_error_never_flushes_pending_claude_tool_calls() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;
        let ctx =
            json!({"provider_api_format":"openai:chat","client_api_format":"claude:messages"});
        let mut matrix = StreamingStandardFormatMatrix::default();
        for (index, id) in [(0, "first"), (1, "pending")] {
            let chunk = json!({"id":"m","choices":[{"delta":{"tool_calls":[{
                "index":index,"id":id,"function":{"name":"search","arguments":"{"}
            }]}}]});
            let emitted = matrix.transform_line(&ctx, data_line(chunk)).unwrap();
            assert!(!String::from_utf8_lossy(&emitted).contains("pending"));
        }
        let failure = json!({"error":{"message":"upstream failed","type":"server_error"}});
        let emitted = matrix.transform_line(&ctx, data_line(failure)).unwrap();
        let sse = String::from_utf8_lossy(&emitted);
        assert!(sse.contains("event: error"));
        assert!(!sse.contains("pending"));
        assert!(!sse.contains("message_stop"));
        assert!(matrix.finish(&ctx).unwrap().is_empty());
    }

    #[test]
    fn incomplete_interleaved_tool_arguments_cannot_finish_as_success() {
        let mut emitter = ClaudeClientEmitter::default();
        let mut emit = |event| {
            emitter.emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event,
            })
        };
        for (index, args) in [(0, "{"), (1, "{}")] {
            emit(CanonicalStreamEvent::ToolCallStart {
                index,
                call_id: format!("id-{index}"),
                name: "test".into(),
            })
            .unwrap();
            emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
                index,
                arguments: args.into(),
            })
            .unwrap();
        }
        let failure = emit(CanonicalStreamEvent::Finish {
            finish_reason: Some("tool_calls".into()),
            usage: None,
        })
        .unwrap_err();
        assert!(failure.to_string().contains("invalid JSON arguments"));
        assert!(emitter.finish().unwrap().is_empty());
    }

    #[test]
    fn deferred_tool_deltas_preserve_order_and_switch_immediately_when_complete() {
        let mut emitter = ClaudeClientEmitter::default();
        let mut bytes = Vec::new();
        let mut emit = |event| {
            emitter
                .emit(CanonicalStreamFrame {
                    id: "m".into(),
                    model: "m".into(),
                    event,
                })
                .unwrap()
        };

        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 0,
            call_id: "call-0".into(),
            name: "shell".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 0,
            arguments: "{\"a\":".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 1,
            call_id: "call-1".into(),
            name: "search".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 1,
            arguments: "{\"b\":".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 2,
            call_id: "call-2".into(),
            name: "grep".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 2,
            arguments: "{\"c\":".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 1,
            arguments: "1}".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 2,
            arguments: "2}".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 0,
            arguments: "0}".into(),
        }));
        // Deferred tools must not appear before Finish flushes them.
        let before_finish = String::from_utf8_lossy(&bytes);
        assert!(!before_finish.contains("call-1"));
        assert!(!before_finish.contains("call-2"));

        bytes.extend(emit(CanonicalStreamEvent::Finish {
            finish_reason: Some("tool_calls".into()),
            usage: None,
        }));
        let events = parse_claude_events(&bytes);
        let mut blocks = Vec::new();
        let mut current: Option<(usize, String, String)> = None;
        for event in events {
            match event["type"].as_str().unwrap() {
                "content_block_start" => {
                    assert!(current.is_none());
                    current = Some((
                        event["index"].as_u64().unwrap() as usize,
                        event["content_block"]["id"].as_str().unwrap().to_owned(),
                        String::new(),
                    ));
                }
                "content_block_delta" => {
                    let cur = current.as_mut().unwrap();
                    assert_eq!(event["index"].as_u64().unwrap() as usize, cur.0);
                    cur.2
                        .push_str(event["delta"]["partial_json"].as_str().unwrap());
                }
                "content_block_stop" => {
                    let cur = current.take().unwrap();
                    assert_eq!(event["index"].as_u64().unwrap() as usize, cur.0);
                    blocks.push(cur);
                }
                _ => {}
            }
        }
        assert_eq!(
            blocks,
            vec![
                (0, "call-0".to_string(), "{\"a\":0}".to_string()),
                (1, "call-1".to_string(), "{\"b\":1}".to_string()),
                (2, "call-2".to_string(), "{\"c\":2}".to_string()),
            ]
        );

        // When active tool already has complete JSON, starting the next tool switches immediately.
        let mut immediate = ClaudeClientEmitter::default();
        immediate
            .emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 0,
                    call_id: "c0".into(),
                    name: "search".into(),
                },
            })
            .unwrap();
        immediate
            .emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"x\":1}".into(),
                },
            })
            .unwrap();
        let switched = immediate
            .emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event: CanonicalStreamEvent::ToolCallStart {
                    index: 1,
                    call_id: "c1".into(),
                    name: "grep".into(),
                },
            })
            .unwrap();
        let switched_sse = String::from_utf8(switched).unwrap();
        assert!(switched_sse.contains("event: content_block_stop"));
        assert!(switched_sse.contains("\"id\":\"c1\""));
        // Trailing whitespace for closed tool 0 is ignored; non-whitespace fails.
        assert!(immediate
            .emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: " \n".into(),
                },
            })
            .unwrap()
            .is_empty());
        let late_err = immediate
            .emit(CanonicalStreamFrame {
                id: "m".into(),
                model: "m".into(),
                event: CanonicalStreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: ",\"y\":2}".into(),
                },
            })
            .unwrap_err();
        assert!(late_err.to_string().contains("closed"));
    }

    #[test]
    fn incomplete_pending_tool_or_eof_never_flushes_or_emits_synthetic_stop() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        // Pending tool incomplete while active tool completed.
        let mut emitter = ClaudeClientEmitter::default();
        for event in [
            CanonicalStreamEvent::ToolCallStart {
                index: 0,
                call_id: "id-0".into(),
                name: "test".into(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                index: 0,
                arguments: "{".into(),
            },
            CanonicalStreamEvent::ToolCallStart {
                index: 1,
                call_id: "id-1".into(),
                name: "test".into(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                index: 1,
                arguments: "{".into(),
            },
            CanonicalStreamEvent::ToolCallArgumentsDelta {
                index: 0,
                arguments: "}".into(),
            },
        ] {
            emitter
                .emit(CanonicalStreamFrame {
                    id: "m".into(),
                    model: "m".into(),
                    event,
                })
                .unwrap();
        }
        let err = emitter.finish().unwrap_err();
        assert!(err.to_string().contains("invalid JSON arguments"));
        assert!(emitter.finish().unwrap().is_empty());

        // Matrix path: incomplete interleaved tool arguments on finish_reason=tool_calls
        // must emit an error event without flushing queued tools or message_stop.
        let ctx =
            json!({"provider_api_format":"openai:chat","client_api_format":"claude:messages"});
        let mut matrix = StreamingStandardFormatMatrix::default();
        for (index, id, args) in [(0, "first", "{"), (1, "queued", "{")] {
            let chunk = json!({"id":"m","choices":[{"delta":{"tool_calls":[{
                "index":index,"id":id,"function":{"name":"search","arguments":args}
            }]}}]});
            let emitted = matrix.transform_line(&ctx, data_line(chunk)).unwrap();
            assert!(!String::from_utf8_lossy(&emitted).contains("queued"));
        }
        let finish = json!({"id":"m","choices":[{"delta":{},"finish_reason":"tool_calls"}]});
        let mut emitted = matrix.transform_line(&ctx, data_line(finish)).unwrap();
        emitted.extend(matrix.finish(&ctx).unwrap());
        let sse = String::from_utf8_lossy(&emitted);
        assert!(sse.contains("event: error"), "{sse}");
        assert!(!sse.contains("queued"), "{sse}");
        assert!(!sse.contains("message_stop"), "{sse}");
        assert!(matrix.finish(&ctx).unwrap().is_empty());
    }

    #[test]
    fn finish_reason_error_never_flushes_pending_claude_tool_calls_or_synthetic_stop() {
        use crate::formats::shared::stream_core::StreamingStandardFormatMatrix;

        let ctx =
            json!({"provider_api_format":"openai:chat","client_api_format":"claude:messages"});
        let mut matrix = StreamingStandardFormatMatrix::default();
        for (index, id) in [(0, "first"), (1, "pending")] {
            let chunk = json!({"id":"m","choices":[{"delta":{"tool_calls":[{
                "index":index,"id":id,"function":{"name":"search","arguments":"{"}
            }]}}]});
            let emitted = matrix.transform_line(&ctx, data_line(chunk)).unwrap();
            assert!(!String::from_utf8_lossy(&emitted).contains("pending"));
        }
        let failure = json!({"id":"m","choices":[{"delta":{},"finish_reason":"error"}]});
        let mut emitted = matrix.transform_line(&ctx, data_line(failure)).unwrap();
        emitted.extend(matrix.finish(&ctx).unwrap());
        let sse = String::from_utf8_lossy(&emitted);
        assert!(sse.contains("event: error"), "{sse}");
        assert!(!sse.contains("pending"), "{sse}");
        assert!(!sse.contains("message_stop"), "{sse}");
        assert!(matrix.finish(&ctx).unwrap().is_empty());
    }

    #[test]
    fn claude_client_emitter_preserves_local_paths_with_interleaved_tools() {
        let mut emitter =
            ClaudeClientEmitter::default().with_agent_bridge_handle(Some("aether-abr1.handle"));
        let mut bytes = Vec::new();
        let mut emit = |event| {
            emitter
                .emit(CanonicalStreamFrame {
                    id: "msg_local".into(),
                    model: "claude-sonnet-4-5".into(),
                    event,
                })
                .unwrap()
        };

        bytes.extend(emit(CanonicalStreamEvent::ReasoningDelta("plan".into())));
        bytes.extend(emit(CanonicalStreamEvent::ReasoningSignature(
            "sig_local".into(),
        )));
        bytes.extend(emit(CanonicalStreamEvent::ReasoningSummaryDone));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 0,
            call_id: "call_read_active".into(),
            name: "Read".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 0,
            arguments: r#"{"file_path":"/tmp/a.rs","#.into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 1,
            call_id: "call_search".into(),
            name: "search".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 1,
            arguments: r#"{"q":"rust"}"#.into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallStart {
            index: 2,
            call_id: "call_read_pending".into(),
            name: "Read".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 2,
            arguments: r#"{"file_path":"/tmp/b.rs","pages":""}"#.into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolCallArgumentsDelta {
            index: 0,
            arguments: r#""pages":""}"#.into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::ToolResultDelta {
            index: 3,
            tool_use_id: "call_read_active".into(),
            name: Some("Read".into()),
            content: "ok".into(),
        }));
        bytes.extend(emit(CanonicalStreamEvent::Finish {
            finish_reason: Some("tool_calls".into()),
            usage: Some(CanonicalUsage {
                input_tokens: 120,
                input_tokens_include_cache: true,
                output_tokens: 30,
                total_tokens: 150,
                cache_creation_tokens: 10,
                cache_read_tokens: 40,
                ..CanonicalUsage::default()
            }),
        }));

        let events = parse_claude_events(&bytes);
        let starts = events
            .iter()
            .filter(|e| e["type"] == "content_block_start")
            .map(|e| {
                (
                    e["index"].as_u64().unwrap(),
                    e["content_block"]["type"].as_str().unwrap().to_owned(),
                    e["content_block"]["id"].as_str().map(ToOwned::to_owned),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            starts,
            vec![
                (0, "redacted_thinking".to_string(), None),
                (1, "thinking".to_string(), None),
                (
                    2,
                    "tool_use".to_string(),
                    Some("call_read_active".to_string())
                ),
                (3, "tool_use".to_string(), Some("call_search".to_string())),
                (
                    4,
                    "tool_use".to_string(),
                    Some("call_read_pending".to_string())
                ),
                (5, "tool_result".to_string(), None),
            ]
        );
        let sse = String::from_utf8(bytes).unwrap();
        assert!(sse.contains("aether-abr1.handle"));
        assert!(sse.contains("\"signature\":\"sig_local\""));
        assert!(!sse.contains("\\\"pages\\\":\\\"\\\""));
        assert!(sse.contains("\\\"file_path\\\":\\\"/tmp/a.rs\\\""));
        assert!(sse.contains("\\\"file_path\\\":\\\"/tmp/b.rs\\\""));
        assert!(sse.contains("\"type\":\"tool_result\""));
        assert!(sse.contains("\"input_tokens\":70"));
        assert!(sse.contains("\"cache_creation_input_tokens\":10"));
        assert!(sse.contains("\"cache_read_input_tokens\":40"));
        assert!(sse.contains("\"stop_reason\":\"tool_use\""));
    }
}
