use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::protocol::canonical::{
    canonical_to_openai_responses_request, from_claude_to_canonical_request,
    AETHER_AGENT_BRIDGE_PROMPT_CACHE_BREAKPOINT_FIELD,
};

pub const AGENT_BRIDGE_HANDLE_PREFIX: &str = "aether-abr1.";
pub const AGENT_BRIDGE_REASONING_PREFIX: &str = "aether-ars1.";
pub const AGENT_BRIDGE_REPORT_CONTEXT_FIELD: &str = "agent_bridge";

const AGENT_BRIDGE_CODEX_OVERLAY: &str = "Aether Claude Code bridge: continue until the task is complete; inspect the repository before editing; use the provided Claude Code tools with their existing names, except that WebFetch and WebSearch are represented by the upstream hosted web_search tool; when fetching a specific URL, open that page and inspect the relevant content; verify tool results, and diagnose then retry recoverable failures.";
const CLAUDE_CODE_HOSTED_WEB_TOOL_NAMES: &[&str] = &["WebFetch", "WebSearch"];
const OPENAI_STRICT_JSON_SCHEMA_STRING_FORMATS: &[&str] = &[
    "date-time",
    "time",
    "date",
    "duration",
    "email",
    "hostname",
    "ipv4",
    "ipv6",
    "uuid",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentBridgePrimaryState {
    #[default]
    FullState,
    ReasoningFallback,
    StateMiss,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBridgeCompatibilityReport {
    pub primary_state: AgentBridgePrimaryState,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_mismatch: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub phase_inferred: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub state_truncated: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restored_item_count: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub saved_item_count: usize,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub accepted_loss_types: BTreeSet<String>,
}

impl AgentBridgeCompatibilityReport {
    pub fn mark_reasoning_fallback(&mut self) {
        if self.primary_state != AgentBridgePrimaryState::StateMiss {
            self.primary_state = AgentBridgePrimaryState::ReasoningFallback;
        }
    }

    pub fn mark_state_miss(&mut self, loss_type: impl Into<String>) {
        self.primary_state = AgentBridgePrimaryState::StateMiss;
        self.accepted_loss_types.insert(loss_type.into());
    }

    pub fn accept_loss(&mut self, loss_type: impl Into<String>) {
        self.accepted_loss_types.insert(loss_type.into());
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentBridgeHistoryMessage {
    pub message_index: usize,
    pub handles: Vec<String>,
    pub reasoning_fallbacks: Vec<String>,
    pub projected_items: Vec<Value>,
    pub has_tool_use: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentBridgeRequestSanitizeReport {
    pub cache_control_removed: usize,
    pub cache_breakpoints_projected: usize,
    pub cache_breakpoints_relocated: usize,
    pub cache_control_unmapped: usize,
    pub bridge_thinking_blocks_removed: usize,
    pub unsupported_thinking_blocks: usize,
    pub tool_reference_blocks_projected: usize,
    pub unsupported_tool_result_content_arrays: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentBridgeCodexOverlayReport {
    pub changed: bool,
    pub hosted_web_tools_projected: usize,
    pub hosted_web_search_inserted: bool,
    pub hosted_web_tool_choice_projected: bool,
}

pub fn sanitize_claude_request_for_agent_bridge_conversion(
    body: &Value,
) -> (Value, AgentBridgeRequestSanitizeReport) {
    sanitize_claude_request_for_agent_bridge_conversion_with_prompt_cache(body, false)
}

pub fn sanitize_claude_request_for_agent_bridge_conversion_with_prompt_cache(
    body: &Value,
    supports_explicit_prompt_cache: bool,
) -> (Value, AgentBridgeRequestSanitizeReport) {
    let mut sanitized = body.clone();
    let mut report = AgentBridgeRequestSanitizeReport::default();
    let Some(object) = sanitized.as_object_mut() else {
        return (sanitized, report);
    };

    let mut relocatable_prefix_markers = 0usize;
    collect_relocatable_cache_control(
        object,
        &mut report,
        supports_explicit_prompt_cache,
        &mut relocatable_prefix_markers,
    );
    if let Some(system) = object.get_mut("system").and_then(Value::as_array_mut) {
        for block in system {
            if let Some(block) = block.as_object_mut() {
                let projectable_block =
                    block.get("type").and_then(Value::as_str).unwrap_or("text") == "text";
                project_cache_control(
                    block,
                    &mut report,
                    supports_explicit_prompt_cache && projectable_block,
                );
            }
        }
    }
    if let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if let Some(tool) = tool.as_object_mut() {
                collect_relocatable_cache_control(
                    tool,
                    &mut report,
                    supports_explicit_prompt_cache,
                    &mut relocatable_prefix_markers,
                );
            }
        }
    }
    if let Some(messages) = object.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            sanitize_agent_bridge_message(message, &mut report, supports_explicit_prompt_cache);
        }
    }
    if relocatable_prefix_markers > 0 {
        if attach_relocated_prefix_cache_breakpoint(object) {
            report.cache_breakpoints_relocated = report
                .cache_breakpoints_relocated
                .saturating_add(relocatable_prefix_markers);
        } else {
            report.cache_control_unmapped = report
                .cache_control_unmapped
                .saturating_add(relocatable_prefix_markers);
        }
    }
    (sanitized, report)
}

fn sanitize_agent_bridge_message(
    message: &mut Value,
    report: &mut AgentBridgeRequestSanitizeReport,
    supports_explicit_prompt_cache: bool,
) {
    let is_assistant = message.get("role").and_then(Value::as_str) == Some("assistant");
    let has_bridge_carrier = is_assistant
        && message
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|blocks| blocks.iter().any(block_has_agent_bridge_carrier));
    let Some(content) = message.get_mut("content") else {
        return;
    };
    strip_content_cache_control(
        content,
        report,
        supports_explicit_prompt_cache && !is_assistant,
    );
    let Some(blocks) = content.as_array_mut() else {
        return;
    };
    blocks.retain(|block| {
        let thinking_block = matches!(
            block.get("type").and_then(Value::as_str),
            Some("thinking" | "redacted_thinking")
        );
        if !thinking_block {
            return true;
        }
        if has_bridge_carrier || block_has_agent_bridge_carrier(block) {
            report.bridge_thinking_blocks_removed =
                report.bridge_thinking_blocks_removed.saturating_add(1);
        } else {
            report.unsupported_thinking_blocks =
                report.unsupported_thinking_blocks.saturating_add(1);
        }
        false
    });
}

fn block_has_agent_bridge_carrier(block: &Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("redacted_thinking") => block
            .get("data")
            .and_then(Value::as_str)
            .is_some_and(is_agent_bridge_carrier),
        Some("thinking") => block
            .get("signature")
            .and_then(Value::as_str)
            .is_some_and(is_agent_bridge_carrier),
        _ => false,
    }
}

fn is_agent_bridge_carrier(value: &str) -> bool {
    value.starts_with(AGENT_BRIDGE_HANDLE_PREFIX)
        || value.starts_with(AGENT_BRIDGE_REASONING_PREFIX)
}

fn take_cache_control(
    object: &mut Map<String, Value>,
    report: &mut AgentBridgeRequestSanitizeReport,
) -> Option<Value> {
    let value = object.remove("cache_control");
    if value.is_some() {
        report.cache_control_removed = report.cache_control_removed.saturating_add(1);
    }
    value
}

fn cache_control_is_projectable(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|cache_control| cache_control.get("type"))
        .and_then(Value::as_str)
        == Some("ephemeral")
}

fn insert_agent_bridge_prompt_cache_breakpoint(object: &mut Map<String, Value>) {
    object.insert(
        AETHER_AGENT_BRIDGE_PROMPT_CACHE_BREAKPOINT_FIELD.to_string(),
        json!({"mode": "explicit"}),
    );
}

fn project_cache_control(
    object: &mut Map<String, Value>,
    report: &mut AgentBridgeRequestSanitizeReport,
    allow_projection: bool,
) {
    let Some(cache_control) = take_cache_control(object, report) else {
        return;
    };
    if allow_projection && cache_control_is_projectable(&cache_control) {
        insert_agent_bridge_prompt_cache_breakpoint(object);
        report.cache_breakpoints_projected = report.cache_breakpoints_projected.saturating_add(1);
    } else {
        report.cache_control_unmapped = report.cache_control_unmapped.saturating_add(1);
    }
}

fn collect_relocatable_cache_control(
    object: &mut Map<String, Value>,
    report: &mut AgentBridgeRequestSanitizeReport,
    allow_relocation: bool,
    relocatable_markers: &mut usize,
) {
    let Some(cache_control) = take_cache_control(object, report) else {
        return;
    };
    if allow_relocation && cache_control_is_projectable(&cache_control) {
        *relocatable_markers = relocatable_markers.saturating_add(1);
    } else {
        report.cache_control_unmapped = report.cache_control_unmapped.saturating_add(1);
    }
}

fn strip_content_cache_control(
    content: &mut Value,
    report: &mut AgentBridgeRequestSanitizeReport,
    allow_projection: bool,
) {
    let Some(blocks) = content.as_array_mut() else {
        return;
    };
    for block in blocks {
        let Some(object) = block.as_object_mut() else {
            continue;
        };
        let projectable_block = matches!(
            object.get("type").and_then(Value::as_str),
            Some("text" | "image" | "document")
        );
        let is_tool_result = object.get("type").and_then(Value::as_str) == Some("tool_result");
        project_cache_control(object, report, allow_projection && projectable_block);
        if is_tool_result {
            if let Some(content) = object.get_mut("content") {
                strip_content_cache_control(content, report, false);
                let Some(parts) = content.as_array_mut() else {
                    continue;
                };
                let mut has_unsupported_part = false;
                for part in parts {
                    let Some(part_object) = part.as_object() else {
                        has_unsupported_part = true;
                        continue;
                    };
                    match part_object.get("type").and_then(Value::as_str) {
                        Some("tool_reference")
                            if part_object
                                .get("tool_name")
                                .and_then(Value::as_str)
                                .is_some_and(|name| !name.trim().is_empty()) =>
                        {
                            let text =
                                serde_json::to_string(part).unwrap_or_else(|_| part.to_string());
                            *part = json!({
                                "type": "text",
                                "text": text,
                            });
                            report.tool_reference_blocks_projected =
                                report.tool_reference_blocks_projected.saturating_add(1);
                        }
                        Some("text" | "image" | "document" | "file") => {}
                        _ => has_unsupported_part = true,
                    }
                }
                if has_unsupported_part {
                    report.unsupported_tool_result_content_arrays = report
                        .unsupported_tool_result_content_arrays
                        .saturating_add(1);
                }
            }
        }
    }
}

fn attach_relocated_prefix_cache_breakpoint(request: &mut Map<String, Value>) -> bool {
    if let Some(system) = request.get_mut("system") {
        match system {
            Value::String(text) if !text.trim().is_empty() => {
                let text = std::mem::take(text);
                let mut block = json!({"type": "text", "text": text});
                insert_agent_bridge_prompt_cache_breakpoint(
                    block.as_object_mut().expect("text block object"),
                );
                *system = Value::Array(vec![block]);
                return true;
            }
            Value::Array(blocks) => {
                if let Some(block) = blocks.iter_mut().rev().find_map(|block| {
                    let block = block.as_object_mut()?;
                    let is_text =
                        block.get("type").and_then(Value::as_str).unwrap_or("text") == "text";
                    let has_text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.trim().is_empty());
                    (is_text && has_text).then_some(block)
                }) {
                    insert_agent_bridge_prompt_cache_breakpoint(block);
                    return true;
                }
            }
            _ => {}
        }
    }

    let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    for message in messages {
        let Some(message) = message.as_object_mut() else {
            continue;
        };
        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            continue;
        }
        let Some(content) = message.get_mut("content") else {
            continue;
        };
        match content {
            Value::String(text) if !text.trim().is_empty() => {
                let text = std::mem::take(text);
                let mut block = json!({"type": "text", "text": text});
                insert_agent_bridge_prompt_cache_breakpoint(
                    block.as_object_mut().expect("text block object"),
                );
                *content = Value::Array(vec![block]);
                return true;
            }
            Value::Array(blocks) => {
                if let Some(block) = blocks.iter_mut().find_map(|block| {
                    let block = block.as_object_mut()?;
                    matches!(
                        block.get("type").and_then(Value::as_str),
                        Some("text" | "image" | "document")
                    )
                    .then_some(block)
                }) {
                    insert_agent_bridge_prompt_cache_breakpoint(block);
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub fn scan_claude_agent_bridge_history(body: &Value) -> Vec<AgentBridgeHistoryMessage> {
    body.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(message_index, message)| {
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            let mut handles = Vec::new();
            let mut reasoning_fallbacks = Vec::new();
            let mut has_tool_use = false;
            for block in message_content_blocks(message.get("content")) {
                let Some(block) = block.as_object() else {
                    continue;
                };
                match block.get("type").and_then(Value::as_str) {
                    Some("redacted_thinking") => {
                        if let Some(data) = block.get("data").and_then(Value::as_str) {
                            collect_agent_bridge_carrier(
                                data,
                                &mut handles,
                                &mut reasoning_fallbacks,
                            );
                        }
                    }
                    Some("thinking") => {
                        if let Some(signature) = block.get("signature").and_then(Value::as_str) {
                            collect_agent_bridge_carrier(
                                signature,
                                &mut handles,
                                &mut reasoning_fallbacks,
                            );
                        }
                    }
                    Some("tool_use") => has_tool_use = true,
                    _ => {}
                }
            }
            Some(AgentBridgeHistoryMessage {
                message_index,
                handles,
                reasoning_fallbacks,
                projected_items: {
                    let mut projected_message = message.clone();
                    sanitize_agent_bridge_message(
                        &mut projected_message,
                        &mut AgentBridgeRequestSanitizeReport::default(),
                        false,
                    );
                    project_claude_message_to_openai_responses_input(&projected_message)
                        .unwrap_or_default()
                },
                has_tool_use,
            })
        })
        .collect()
}

fn message_content_blocks(content: Option<&Value>) -> Vec<&Value> {
    match content {
        Some(Value::Array(blocks)) => blocks.iter().collect(),
        Some(Value::Object(_)) => content.into_iter().collect(),
        _ => Vec::new(),
    }
}

fn collect_agent_bridge_carrier(
    carrier: &str,
    handles: &mut Vec<String>,
    reasoning_fallbacks: &mut Vec<String>,
) {
    let carrier = carrier.trim();
    if carrier
        .strip_prefix(AGENT_BRIDGE_HANDLE_PREFIX)
        .is_some_and(|value| !value.is_empty())
    {
        handles.push(carrier.to_string());
    } else if carrier
        .strip_prefix(AGENT_BRIDGE_REASONING_PREFIX)
        .is_some_and(|value| !value.is_empty())
    {
        reasoning_fallbacks.push(carrier.to_string());
    }
}

pub fn project_claude_message_to_openai_responses_input(message: &Value) -> Option<Vec<Value>> {
    let request = json!({
        "model": "agent-bridge-projection",
        "max_tokens": 128,
        "messages": [message.clone()],
    });
    let canonical = from_claude_to_canonical_request(&request)?;
    canonical_to_openai_responses_request(&canonical, "agent-bridge-projection", false)?
        .get("input")?
        .as_array()
        .cloned()
}

pub fn agent_bridge_prompt_cache_identity(material: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("https://aether.local/agent-bridge/v1/{material}").as_bytes(),
    )
    .to_string()
}

pub fn apply_agent_bridge_codex_overlay(body: &mut Value) -> bool {
    apply_agent_bridge_codex_overlay_with_report(body).changed
}

pub fn apply_agent_bridge_codex_overlay_with_report(
    body: &mut Value,
) -> AgentBridgeCodexOverlayReport {
    let Some(object) = body.as_object_mut() else {
        return AgentBridgeCodexOverlayReport::default();
    };
    let mut report = project_claude_code_web_tools_to_hosted_search(object);
    let strict_changed = apply_strict_to_compatible_tools(object.get_mut("tools"));
    let mut changed = report.changed || strict_changed;
    let Some(input) = object.get_mut("input").and_then(Value::as_array_mut) else {
        report.changed = changed;
        return report;
    };
    let already_present = input.iter().any(|item| {
        item.get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|part| {
                part.get("text").and_then(Value::as_str) == Some(AGENT_BRIDGE_CODEX_OVERLAY)
            })
    });
    if already_present {
        report.changed = changed;
        return report;
    }
    input.insert(
        0,
        json!({
            "type": "message",
            "role": "developer",
            "content": [{
                "type": "input_text",
                "text": AGENT_BRIDGE_CODEX_OVERLAY,
            }],
        }),
    );
    changed = true;
    report.changed = changed;
    report
}

fn project_claude_code_web_tools_to_hosted_search(
    object: &mut Map<String, Value>,
) -> AgentBridgeCodexOverlayReport {
    let mut report = AgentBridgeCodexOverlayReport::default();
    let Some(tools) = object.get_mut("tools").and_then(Value::as_array_mut) else {
        return report;
    };
    let mut has_hosted_web_search = tools.iter().any(is_responses_hosted_web_search_tool);
    let original = std::mem::take(tools);
    let mut projected = Vec::with_capacity(original.len());
    for tool in original {
        if is_claude_code_web_function_tool(&tool) {
            report.hosted_web_tools_projected += 1;
            report.changed = true;
            if !has_hosted_web_search {
                projected.push(json!({"type": "web_search"}));
                has_hosted_web_search = true;
                report.hosted_web_search_inserted = true;
            }
            continue;
        }
        projected.push(tool);
    }
    *tools = projected;

    if report.hosted_web_tools_projected > 0
        && has_hosted_web_search
        && object
            .get("tool_choice")
            .is_some_and(tool_choice_targets_claude_code_web_tool)
    {
        object.insert("tool_choice".to_string(), json!({"type": "web_search"}));
        report.hosted_web_tool_choice_projected = true;
        report.changed = true;
    }
    report
}

fn is_claude_code_web_function_tool(tool: &Value) -> bool {
    tool.get("type").and_then(Value::as_str) == Some("function")
        && tool
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(is_claude_code_hosted_web_tool_name)
}

fn is_responses_hosted_web_search_tool(tool: &Value) -> bool {
    matches!(
        tool.get("type").and_then(Value::as_str),
        Some("web_search" | "web_search_preview")
    )
}

fn tool_choice_targets_claude_code_web_tool(choice: &Value) -> bool {
    let Some(choice) = choice.as_object() else {
        return false;
    };
    if choice.get("type").and_then(Value::as_str) != Some("function") {
        return false;
    }
    choice
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            choice
                .get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
        })
        .is_some_and(is_claude_code_hosted_web_tool_name)
}

fn is_claude_code_hosted_web_tool_name(name: &str) -> bool {
    CLAUDE_CODE_HOSTED_WEB_TOOL_NAMES.contains(&name)
}

fn apply_strict_to_compatible_tools(tools: Option<&mut Value>) -> bool {
    let Some(tools) = tools.and_then(Value::as_array_mut) else {
        return false;
    };
    let mut changed = false;
    for tool in tools {
        let Some(tool) = tool.as_object_mut() else {
            continue;
        };
        if tool.get("type").and_then(Value::as_str) != Some("function")
            || !tool
                .get("parameters")
                .is_some_and(strict_json_schema_is_compatible)
        {
            continue;
        }
        if tool.get("strict").and_then(Value::as_bool) != Some(true) {
            tool.insert("strict".to_string(), Value::Bool(true));
            changed = true;
        }
    }
    changed
}

fn strict_json_schema_is_compatible(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return true;
    };
    if let Some(format) = object.get("format") {
        let Some(format) = format.as_str() else {
            return false;
        };
        if !OPENAI_STRICT_JSON_SCHEMA_STRING_FORMATS.contains(&format) {
            return false;
        }
    }
    let object_typed = object.get("type").is_some_and(|value| match value {
        Value::String(value) => value == "object",
        Value::Array(values) => values.iter().any(|value| value.as_str() == Some("object")),
        _ => false,
    }) || object.contains_key("properties");
    if object_typed {
        if object.get("additionalProperties").and_then(Value::as_bool) != Some(false) {
            return false;
        }
        let properties = object
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let required = object
            .get("required")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        if properties
            .keys()
            .any(|key| !required.contains(key.as_str()))
        {
            return false;
        }
    }
    for key in ["properties", "$defs", "definitions"] {
        if object
            .get(key)
            .and_then(Value::as_object)
            .is_some_and(|values| {
                values
                    .values()
                    .any(|value| !strict_json_schema_is_compatible(value))
            })
        {
            return false;
        }
    }
    if object
        .get("items")
        .is_some_and(|value| !strict_json_schema_is_compatible(value))
    {
        return false;
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if object
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|values| {
                values
                    .iter()
                    .any(|value| !strict_json_schema_is_compatible(value))
            })
        {
            return false;
        }
    }
    true
}

pub fn infer_agent_bridge_message_phase(items: &mut [Value], has_tool_use: bool) -> bool {
    let phase = if has_tool_use {
        "commentary"
    } else {
        "final_answer"
    };
    let mut changed = false;
    for item in items {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        if item.get("type").and_then(Value::as_str) == Some("message")
            && item.get("role").and_then(Value::as_str) == Some("assistant")
            && item.get("phase").and_then(Value::as_str) != Some(phase)
        {
            item.insert("phase".to_string(), Value::String(phase.to_string()));
            changed = true;
        }
    }
    changed
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentBridgeProjectionSanitizeReport {
    pub encrypted_reasoning_removed: usize,
    pub phase_removed: usize,
    pub replay_only_items_removed: usize,
    pub optional_metadata_removed: usize,
}

pub fn sanitize_openai_responses_for_claude_projection(
    response: &Value,
) -> (Value, AgentBridgeProjectionSanitizeReport) {
    let Some(source) = response.as_object() else {
        return (
            response.clone(),
            AgentBridgeProjectionSanitizeReport::default(),
        );
    };
    let mut report = AgentBridgeProjectionSanitizeReport::default();
    let mut projected = Map::new();
    for key in [
        "id",
        "object",
        "model",
        "output",
        "usage",
        "status",
        "error",
        "incomplete_details",
    ] {
        if let Some(value) = source.get(key) {
            projected.insert(key.to_string(), value.clone());
        }
    }
    report.optional_metadata_removed = source.len().saturating_sub(projected.len());
    if let Some(output) = projected.get_mut("output").and_then(Value::as_array_mut) {
        let mut visible = Vec::with_capacity(output.len());
        for mut item in std::mem::take(output) {
            let Some(object) = item.as_object_mut() else {
                visible.push(item);
                continue;
            };
            match object.get("type").and_then(Value::as_str) {
                Some("reasoning") => {
                    if object.remove("encrypted_content").is_some() {
                        report.encrypted_reasoning_removed += 1;
                    }
                    retain_fields(object, &["type", "id", "status", "summary"], &mut report);
                    let has_summary = object
                        .get("summary")
                        .and_then(Value::as_array)
                        .is_some_and(|summary| !summary.is_empty());
                    if has_summary {
                        visible.push(item);
                    } else {
                        report.replay_only_items_removed += 1;
                    }
                }
                Some("message") => {
                    if object.remove("phase").is_some() {
                        report.phase_removed += 1;
                    }
                    sanitize_message_content_for_claude_projection(object, &mut report);
                    retain_fields(
                        object,
                        &["type", "id", "status", "role", "content"],
                        &mut report,
                    );
                    visible.push(item);
                }
                Some("compaction" | "context_compaction" | "response.compaction") => {
                    report.replay_only_items_removed += 1;
                }
                Some("web_search_call") => {
                    // Responses 已在上游执行该工具；Claude Code 只应看到最终文本，
                    // 完整调用仍由 Agent Bridge 状态保存并在下一轮原样回放。
                    report.replay_only_items_removed += 1;
                }
                _ => visible.push(item),
            }
        }
        *output = visible;
    }
    (Value::Object(projected), report)
}

fn sanitize_message_content_for_claude_projection(
    message: &mut Map<String, Value>,
    report: &mut AgentBridgeProjectionSanitizeReport,
) {
    let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    for block in content {
        let Some(block) = block.as_object_mut() else {
            continue;
        };
        match block.get("type").and_then(Value::as_str) {
            Some("output_text") => retain_fields(block, &["type", "text"], report),
            Some("refusal") => retain_fields(block, &["type", "refusal"], report),
            _ => {}
        }
    }
}

fn retain_fields(
    object: &mut Map<String, Value>,
    fields: &[&str],
    report: &mut AgentBridgeProjectionSanitizeReport,
) {
    let before = object.len();
    object.retain(|key, _| fields.contains(&key.as_str()));
    report.optional_metadata_removed += before.saturating_sub(object.len());
}

pub fn validate_agent_bridge_function_call_arguments(response: &Value) -> Result<(), String> {
    for (index, item) in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            continue;
        }
        let arguments = item
            .get("arguments")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("output[{index}] function_call has empty arguments"))?;
        let parsed = serde_json::from_str::<Value>(arguments)
            .map_err(|_| format!("output[{index}] function_call arguments are not valid JSON"))?;
        if !parsed.is_object() {
            return Err(format!(
                "output[{index}] function_call arguments must be a JSON object"
            ));
        }
    }
    Ok(())
}

pub fn agent_bridge_response_handle_from_report_context(report_context: &Value) -> Option<&str> {
    report_context
        .get(AGENT_BRIDGE_REPORT_CONTEXT_FIELD)
        .and_then(Value::as_object)
        .and_then(|bridge| bridge.get("response_handle"))
        .and_then(Value::as_str)
        .filter(|value| value.starts_with(AGENT_BRIDGE_HANDLE_PREFIX))
}

pub fn apply_agent_bridge_response_carriers_from_report_context(
    response: &mut Value,
    report_context: &Value,
) -> bool {
    let Some(bridge) = report_context
        .get(AGENT_BRIDGE_REPORT_CONTEXT_FIELD)
        .and_then(Value::as_object)
    else {
        return false;
    };
    let handle = bridge
        .get("response_handle")
        .and_then(Value::as_str)
        .filter(|value| value.starts_with(AGENT_BRIDGE_HANDLE_PREFIX));
    let fallbacks = bridge
        .get("response_reasoning_fallbacks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| value.starts_with(AGENT_BRIDGE_REASONING_PREFIX))
        .collect::<Vec<_>>();
    if handle.is_none() && fallbacks.is_empty() {
        return false;
    }
    let Some(content) = response.get_mut("content").and_then(Value::as_array_mut) else {
        return false;
    };
    let mut carriers = Vec::with_capacity(usize::from(handle.is_some()) + fallbacks.len());
    if let Some(handle) = handle {
        carriers.push(json!({"type": "redacted_thinking", "data": handle}));
    }
    carriers.extend(
        fallbacks
            .into_iter()
            .map(|fallback| json!({"type": "redacted_thinking", "data": fallback})),
    );
    carriers.append(content);
    *content = carriers;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::context::FormatContext;
    use crate::formats::conversion::request::normalize_openai_responses_request_to_openai_chat_request;
    use crate::formats::registry::convert_request;

    #[test]
    fn agent_bridge_sanitizes_nested_message_metadata_before_claude_projection() {
        let response = json!({
            "id": "resp_nested_metadata",
            "object": "response",
            "model": "gpt-5.6-terra",
            "status": "completed",
            "error": null,
            "incomplete_details": null,
            "metadata": {"optional": true},
            "output": [{
                "id": "msg_nested_metadata",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "phase": "final_answer",
                "content": [{
                    "type": "output_text",
                    "text": "done",
                    "annotations": [],
                    "logprobs": []
                }]
            }],
            "usage": {
                "input_tokens": 9,
                "input_tokens_details": {"cached_tokens": 8, "cache_write_tokens": 0},
                "output_tokens": 1,
                "output_tokens_details": {"reasoning_tokens": 0},
                "total_tokens": 10
            }
        });
        let (projected, report) = sanitize_openai_responses_for_claude_projection(&response);
        assert_eq!(report.phase_removed, 1);
        assert!(report.optional_metadata_removed >= 3);
        let text = &projected["output"][0]["content"][0];
        assert!(text.get("annotations").is_none());
        assert!(text.get("logprobs").is_none());
        let converted = crate::formats::registry::convert_response_pure(
            "openai:responses",
            "claude:messages",
            &projected,
        )
        .expect("audited bridge projection should drop replay-only nested metadata");
        assert_eq!(converted.value["content"][0]["text"], "done");
    }

    #[test]
    fn agent_bridge_sanitizes_carriers_and_cache_breakpoints_before_strict_conversion() {
        let body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "cache_control": {"type": "ephemeral"},
            "system": [{"type": "text", "text": "system", "cache_control": {"type": "ephemeral"}}],
            "tools": [{
                "name": "Read",
                "description": "read",
                "cache_control": {"type": "ephemeral"},
                "input_schema": {
                    "type": "object",
                    "properties": {"cache_control": {"type": "string"}}
                }
            }],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "inspect", "cache_control": {"type": "ephemeral"}}]},
                {"role": "assistant", "content": [
                    {"type": "redacted_thinking", "data": "aether-abr1.handle"},
                    {"type": "thinking", "thinking": "plan", "signature": "aether-ars1.sealed"},
                    {"type": "text", "text": "checking", "cache_control": {"type": "ephemeral"}},
                    {"type": "tool_use", "id": "call_1", "name": "Read", "input": {"cache_control": "schema-value"}}
                ]}
            ]
        });
        let (sanitized, report) = sanitize_claude_request_for_agent_bridge_conversion(&body);
        assert_eq!(report.cache_control_removed, 5);
        assert_eq!(report.cache_breakpoints_projected, 0);
        assert_eq!(report.cache_breakpoints_relocated, 0);
        assert_eq!(report.cache_control_unmapped, 5);
        assert_eq!(report.bridge_thinking_blocks_removed, 2);
        assert_eq!(
            sanitized["tools"][0]["input_schema"]["properties"]["cache_control"]["type"],
            "string"
        );
        assert_eq!(
            sanitized["messages"][1]["content"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let converted = convert_request(
            "claude:messages",
            "openai:responses",
            &sanitized,
            &FormatContext::default().with_mapped_model("gpt-5.3-codex"),
        )
        .expect("audited bridge projection should pass the strict converter");
        let input = converted["input"].as_array().unwrap();
        assert!(input
            .iter()
            .any(|item| item.get("role") == Some(&json!("assistant"))));
        assert!(input
            .iter()
            .any(|item| item.get("call_id") == Some(&json!("call_1"))));
    }

    #[test]
    fn agent_bridge_projects_claude_cache_controls_to_gpt56_responses_breakpoints() {
        let body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "system": "stable system prompt",
            "tools": [{
                "name": "Read",
                "description": "read",
                "cache_control": {"type": "ephemeral"},
                "input_schema": {"type": "object", "properties": {}}
            }],
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "stable project context",
                    "cache_control": {"type": "ephemeral"}
                }]
            }]
        });

        let (sanitized, report) =
            sanitize_claude_request_for_agent_bridge_conversion_with_prompt_cache(&body, true);
        assert_eq!(report.cache_control_removed, 2);
        assert_eq!(report.cache_breakpoints_projected, 1);
        assert_eq!(report.cache_breakpoints_relocated, 1);
        assert_eq!(report.cache_control_unmapped, 0);
        assert!(!sanitized.to_string().contains("cache_control"));

        let converted = convert_request(
            "claude:messages",
            "openai:responses",
            &sanitized,
            &FormatContext::default().with_mapped_model("gpt-5.6-terra"),
        )
        .expect("GPT-5.6 bridge projection should preserve cache breakpoints");
        let input = converted["input"].as_array().expect("Responses input");
        let developer_breakpoint = input
            .iter()
            .find(|item| item.get("role") == Some(&json!("developer")))
            .and_then(|item| item.get("content"))
            .and_then(Value::as_array)
            .and_then(|content| content.first())
            .and_then(|part| part.get("prompt_cache_breakpoint"));
        let user_breakpoint = input
            .iter()
            .find(|item| item.get("role") == Some(&json!("user")))
            .and_then(|item| item.get("content"))
            .and_then(Value::as_array)
            .and_then(|content| content.first())
            .and_then(|part| part.get("prompt_cache_breakpoint"));
        assert_eq!(developer_breakpoint, Some(&json!({"mode": "explicit"})));
        assert_eq!(user_breakpoint, Some(&json!({"mode": "explicit"})));
        let converted_json = converted.to_string();
        assert!(!converted_json.contains("cache_control"));
        assert!(!converted_json.contains(AETHER_AGENT_BRIDGE_PROMPT_CACHE_BREAKPOINT_FIELD));
        crate::validate_openai_prompt_cache_request(
            "openai:responses",
            "gpt-5.6-terra",
            &converted,
        )
        .expect("projected GPT-5.6 cache contract should be valid");
    }

    #[test]
    fn agent_bridge_keeps_older_responses_models_on_implicit_cache_fallback() {
        let body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "system": [{
                "type": "text",
                "text": "stable system prompt",
                "cache_control": {"type": "ephemeral"}
            }],
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (sanitized, report) =
            sanitize_claude_request_for_agent_bridge_conversion_with_prompt_cache(&body, false);
        assert_eq!(report.cache_control_removed, 1);
        assert_eq!(report.cache_breakpoints_projected, 0);
        assert_eq!(report.cache_control_unmapped, 1);
        let converted = convert_request(
            "claude:messages",
            "openai:responses",
            &sanitized,
            &FormatContext::default().with_mapped_model("gpt-5.5"),
        )
        .expect("older models should retain implicit prompt caching fallback");
        assert!(!converted.to_string().contains("prompt_cache_breakpoint"));
    }

    #[test]
    fn agent_bridge_drops_unrestorable_thinking_and_preserves_supported_tool_results() {
        let body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "private", "signature": "upstream-signature"},
                        {"type": "text", "text": "visible"},
                        {"type": "tool_use", "id": "call_1", "name": "Read", "input": {"path": "a.rs"}}
                    ]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": [{"type": "text", "text": "multi-block"}]
                    }]
                }
            ]
        });
        let (sanitized, report) = sanitize_claude_request_for_agent_bridge_conversion(&body);
        assert_eq!(report.bridge_thinking_blocks_removed, 0);
        assert_eq!(report.unsupported_thinking_blocks, 1);
        assert_eq!(report.unsupported_tool_result_content_arrays, 0);
        assert_eq!(
            sanitized["messages"][0]["content"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(sanitized["messages"][0]["content"]
            .as_array()
            .unwrap()
            .iter()
            .all(|block| block.get("type") != Some(&json!("thinking"))));

        let converted = convert_request(
            "claude:messages",
            "openai:responses",
            &sanitized,
            &FormatContext::default().with_mapped_model("gpt-5.6"),
        )
        .expect("representable multi-block tool results should pass strict conversion");
        assert_eq!(
            converted["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item.get("type") == Some(&json!("function_call_output")))
                .unwrap()["output"],
            "multi-block"
        );
    }

    #[test]
    fn agent_bridge_projects_claude_code_tool_references_without_dropping_names() {
        let body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "redacted_thinking", "data": "aether-abr1.handle"},
                        {"type": "thinking", "thinking": "summary", "signature": "upstream-signature"},
                        {"type": "tool_use", "id": "call_1", "name": "ToolSearch", "input": {"query": "select:Read"}}
                    ]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_1",
                        "content": [
                            {"type": "tool_reference", "tool_name": "Read"},
                            {"type": "tool_reference", "tool_name": "TaskList"}
                        ]
                    }]
                }
            ]
        });

        let (sanitized, report) = sanitize_claude_request_for_agent_bridge_conversion(&body);
        assert_eq!(report.bridge_thinking_blocks_removed, 2);
        assert_eq!(report.unsupported_thinking_blocks, 0);
        assert_eq!(report.tool_reference_blocks_projected, 2);
        assert_eq!(report.unsupported_tool_result_content_arrays, 0);

        let converted = convert_request(
            "claude:messages",
            "openai:responses",
            &sanitized,
            &FormatContext::default().with_mapped_model("gpt-5.6"),
        )
        .expect("Claude Code tool references should use the audited bridge projection");
        let output = converted["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item.get("type") == Some(&json!("function_call_output")))
            .and_then(|item| item.get("output"))
            .and_then(Value::as_str)
            .unwrap();
        assert!(output.contains("\"tool_name\":\"Read\""));
        assert!(output.contains("\"tool_name\":\"TaskList\""));
    }

    #[test]
    fn agent_bridge_scans_carriers_and_projects_visible_history() {
        let body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "redacted_thinking", "data": "aether-abr1.handle-1"},
                    {"type": "redacted_thinking", "data": "aether-ars1.reasoning-1"},
                    {"type": "text", "text": "checking"},
                    {"type": "tool_use", "id": "call_1", "name": "Read", "input": {"path": "a.rs"}}
                ]
            }]
        });
        let history = scan_claude_agent_bridge_history(&body);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].handles, ["aether-abr1.handle-1"]);
        assert_eq!(history[0].reasoning_fallbacks, ["aether-ars1.reasoning-1"]);
        assert!(history[0].has_tool_use);
        assert_eq!(history[0].projected_items[0]["type"], "message");
        assert_eq!(history[0].projected_items[1]["call_id"], "call_1");
    }

    #[test]
    fn agent_bridge_response_carriers_precede_visible_content() {
        let mut response = json!({"content": [{"type": "text", "text": "done"}]});
        let context = json!({
            "agent_bridge": {
                "response_handle": "aether-abr1.handle",
                "response_reasoning_fallbacks": ["aether-ars1.fallback"]
            }
        });
        assert!(apply_agent_bridge_response_carriers_from_report_context(
            &mut response,
            &context
        ));
        assert_eq!(response["content"][0]["data"], "aether-abr1.handle");
        assert_eq!(response["content"][1]["data"], "aether-ars1.fallback");
        assert_eq!(response["content"][2]["text"], "done");
    }

    #[test]
    fn agent_bridge_sanitizes_replay_state_without_hiding_executable_unknown_items() {
        let (projected, report) = sanitize_openai_responses_for_claude_projection(&json!({
            "id": "resp_1",
            "status": "completed",
            "metadata": {"trace": "private"},
            "output": [
                {"type": "reasoning", "id": "rs_1", "encrypted_content": "sealed", "summary": []},
                {"type": "compaction", "id": "cmp_1", "encrypted_content": "compact"},
                {"type": "message", "role": "assistant", "phase": "final_answer", "content": [{"type": "output_text", "text": "done"}]},
                {"type": "future_required_call", "id": "required_1"}
            ]
        }));
        assert_eq!(projected["output"].as_array().unwrap().len(), 2);
        assert_eq!(projected["output"][0]["type"], "message");
        assert_eq!(projected["output"][1]["type"], "future_required_call");
        assert_eq!(report.encrypted_reasoning_removed, 1);
        assert_eq!(report.phase_removed, 1);
        assert_eq!(report.replay_only_items_removed, 2);
    }

    #[test]
    fn agent_bridge_hides_completed_hosted_web_search_from_claude_client() {
        let (projected, report) = sanitize_openai_responses_for_claude_projection(&json!({
            "id": "resp_web",
            "object": "response",
            "model": "gpt-5.4",
            "status": "completed",
            "output": [
                {
                    "type": "web_search_call",
                    "id": "ws_1",
                    "status": "completed",
                    "action": {"type": "open_page", "url": "https://example.com"}
                },
                {
                    "type": "message",
                    "id": "msg_web",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "page result",
                        "annotations": [{"type": "url_citation", "url": "https://example.com"}]
                    }]
                }
            ]
        }));
        assert_eq!(report.replay_only_items_removed, 1);
        assert_eq!(projected["output"].as_array().unwrap().len(), 1);
        assert_eq!(projected["output"][0]["type"], "message");

        let converted = crate::formats::registry::convert_response_pure(
            "openai:responses",
            "claude:messages",
            &projected,
        )
        .expect("hosted search must project as final Claude text");
        assert_eq!(converted.value["content"].as_array().unwrap().len(), 1);
        assert_eq!(converted.value["content"][0]["type"], "text");
        assert_eq!(converted.value["content"][0]["text"], "page result");
    }

    #[test]
    fn agent_bridge_projects_claude_code_web_tools_to_hosted_search() {
        let mut body = json!({
            "input": [],
            "tools": [
                {"type": "function", "name": "Read", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"], "additionalProperties": false}},
                {"type": "function", "name": "WebFetch", "parameters": {"type": "object", "properties": {"url": {"type": "string", "format": "uri"}, "prompt": {"type": "string"}}, "required": ["url", "prompt"], "additionalProperties": false}},
                {"type": "function", "name": "WebSearch", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"], "additionalProperties": false}}
            ],
            "tool_choice": {"type": "function", "name": "WebFetch"}
        });

        let report = apply_agent_bridge_codex_overlay_with_report(&mut body);
        assert!(report.changed);
        assert_eq!(report.hosted_web_tools_projected, 2);
        assert!(report.hosted_web_search_inserted);
        assert!(report.hosted_web_tool_choice_projected);
        assert_eq!(
            body["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|tool| tool.get("type") == Some(&json!("web_search")))
                .count(),
            1
        );
        let projected_tools = body["tools"].to_string();
        assert!(!projected_tools.contains("WebFetch"));
        assert!(!projected_tools.contains("WebSearch"));
        assert_eq!(body["tool_choice"], json!({"type": "web_search"}));
        assert_eq!(body["tools"][0]["name"], "Read");
        assert_eq!(body["tools"][0]["strict"], true);
        assert!(body["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("upstream hosted web_search"));
    }

    #[test]
    fn agent_bridge_web_tool_projection_reuses_existing_hosted_search() {
        let mut body = json!({
            "input": [],
            "tools": [
                {"type": "web_search", "search_context_size": "high"},
                {"type": "function", "name": "WebFetch", "parameters": {"type": "object"}}
            ]
        });

        let report = apply_agent_bridge_codex_overlay_with_report(&mut body);
        assert_eq!(report.hosted_web_tools_projected, 1);
        assert!(!report.hosted_web_search_inserted);
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["search_context_size"], "high");
    }

    #[test]
    fn agent_bridge_overlay_sets_strict_only_for_closed_required_schema() {
        let mut body = json!({
            "input": [],
            "tools": [
                {"type": "function", "name": "strict", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"], "additionalProperties": false}},
                {"type": "function", "name": "loose", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}},
                {"type": "function", "name": "UrlLookup", "parameters": {"type": "object", "properties": {"url": {"type": "string", "format": "uri"}, "prompt": {"type": "string"}}, "required": ["url", "prompt"], "additionalProperties": false}},
                {"type": "function", "name": "dated", "parameters": {"type": "object", "properties": {"at": {"type": "string", "format": "date-time"}}, "required": ["at"], "additionalProperties": false}}
            ]
        });
        assert!(apply_agent_bridge_codex_overlay(&mut body));
        assert_eq!(body["tools"][0]["strict"], true);
        assert!(body["tools"][1].get("strict").is_none());
        assert!(body["tools"][2].get("strict").is_none());
        assert_eq!(
            body["tools"][2]["parameters"]["properties"]["url"]["format"],
            "uri"
        );
        assert_eq!(body["tools"][3]["strict"], true);
        assert_eq!(body["input"][0]["role"], "developer");
    }

    #[test]
    fn agent_bridge_report_serializes_kebab_case_primary_state() {
        let report = AgentBridgeCompatibilityReport {
            primary_state: AgentBridgePrimaryState::ReasoningFallback,
            phase_inferred: true,
            ..AgentBridgeCompatibilityReport::default()
        };
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["primary_state"], "reasoning-fallback");
        assert_eq!(value["phase_inferred"], true);
    }

    #[test]
    fn agent_bridge_openai_chat_history_keeps_reasoning_with_tool_calls() {
        let converted = normalize_openai_responses_request_to_openai_chat_request(&json!({
            "model": "gpt-5.3-codex",
            "input": [
                {"role": "user", "content": "inspect"},
                {"type": "reasoning", "summary": [{"type": "summary_text", "text": "plan"}]},
                {"type": "message", "role": "assistant", "content": "checking"},
                {"type": "function_call", "call_id": "call_1", "name": "Read", "arguments": "{\"path\":\"a.rs\"}"}
            ]
        }))
        .expect("responses history should project to chat");
        let assistant = &converted["messages"][1];
        assert_eq!(assistant["reasoning_content"], "plan");
        assert_eq!(assistant["content"], "checking");
        assert_eq!(assistant["tool_calls"][0]["id"], "call_1");
    }
}
