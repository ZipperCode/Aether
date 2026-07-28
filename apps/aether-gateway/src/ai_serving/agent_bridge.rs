use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read};
use std::time::Duration;

use super::{
    agent_bridge_prompt_cache_identity, apply_agent_bridge_codex_overlay_with_report,
    infer_agent_bridge_message_phase, normalize_api_format_alias,
    openai_model_supports_prompt_cache_options, sanitize_openai_responses_for_claude_projection,
    scan_claude_agent_bridge_history, validate_agent_bridge_function_call_arguments,
    AgentBridgeCompatibilityReport, AgentBridgePrimaryState, AgentBridgeRequestSanitizeReport,
    AGENT_BRIDGE_HANDLE_PREFIX, AGENT_BRIDGE_REASONING_PREFIX, AGENT_BRIDGE_REPORT_CONTEXT_FIELD,
};
use aes_gcm::aead::{Aead, AeadCore, OsRng, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aether_runtime_state::RuntimeState;
use aether_scheduler_core::ClientSessionAffinity;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::{AppState, GatewayError};

type HmacSha256 = Hmac<Sha256>;

const AGENT_BRIDGE_VERSION: u8 = 1;
const AGENT_BRIDGE_RUNTIME_KEY_PREFIX: &str = "agent_bridge:responses:v1:";
const AGENT_BRIDGE_KEY_DOMAIN: &[u8] = b"aether/agent-bridge/v1/aes-256-gcm";
const AGENT_BRIDGE_SCOPE_DOMAIN: &[u8] = b"aether/agent-bridge/v1/scope";
const AGENT_BRIDGE_STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const AGENT_BRIDGE_MAX_ROUND_BYTES: usize = 8 * 1024 * 1024;
const AGENT_BRIDGE_MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
const AGENT_BRIDGE_MAX_REQUEST_HANDLES: usize = 256;
const AGENT_BRIDGE_MAX_FALLBACK_BYTES: usize = 512 * 1024;
const AGENT_BRIDGE_ZSTD_LEVEL: i32 = 3;
const OPENAI_PROMPT_CACHE_BREAKPOINT_CAPABILITIES: &[&str] = &[
    "openai_prompt_cache_breakpoints",
    "prompt_cache_breakpoints",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentBridgeContext {
    pub(crate) session_scope: String,
    pub(crate) user_id: String,
    pub(crate) api_key_id: String,
    pub(crate) provider_id: String,
    pub(crate) endpoint_id: String,
    pub(crate) provider_key_id: String,
    pub(crate) target_format: String,
    pub(crate) mapped_model: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentBridgeRequestInput<'a> {
    pub(crate) client_api_format: &'a str,
    pub(crate) provider_api_format: &'a str,
    pub(crate) mapped_model: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) api_key_id: &'a str,
    pub(crate) provider_id: &'a str,
    pub(crate) endpoint_id: &'a str,
    pub(crate) provider_key_id: &'a str,
    pub(crate) client_session_affinity: Option<&'a ClientSessionAffinity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentBridgeSealedEnvelope {
    version: u8,
    scope_fingerprint: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredResponseItem {
    output_index: usize,
    item: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredResponseState {
    version: u8,
    items: Vec<StoredResponseItem>,
    #[serde(default)]
    omitted_message_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenEnvelopeError {
    ScopeMismatch,
    Invalid,
    Oversized,
}

#[derive(Debug, Clone)]
struct RestoredState {
    state: StoredResponseState,
    serialized_bytes: usize,
}

#[derive(Debug, Clone)]
struct PreparedState {
    state: StoredResponseState,
    truncated: bool,
}

pub(crate) async fn prepare_agent_bridge_request(
    state: &AppState,
    original_claude_body: &Value,
    provider_request_body: &mut Value,
    input: AgentBridgeRequestInput<'_>,
    sanitize_report: AgentBridgeRequestSanitizeReport,
) -> Option<Value> {
    let affinity = input.client_session_affinity?;
    let session_scope = affinity.session_key.as_deref()?.trim();
    let context = AgentBridgeContext {
        session_scope: session_scope.to_string(),
        user_id: input.user_id.trim().to_string(),
        api_key_id: input.api_key_id.trim().to_string(),
        provider_id: input.provider_id.trim().to_string(),
        endpoint_id: input.endpoint_id.trim().to_string(),
        provider_key_id: input.provider_key_id.trim().to_string(),
        target_format: normalize_api_format_alias(input.provider_api_format),
        mapped_model: input.mapped_model.trim().to_string(),
    };
    let mut report = AgentBridgeCompatibilityReport::default();
    if sanitize_report.cache_control_unmapped > 0 {
        report.accept_loss("cache-breakpoint");
    }
    if sanitize_report.cache_breakpoints_relocated > 0 {
        report.accept_loss("cache-breakpoint-relocated");
    }
    if sanitize_report.unsupported_thinking_blocks > 0 {
        report.mark_state_miss("reasoning-continuity");
    }
    if sanitize_report.tool_reference_blocks_projected > 0 {
        report.accept_loss("tool-reference-structure");
    }
    restore_agent_bridge_history(
        state.runtime_state(),
        state.encryption_key(),
        &context,
        original_claude_body,
        provider_request_body,
        &mut report,
    )
    .await;

    force_responses_replay_contract(provider_request_body, &context);
    let overlay_report = apply_agent_bridge_codex_overlay_with_report(provider_request_body);
    if overlay_report.hosted_web_tools_projected > 0 {
        report.accept_loss("claude-code-web-tool-contract");
    }
    let state_handle = new_agent_bridge_handle();
    let response_handle = state
        .encryption_key()
        .filter(|value| !value.is_empty())
        .map(|_| state_handle.clone());

    Some(json!({
        "version": AGENT_BRIDGE_VERSION,
        "enabled": true,
        "context": context,
        "state_handle": state_handle,
        "response_handle": response_handle,
        "compatibility": report,
    }))
}

pub(crate) async fn agent_bridge_request_is_eligible(
    state: &AppState,
    input: AgentBridgeRequestInput<'_>,
) -> bool {
    if normalize_api_format_alias(input.client_api_format) != "claude:messages"
        || normalize_api_format_alias(input.provider_api_format) != "openai:responses"
    {
        return false;
    }
    let Some(affinity) = input.client_session_affinity else {
        return false;
    };
    if affinity
        .client_family
        .as_deref()
        .map(str::trim)
        .is_none_or(|family| !family.eq_ignore_ascii_case("claude_code"))
        || !affinity.has_session_key()
    {
        return false;
    }
    let mode = state
        .read_system_config_json_value("agent_format_bridge_mode")
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::trim).map(str::to_ascii_lowercase))
        .unwrap_or_else(|| "auto".to_string());
    mode != "off"
}

pub(crate) fn agent_bridge_supports_explicit_prompt_cache(
    mapped_model: &str,
    provider_type: &str,
    endpoint_base_url: &str,
    provider_key_capabilities: Option<&Value>,
) -> bool {
    if !openai_model_supports_prompt_cache_options(mapped_model) {
        return false;
    }
    if let Some(enabled) = explicit_prompt_cache_capability(provider_key_capabilities) {
        return enabled;
    }

    // 第三方 Responses 端点经常只实现隐式缓存；仅对官方 OpenAI 地址默认启用新字段。
    provider_type.trim().eq_ignore_ascii_case("openai")
        && url::Url::parse(endpoint_base_url.trim())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| host == "api.openai.com")
}

fn explicit_prompt_cache_capability(capabilities: Option<&Value>) -> Option<bool> {
    let capabilities = capabilities?;
    if let Some(object) = capabilities.as_object() {
        return object.iter().find_map(|(name, value)| {
            OPENAI_PROMPT_CACHE_BREAKPOINT_CAPABILITIES
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
                .then(|| match value {
                    Value::Bool(enabled) => *enabled,
                    Value::String(enabled) => enabled.eq_ignore_ascii_case("true"),
                    Value::Number(enabled) => enabled.as_i64().is_some_and(|enabled| enabled > 0),
                    _ => false,
                })
        });
    }
    capabilities.as_array().and_then(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .any(|name| {
                OPENAI_PROMPT_CACHE_BREAKPOINT_CAPABILITIES
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
            })
            .then_some(true)
    })
}

fn force_responses_replay_contract(body: &mut Value, context: &AgentBridgeContext) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    object.insert("store".to_string(), Value::Bool(false));
    let include = object
        .entry("include".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(include) = include.as_array_mut() {
        let encrypted_content = Value::String("reasoning.encrypted_content".to_string());
        if !include.contains(&encrypted_content) {
            include.push(encrypted_content);
        }
    }
    let cache_material = serde_json::to_string(context).unwrap_or_default();
    object.insert(
        "prompt_cache_key".to_string(),
        Value::String(agent_bridge_prompt_cache_identity(&cache_material)),
    );
}

async fn restore_agent_bridge_history(
    runtime_state: &RuntimeState,
    encryption_key: Option<&str>,
    context: &AgentBridgeContext,
    original_claude_body: &Value,
    provider_request_body: &mut Value,
    report: &mut AgentBridgeCompatibilityReport,
) {
    let history = scan_claude_agent_bridge_history(original_claude_body);
    if history.is_empty() {
        return;
    }
    let Some(input_items) = provider_request_body
        .get_mut("input")
        .and_then(Value::as_array_mut)
    else {
        report.mark_state_miss("visible-history-unavailable");
        return;
    };

    let unique_handles = history
        .iter()
        .flat_map(|message| message.handles.iter())
        .filter(|handle| runtime_key_from_handle(handle).is_some())
        .cloned()
        .collect::<HashSet<_>>();
    let mut ordered_handles = unique_handles.into_iter().collect::<Vec<_>>();
    ordered_handles.sort();
    if ordered_handles.len() > AGENT_BRIDGE_MAX_REQUEST_HANDLES {
        ordered_handles.truncate(AGENT_BRIDGE_MAX_REQUEST_HANDLES);
        report.state_truncated = true;
        report.accept_loss("handle-limit");
    }

    let mut restored_by_handle = HashMap::new();
    if !ordered_handles.is_empty() {
        if let Some(key) = encryption_key.filter(|value| !value.is_empty()) {
            let runtime_keys = ordered_handles
                .iter()
                .filter_map(|handle| runtime_key_from_handle(handle))
                .collect::<Vec<_>>();
            match runtime_state.kv_get_many(&runtime_keys).await {
                Ok(values) => {
                    let mut restored_bytes = 0usize;
                    for ((handle, runtime_key), raw) in
                        ordered_handles.iter().zip(runtime_keys.iter()).zip(values)
                    {
                        let Some(raw) = raw else {
                            continue;
                        };
                        match open_stored_state(&raw, key, context) {
                            Ok(restored)
                                if restored_bytes.saturating_add(restored.serialized_bytes)
                                    <= AGENT_BRIDGE_MAX_REQUEST_BYTES =>
                            {
                                restored_bytes =
                                    restored_bytes.saturating_add(restored.serialized_bytes);
                                restored_by_handle.insert(handle.clone(), restored.state);
                                // 读取成功后续期，Redis 与单实例 Memory 使用相同语义。
                                let _ = runtime_state
                                    .kv_set(runtime_key, raw, Some(AGENT_BRIDGE_STATE_TTL))
                                    .await;
                            }
                            Ok(_) | Err(OpenEnvelopeError::Oversized) => {
                                report.state_truncated = true;
                                report.accept_loss("restore-size-limit");
                            }
                            Err(OpenEnvelopeError::ScopeMismatch) => {
                                report.scope_mismatch = true;
                                report.accept_loss("scope-mismatch");
                            }
                            Err(OpenEnvelopeError::Invalid) => {
                                report.accept_loss("state-corrupt");
                            }
                        }
                    }
                }
                Err(_) => report.accept_loss("state-unavailable"),
            }
        } else {
            report.accept_loss("encryption-key-missing");
        }
    }

    let mut search_from = 0usize;
    for message in history {
        let Some(position) =
            find_value_subsequence(input_items, &message.projected_items, search_from)
        else {
            if !message.handles.is_empty() || !message.reasoning_fallbacks.is_empty() {
                report.mark_state_miss("visible-history-mismatch");
            }
            continue;
        };
        let end = position.saturating_add(message.projected_items.len());
        let restored = message
            .handles
            .iter()
            .find_map(|handle| restored_by_handle.get(handle));
        if let Some(state) = restored {
            let replay = materialize_replay_items(state, &message.projected_items);
            report.restored_item_count = report.restored_item_count.saturating_add(replay.len());
            if !state.omitted_message_indices.is_empty() {
                report.state_truncated = true;
            }
            input_items.splice(position..end, replay.iter().cloned());
            search_from = position.saturating_add(replay.len());
            continue;
        }

        let fallback_items = encryption_key
            .filter(|value| !value.is_empty())
            .map(|key| {
                message
                    .reasoning_fallbacks
                    .iter()
                    .filter_map(|carrier| open_reasoning_fallback(carrier, key, context).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !fallback_items.is_empty() {
            let fallback_len = fallback_items.len();
            input_items.splice(position..position, fallback_items);
            report.restored_item_count = report.restored_item_count.saturating_add(fallback_len);
            report.mark_reasoning_fallback();
            let visible_start = position.saturating_add(fallback_len);
            let visible_end = visible_start.saturating_add(message.projected_items.len());
            if infer_agent_bridge_message_phase(
                &mut input_items[visible_start..visible_end],
                message.has_tool_use,
            ) {
                report.phase_inferred = true;
            }
            search_from = visible_end;
        } else {
            if !message.handles.is_empty() || !message.reasoning_fallbacks.is_empty() {
                report.mark_state_miss("state-miss");
            }
            if infer_agent_bridge_message_phase(
                &mut input_items[position..end],
                message.has_tool_use,
            ) {
                report.phase_inferred = true;
            }
            search_from = end;
        }
    }
}

fn find_value_subsequence(haystack: &[Value], needle: &[Value], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn materialize_replay_items(state: &StoredResponseState, projected: &[Value]) -> Vec<Value> {
    if state.omitted_message_indices.is_empty() {
        return state.items.iter().map(|item| item.item.clone()).collect();
    }
    let mut stored = state
        .items
        .iter()
        .map(|item| (item.output_index, item.item.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut projected_messages = projected
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"));
    for output_index in &state.omitted_message_indices {
        if let Some(message) = projected_messages.next() {
            stored.insert(*output_index, message.clone());
        }
    }
    stored.into_values().collect()
}

pub(crate) fn insert_agent_bridge_report_context(
    extra_fields: &mut Map<String, Value>,
    bridge: Option<Value>,
) {
    if let Some(bridge) = bridge {
        extra_fields.insert(AGENT_BRIDGE_REPORT_CONTEXT_FIELD.to_string(), bridge);
    }
}

pub(crate) async fn finalize_agent_bridge_sync_response(
    state: &AppState,
    report_context: &mut Option<Value>,
    response: Value,
) -> Result<Value, GatewayError> {
    let Some((context, handle)) = bridge_context_and_handle(report_context.as_ref()) else {
        return Ok(response);
    };
    validate_agent_bridge_terminal_response(&response)?;
    validate_agent_bridge_function_call_arguments(&response).map_err(|message| {
        GatewayError::Internal(format!(
            "Agent Bridge rejected malformed tool call: {message}"
        ))
    })?;

    let mut report = bridge_compatibility_report(report_context.as_ref());
    let encryption_key = state.encryption_key().filter(|value| !value.is_empty());
    let mut fallbacks = Vec::new();
    if let Some(key) = encryption_key {
        for item in response
            .get("output")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
        {
            if let Ok(fallback) = seal_reasoning_fallback(item, key, &context) {
                fallbacks.push(fallback);
            }
        }
    }

    let mut saved = false;
    if response_terminal_is_replayable(&response) {
        if let Some(key) = encryption_key {
            match prepare_response_state(&response) {
                Ok(prepared) => {
                    report.state_truncated |= prepared.truncated;
                    match seal_stored_state(&prepared.state, key, &context).and_then(|raw| {
                        let runtime_key =
                            runtime_key_from_handle(&handle).ok_or(OpenEnvelopeError::Invalid)?;
                        Ok((runtime_key, raw))
                    }) {
                        Ok((runtime_key, raw)) => {
                            if state
                                .runtime_state()
                                .kv_set(&runtime_key, raw, Some(AGENT_BRIDGE_STATE_TTL))
                                .await
                                .is_ok()
                            {
                                saved = true;
                                report.saved_item_count = prepared.state.items.len();
                            } else {
                                report.accept_loss("state-unavailable");
                            }
                        }
                        Err(OpenEnvelopeError::Oversized) => {
                            report.state_truncated = true;
                            report.accept_loss("state-size-limit");
                        }
                        Err(_) => report.accept_loss("state-seal-failed"),
                    }
                }
                Err(_) => {
                    report.state_truncated = true;
                    report.accept_loss("state-size-limit");
                }
            }
        } else {
            report.accept_loss("encryption-key-missing");
        }
    }

    if !saved {
        if fallbacks.is_empty() {
            report.mark_state_miss("state-not-saved");
        } else {
            report.mark_reasoning_fallback();
        }
    }
    let (projected_response, projection_report) =
        sanitize_openai_responses_for_claude_projection(&response);
    if projection_report.optional_metadata_removed > 0 {
        report.accept_loss("optional-metadata");
    }
    update_bridge_response_context(report_context, saved.then_some(handle), fallbacks, report);
    Ok(projected_response)
}

fn validate_agent_bridge_terminal_response(response: &Value) -> Result<(), GatewayError> {
    let status = response
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("completed");
    if matches!(status, "failed" | "cancelled") {
        return Err(GatewayError::Internal(format!(
            "Agent Bridge cannot finalize Responses terminal status {status}"
        )));
    }
    Ok(())
}

fn response_terminal_is_replayable(response: &Value) -> bool {
    matches!(
        response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("completed"),
        "completed" | "incomplete"
    )
}

fn prepare_response_state(response: &Value) -> Result<PreparedState, GatewayError> {
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    prepare_state_from_items(output.into_iter().enumerate().collect())
}

fn prepare_state_from_items(items: Vec<(usize, Value)>) -> Result<PreparedState, GatewayError> {
    let mut state = StoredResponseState {
        version: AGENT_BRIDGE_VERSION,
        items: items
            .into_iter()
            .map(|(output_index, item)| StoredResponseItem { output_index, item })
            .collect(),
        omitted_message_indices: Vec::new(),
    };
    if serialized_state_size(&state)? <= AGENT_BRIDGE_MAX_ROUND_BYTES {
        return Ok(PreparedState {
            state,
            truncated: false,
        });
    }

    let mut kept = Vec::with_capacity(state.items.len());
    for item in state.items {
        if item.item.get("type").and_then(Value::as_str) == Some("message") {
            state.omitted_message_indices.push(item.output_index);
        } else {
            kept.push(item);
        }
    }
    state.items = kept;
    if serialized_state_size(&state)? > AGENT_BRIDGE_MAX_ROUND_BYTES {
        return Err(GatewayError::Internal(
            "Agent Bridge response state exceeds the 8 MiB replay limit".to_string(),
        ));
    }
    Ok(PreparedState {
        state,
        truncated: true,
    })
}

fn serialized_state_size(state: &StoredResponseState) -> Result<usize, GatewayError> {
    serde_json::to_vec(state)
        .map(|bytes| bytes.len())
        .map_err(|error| GatewayError::Internal(error.to_string()))
}

fn bridge_context_and_handle(
    report_context: Option<&Value>,
) -> Option<(AgentBridgeContext, String)> {
    let bridge = report_context?
        .get(AGENT_BRIDGE_REPORT_CONTEXT_FIELD)?
        .as_object()?;
    if bridge.get("finalized").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let context = serde_json::from_value(bridge.get("context")?.clone()).ok()?;
    let handle = bridge
        .get("state_handle")
        .or_else(|| bridge.get("response_handle"))?
        .as_str()
        .filter(|value| runtime_key_from_handle(value).is_some())?
        .to_string();
    Some((context, handle))
}

fn bridge_compatibility_report(report_context: Option<&Value>) -> AgentBridgeCompatibilityReport {
    report_context
        .and_then(|context| context.get(AGENT_BRIDGE_REPORT_CONTEXT_FIELD))
        .and_then(|bridge| bridge.get("compatibility"))
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn update_bridge_response_context(
    report_context: &mut Option<Value>,
    response_handle: Option<String>,
    reasoning_fallbacks: Vec<String>,
    report: AgentBridgeCompatibilityReport,
) {
    let Some(bridge) = report_context
        .as_mut()
        .and_then(Value::as_object_mut)
        .and_then(|context| context.get_mut(AGENT_BRIDGE_REPORT_CONTEXT_FIELD))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    match response_handle {
        Some(handle) => {
            bridge.insert("response_handle".to_string(), Value::String(handle));
        }
        None => {
            bridge.remove("response_handle");
        }
    }
    bridge.insert(
        "response_reasoning_fallbacks".to_string(),
        Value::Array(reasoning_fallbacks.into_iter().map(Value::String).collect()),
    );
    bridge.insert(
        "compatibility".to_string(),
        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
    );
    bridge.insert("finalized".to_string(), Value::Bool(true));
}

fn new_agent_bridge_handle() -> String {
    format!("{AGENT_BRIDGE_HANDLE_PREFIX}{}", Uuid::new_v4())
}

fn runtime_key_from_handle(handle: &str) -> Option<String> {
    let opaque = handle.strip_prefix(AGENT_BRIDGE_HANDLE_PREFIX)?.trim();
    let uuid = Uuid::parse_str(opaque).ok()?;
    Some(format!("{AGENT_BRIDGE_RUNTIME_KEY_PREFIX}{uuid}"))
}

fn seal_stored_state(
    state: &StoredResponseState,
    master_key: &str,
    context: &AgentBridgeContext,
) -> Result<String, OpenEnvelopeError> {
    let serialized = serde_json::to_vec(state).map_err(|_| OpenEnvelopeError::Invalid)?;
    if serialized.len() > AGENT_BRIDGE_MAX_ROUND_BYTES {
        return Err(OpenEnvelopeError::Oversized);
    }
    seal_bytes(&serialized, master_key, context).and_then(|envelope| {
        serde_json::to_string(&envelope).map_err(|_| OpenEnvelopeError::Invalid)
    })
}

fn open_stored_state(
    raw: &str,
    master_key: &str,
    context: &AgentBridgeContext,
) -> Result<RestoredState, OpenEnvelopeError> {
    let envelope = serde_json::from_str::<AgentBridgeSealedEnvelope>(raw)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let plaintext = open_bytes(&envelope, master_key, context, AGENT_BRIDGE_MAX_ROUND_BYTES)?;
    let serialized_bytes = plaintext.len();
    let state = serde_json::from_slice::<StoredResponseState>(&plaintext)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    if state.version != AGENT_BRIDGE_VERSION {
        return Err(OpenEnvelopeError::Invalid);
    }
    Ok(RestoredState {
        state,
        serialized_bytes,
    })
}

fn seal_reasoning_fallback(
    reasoning_item: &Value,
    master_key: &str,
    context: &AgentBridgeContext,
) -> Result<String, OpenEnvelopeError> {
    if reasoning_item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return Err(OpenEnvelopeError::Invalid);
    }
    let serialized = serde_json::to_vec(reasoning_item).map_err(|_| OpenEnvelopeError::Invalid)?;
    let envelope = seal_bytes(&serialized, master_key, context)?;
    let encoded = URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&envelope).map_err(|_| OpenEnvelopeError::Invalid)?);
    let carrier = format!("{AGENT_BRIDGE_REASONING_PREFIX}{encoded}");
    if carrier.len() > AGENT_BRIDGE_MAX_FALLBACK_BYTES {
        return Err(OpenEnvelopeError::Oversized);
    }
    Ok(carrier)
}

fn open_reasoning_fallback(
    carrier: &str,
    master_key: &str,
    context: &AgentBridgeContext,
) -> Result<Value, OpenEnvelopeError> {
    if carrier.len() > AGENT_BRIDGE_MAX_FALLBACK_BYTES {
        return Err(OpenEnvelopeError::Oversized);
    }
    let encoded = carrier
        .strip_prefix(AGENT_BRIDGE_REASONING_PREFIX)
        .ok_or(OpenEnvelopeError::Invalid)?;
    let envelope_bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let envelope = serde_json::from_slice::<AgentBridgeSealedEnvelope>(&envelope_bytes)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let plaintext = open_bytes(
        &envelope,
        master_key,
        context,
        AGENT_BRIDGE_MAX_FALLBACK_BYTES,
    )?;
    let item =
        serde_json::from_slice::<Value>(&plaintext).map_err(|_| OpenEnvelopeError::Invalid)?;
    if item.get("type").and_then(Value::as_str) != Some("reasoning") {
        return Err(OpenEnvelopeError::Invalid);
    }
    Ok(item)
}

fn seal_bytes(
    plaintext: &[u8],
    master_key: &str,
    context: &AgentBridgeContext,
) -> Result<AgentBridgeSealedEnvelope, OpenEnvelopeError> {
    let aad = context_aad(context)?;
    let key = derive_domain_key(master_key, AGENT_BRIDGE_KEY_DOMAIN)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| OpenEnvelopeError::Invalid)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let compressed = zstd::stream::encode_all(Cursor::new(plaintext), AGENT_BRIDGE_ZSTD_LEVEL)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &compressed,
                aad: &aad,
            },
        )
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    Ok(AgentBridgeSealedEnvelope {
        version: AGENT_BRIDGE_VERSION,
        scope_fingerprint: context_scope_fingerprint(master_key, &aad)?,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
    })
}

fn open_bytes(
    envelope: &AgentBridgeSealedEnvelope,
    master_key: &str,
    context: &AgentBridgeContext,
    max_plaintext_bytes: usize,
) -> Result<Vec<u8>, OpenEnvelopeError> {
    if envelope.version != AGENT_BRIDGE_VERSION {
        return Err(OpenEnvelopeError::Invalid);
    }
    let aad = context_aad(context)?;
    if envelope.scope_fingerprint != context_scope_fingerprint(master_key, &aad)? {
        return Err(OpenEnvelopeError::ScopeMismatch);
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(&envelope.nonce)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    if nonce.len() != 12 {
        return Err(OpenEnvelopeError::Invalid);
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&envelope.ciphertext)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    if ciphertext.len() > max_plaintext_bytes.saturating_add(1024 * 1024) {
        return Err(OpenEnvelopeError::Oversized);
    }
    let key = derive_domain_key(master_key, AGENT_BRIDGE_KEY_DOMAIN)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| OpenEnvelopeError::Invalid)?;
    let compressed = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed))
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    let mut limited = decoder.take(max_plaintext_bytes.saturating_add(1) as u64);
    let mut plaintext = Vec::new();
    limited
        .read_to_end(&mut plaintext)
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    if plaintext.len() > max_plaintext_bytes {
        return Err(OpenEnvelopeError::Oversized);
    }
    Ok(plaintext)
}

fn context_aad(context: &AgentBridgeContext) -> Result<Vec<u8>, OpenEnvelopeError> {
    serde_json::to_vec(&json!({
        "version": AGENT_BRIDGE_VERSION,
        "context": context,
    }))
    .map_err(|_| OpenEnvelopeError::Invalid)
}

fn derive_domain_key(master_key: &str, domain: &[u8]) -> Result<[u8; 32], OpenEnvelopeError> {
    if master_key.is_empty() {
        return Err(OpenEnvelopeError::Invalid);
    }
    let mut mac = <HmacSha256 as Mac>::new_from_slice(master_key.as_bytes())
        .map_err(|_| OpenEnvelopeError::Invalid)?;
    mac.update(domain);
    Ok(mac.finalize().into_bytes().into())
}

fn context_scope_fingerprint(master_key: &str, aad: &[u8]) -> Result<String, OpenEnvelopeError> {
    let scope_key = derive_domain_key(master_key, AGENT_BRIDGE_SCOPE_DOMAIN)?;
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(&scope_key).map_err(|_| OpenEnvelopeError::Invalid)?;
    mac.update(aad);
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

pub(crate) struct AgentBridgeStreamCapture {
    context: AgentBridgeContext,
    handle: String,
    encryption_key: Option<String>,
    buffered: Vec<u8>,
    next_sequence: Option<u64>,
    seen_sequences: HashSet<u64>,
    pending_sequences: BTreeMap<u64, Vec<u8>>,
    output_items: BTreeMap<usize, Value>,
    terminal_status: Option<String>,
    terminal_seen: bool,
    failed: bool,
    reasoning_fallbacks: Vec<String>,
    fallback_item_keys: HashSet<String>,
    optional_metadata_removed: bool,
}

impl AgentBridgeStreamCapture {
    pub(crate) fn from_report_context(
        state: &AppState,
        report_context: Option<&Value>,
    ) -> Option<Self> {
        let (context, handle) = bridge_context_and_handle(report_context)?;
        Some(Self {
            context,
            handle,
            encryption_key: state.encryption_key().map(ToOwned::to_owned),
            buffered: Vec::new(),
            next_sequence: None,
            seen_sequences: HashSet::new(),
            pending_sequences: BTreeMap::new(),
            output_items: BTreeMap::new(),
            terminal_status: None,
            terminal_seen: false,
            failed: false,
            reasoning_fallbacks: Vec::new(),
            fallback_item_keys: HashSet::new(),
            optional_metadata_removed: false,
        })
    }

    pub(crate) fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<u8>, GatewayError> {
        self.buffered.extend_from_slice(chunk);
        let mut output = Vec::new();
        while let Some(record) = take_next_sse_record(&mut self.buffered) {
            output.extend(self.push_record(record)?);
        }
        Ok(output)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<u8>, GatewayError> {
        let mut output = Vec::new();
        if self.buffered.iter().any(|byte| !byte.is_ascii_whitespace()) {
            let mut record = std::mem::take(&mut self.buffered);
            if !record.ends_with(b"\n\n") {
                record.extend_from_slice(b"\n\n");
            }
            output.extend(self.push_record(record)?);
        } else {
            self.buffered.clear();
        }
        if !self.pending_sequences.is_empty() {
            return Err(GatewayError::Internal(
                "Agent Bridge stream ended with an unresolved sequence gap".to_string(),
            ));
        }
        Ok(output)
    }

    fn push_record(&mut self, record: Vec<u8>) -> Result<Vec<u8>, GatewayError> {
        let Some(value) = decode_sse_record_value(&record) else {
            return Ok(record);
        };
        let Some(sequence) = value.get("sequence_number").and_then(Value::as_u64) else {
            return self.process_record(record, value);
        };
        if !self.seen_sequences.insert(sequence) {
            return Ok(Vec::new());
        }
        let expected = self.next_sequence.get_or_insert(sequence);
        if sequence < *expected {
            return Err(GatewayError::Internal(
                "Agent Bridge received a late out-of-order sequence".to_string(),
            ));
        }
        self.pending_sequences.insert(sequence, record);
        let mut output = Vec::new();
        loop {
            let expected = self.next_sequence.expect("sequence initialized");
            let Some(record) = self.pending_sequences.remove(&expected) else {
                break;
            };
            let value = decode_sse_record_value(&record).ok_or_else(|| {
                GatewayError::Internal("Agent Bridge lost a buffered SSE record".to_string())
            })?;
            output.extend(self.process_record(record, value)?);
            self.next_sequence = Some(expected.saturating_add(1));
        }
        Ok(output)
    }

    fn process_record(
        &mut self,
        record: Vec<u8>,
        mut value: Value,
    ) -> Result<Vec<u8>, GatewayError> {
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut fallback_after_record = None;
        if event_type == "response.output_item.done" {
            if let Some(item) = value.get("item").cloned() {
                validate_stream_function_call_item(&item)?;
                let output_index = value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .and_then(|index| usize::try_from(index).ok())
                    .unwrap_or(self.output_items.len());
                self.output_items.insert(output_index, item.clone());
                fallback_after_record = self.reasoning_fallback_for_item(&item);
            }
        }

        if matches!(
            event_type.as_str(),
            "response.completed" | "response.done" | "response.incomplete" | "response.failed"
        ) {
            self.terminal_seen = true;
            let response = value.get("response").cloned().unwrap_or(Value::Null);
            self.optional_metadata_removed |=
                sanitize_openai_responses_for_claude_projection(&response)
                    .1
                    .optional_metadata_removed
                    > 0;
            self.terminal_status = response
                .get("status")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    let status = if event_type == "response.failed" {
                        "failed"
                    } else if event_type == "response.incomplete" {
                        "incomplete"
                    } else {
                        "completed"
                    };
                    Some(status.to_owned())
                });
            self.failed |= event_type == "response.failed"
                || self
                    .terminal_status
                    .as_deref()
                    .is_some_and(|status| matches!(status, "failed" | "cancelled"));
            if let Some(output) = response.get("output").and_then(Value::as_array) {
                let mut terminal_fallbacks = Vec::new();
                for (output_index, item) in output.iter().enumerate() {
                    validate_stream_function_call_item(item)?;
                    if item.get("type").and_then(Value::as_str) == Some("function_call") {
                        match self.output_items.get(&output_index) {
                            Some(authoritative)
                                if authoritative.get("type").and_then(Value::as_str)
                                    == Some("function_call") => {}
                            Some(_) => {
                                return Err(GatewayError::Internal(format!(
                                    "Agent Bridge output index {output_index} changed item type before terminal state"
                                )));
                            }
                            None => {
                                return Err(GatewayError::Internal(format!(
                                    "Agent Bridge function call at output index {output_index} has no authoritative output_item.done event"
                                )));
                            }
                        }
                    } else if self
                        .output_items
                        .get(&output_index)
                        .is_some_and(|authoritative| {
                            authoritative.get("type").and_then(Value::as_str)
                                == Some("function_call")
                        })
                    {
                        return Err(GatewayError::Internal(format!(
                            "Agent Bridge output index {output_index} changed item type before terminal state"
                        )));
                    } else {
                        self.output_items.insert(output_index, item.clone());
                    }
                    if let Some(fallback) = self.reasoning_fallback_for_item(item) {
                        terminal_fallbacks.push(fallback);
                    }
                }
                if !terminal_fallbacks.is_empty() {
                    value
                        .as_object_mut()
                        .expect("stream event is an object")
                        .insert(
                            "aether_reasoning_fallbacks".to_string(),
                            Value::Array(
                                terminal_fallbacks.into_iter().map(Value::String).collect(),
                            ),
                        );
                }
            }
        } else if event_type == "error" {
            self.failed = true;
        }

        let mut output = if value.get("aether_reasoning_fallbacks").is_some() {
            encode_internal_sse_event(&event_type, &value)?
        } else {
            record
        };
        if let Some(fallback) = fallback_after_record {
            output.extend(encode_internal_sse_event(
                "response.aether_reasoning_fallback",
                &json!({
                    "type": "response.aether_reasoning_fallback",
                    "signature": fallback,
                }),
            )?);
        }
        Ok(output)
    }

    fn reasoning_fallback_for_item(&mut self, item: &Value) -> Option<String> {
        if item.get("type").and_then(Value::as_str) != Some("reasoning") {
            return None;
        }
        let item_key = item
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| serde_json::to_string(item).unwrap_or_default());
        if !self.fallback_item_keys.insert(item_key) {
            return None;
        }
        let fallback =
            seal_reasoning_fallback(item, self.encryption_key.as_deref()?, &self.context).ok()?;
        self.reasoning_fallbacks.push(fallback.clone());
        Some(fallback)
    }

    pub(crate) async fn persist_if_complete(
        &self,
        state: &AppState,
        report_context: &mut Option<Value>,
        stream_failed: bool,
    ) {
        let mut report = bridge_compatibility_report(report_context.as_ref());
        if self.optional_metadata_removed {
            report.accept_loss("optional-metadata");
        }
        if stream_failed
            || self.failed
            || !self.terminal_seen
            || !matches!(
                self.terminal_status.as_deref(),
                Some("completed" | "incomplete")
            )
        {
            report.mark_state_miss("stream-terminal-not-replayable");
            update_bridge_response_context(
                report_context,
                Some(self.handle.clone()),
                self.reasoning_fallbacks.clone(),
                report,
            );
            return;
        }
        let Some(encryption_key) = self.encryption_key.as_deref().filter(|key| !key.is_empty())
        else {
            if self.reasoning_fallbacks.is_empty() {
                report.mark_state_miss("encryption-key-missing");
            } else {
                report.mark_reasoning_fallback();
            }
            update_bridge_response_context(
                report_context,
                Some(self.handle.clone()),
                self.reasoning_fallbacks.clone(),
                report,
            );
            return;
        };
        let prepared = match prepare_state_from_items(
            self.output_items
                .iter()
                .map(|(index, item)| (*index, item.clone()))
                .collect(),
        ) {
            Ok(prepared) => prepared,
            Err(_) => {
                report.state_truncated = true;
                report.mark_state_miss("state-size-limit");
                update_bridge_response_context(
                    report_context,
                    Some(self.handle.clone()),
                    self.reasoning_fallbacks.clone(),
                    report,
                );
                return;
            }
        };
        report.state_truncated |= prepared.truncated;
        let saved = seal_stored_state(&prepared.state, encryption_key, &self.context)
            .ok()
            .and_then(|raw| runtime_key_from_handle(&self.handle).map(|key| (key, raw)));
        let saved = if let Some((runtime_key, raw)) = saved {
            state
                .runtime_state()
                .kv_set(&runtime_key, raw, Some(AGENT_BRIDGE_STATE_TTL))
                .await
                .is_ok()
        } else {
            false
        };
        if saved {
            report.saved_item_count = prepared.state.items.len();
        } else if self.reasoning_fallbacks.is_empty() {
            report.mark_state_miss("state-not-saved");
        } else {
            report.mark_reasoning_fallback();
        }
        update_bridge_response_context(
            report_context,
            Some(self.handle.clone()),
            self.reasoning_fallbacks.clone(),
            report,
        );
    }
}

fn validate_stream_function_call_item(item: &Value) -> Result<(), GatewayError> {
    validate_agent_bridge_function_call_arguments(&json!({"output": [item.clone()]})).map_err(
        |message| {
            GatewayError::Internal(format!(
                "Agent Bridge rejected malformed streamed tool call: {message}"
            ))
        },
    )
}

fn take_next_sse_record(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf = buffer.windows(4).position(|window| window == b"\r\n\r\n");
    let (position, delimiter_len) = match (lf, crlf) {
        (Some(lf), Some(crlf)) if lf <= crlf => (lf, 2),
        (Some(_), Some(crlf)) => (crlf, 4),
        (Some(lf), None) => (lf, 2),
        (None, Some(crlf)) => (crlf, 4),
        (None, None) => return None,
    };
    Some(buffer.drain(..position + delimiter_len).collect())
}

fn decode_sse_record_value(record: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(record).ok()?;
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    serde_json::from_str(&data).ok()
}

fn encode_internal_sse_event(event_type: &str, value: &Value) -> Result<Vec<u8>, GatewayError> {
    let payload =
        serde_json::to_string(value).map_err(|error| GatewayError::Internal(error.to_string()))?;
    Ok(format!("event: {event_type}\ndata: {payload}\n\n").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_serving::{
        convert_request, sanitize_claude_request_for_agent_bridge_conversion, FormatContext,
    };
    use crate::data::GatewayDataState;
    use aether_runtime_state::MemoryRuntimeStateConfig;

    fn context() -> AgentBridgeContext {
        AgentBridgeContext {
            session_scope: "claude-session-1".to_string(),
            user_id: "user-1".to_string(),
            api_key_id: "api-key-1".to_string(),
            provider_id: "provider-1".to_string(),
            endpoint_id: "endpoint-1".to_string(),
            provider_key_id: "provider-key-1".to_string(),
            target_format: "openai:responses".to_string(),
            mapped_model: "gpt-5.3-codex".to_string(),
        }
    }

    fn sample_state() -> StoredResponseState {
        StoredResponseState {
            version: AGENT_BRIDGE_VERSION,
            items: vec![StoredResponseItem {
                output_index: 0,
                item: json!({
                    "type": "reasoning",
                    "id": "rs_1",
                    "encrypted_content": "sealed",
                    "summary": []
                }),
            }],
            omitted_message_indices: Vec::new(),
        }
    }

    #[test]
    fn agent_bridge_state_roundtrips_with_aes_gcm_and_zstd() {
        let sealed = seal_stored_state(&sample_state(), "master-key", &context()).unwrap();
        assert!(!sealed.contains("sealed"));
        let restored = open_stored_state(&sealed, "master-key", &context()).unwrap();
        assert_eq!(restored.state.items[0].item["id"], "rs_1");
    }

    #[test]
    fn agent_bridge_state_rejects_tampering_and_cross_scope_replay() {
        let sealed = seal_stored_state(&sample_state(), "master-key", &context()).unwrap();
        let mut envelope: AgentBridgeSealedEnvelope = serde_json::from_str(&sealed).unwrap();
        envelope.ciphertext.push('x');
        let tampered = serde_json::to_string(&envelope).unwrap();
        assert_eq!(
            open_stored_state(&tampered, "master-key", &context()).unwrap_err(),
            OpenEnvelopeError::Invalid
        );

        let mut other = context();
        other.provider_key_id = "provider-key-2".to_string();
        assert_eq!(
            open_stored_state(&sealed, "master-key", &other).unwrap_err(),
            OpenEnvelopeError::ScopeMismatch
        );
        other = context();
        other.user_id = "user-2".to_string();
        assert_eq!(
            open_stored_state(&sealed, "master-key", &other).unwrap_err(),
            OpenEnvelopeError::ScopeMismatch
        );
    }

    #[test]
    fn agent_bridge_reasoning_fallback_is_self_contained_and_scoped() {
        let item = sample_state().items.remove(0).item;
        let carrier = seal_reasoning_fallback(&item, "master-key", &context()).unwrap();
        assert!(carrier.starts_with(AGENT_BRIDGE_REASONING_PREFIX));
        assert_eq!(
            open_reasoning_fallback(&carrier, "master-key", &context()).unwrap(),
            item
        );
    }

    #[tokio::test]
    async fn agent_bridge_runtime_state_uses_sliding_ttl_and_exact_scope() {
        let runtime = RuntimeState::memory(MemoryRuntimeStateConfig::default());
        let handle = new_agent_bridge_handle();
        let runtime_key = runtime_key_from_handle(&handle).unwrap();
        let raw = seal_stored_state(&sample_state(), "master-key", &context()).unwrap();
        runtime
            .kv_set(&runtime_key, raw.clone(), Some(Duration::from_secs(1)))
            .await
            .unwrap();
        let restored = open_stored_state(
            runtime
                .kv_get(&runtime_key)
                .await
                .unwrap()
                .as_deref()
                .unwrap(),
            "master-key",
            &context(),
        )
        .unwrap();
        assert_eq!(restored.state.items.len(), 1);
        runtime
            .kv_set(&runtime_key, raw, Some(AGENT_BRIDGE_STATE_TTL))
            .await
            .unwrap();
        assert!(runtime.kv_ttl_seconds(&runtime_key).await.unwrap().unwrap() > 60);
    }

    #[test]
    fn agent_bridge_prompt_cache_identity_is_stable_and_scope_sensitive() {
        let first = agent_bridge_prompt_cache_identity(&serde_json::to_string(&context()).unwrap());
        let second =
            agent_bridge_prompt_cache_identity(&serde_json::to_string(&context()).unwrap());
        assert_eq!(first, second);
        let mut other = context();
        other.session_scope = "claude-session-2".to_string();
        assert_ne!(
            first,
            agent_bridge_prompt_cache_identity(&serde_json::to_string(&other).unwrap())
        );
    }

    #[test]
    fn agent_bridge_explicit_prompt_cache_requires_upstream_capability() {
        assert!(!agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.6-terra",
            "custom",
            "https://third-party.example/v1",
            None,
        ));
        assert!(agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.6-terra",
            "custom",
            "https://third-party.example/v1",
            Some(&json!({"openai_prompt_cache_breakpoints": true})),
        ));
        assert!(!agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.5",
            "custom",
            "https://third-party.example/v1",
            Some(&json!({"openai_prompt_cache_breakpoints": true})),
        ));
        assert!(agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.6-sol",
            "openai",
            "https://api.openai.com/v1",
            None,
        ));
        assert!(!agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.6-sol",
            "openai",
            "https://openai-compatible.example/v1",
            None,
        ));
        assert!(!agent_bridge_supports_explicit_prompt_cache(
            "gpt-5.6-sol",
            "openai",
            "https://api.openai.com/v1",
            Some(&json!({"prompt_cache_breakpoints": false})),
        ));
    }

    #[test]
    fn agent_bridge_materializes_truncated_messages_without_partial_tools() {
        let state = StoredResponseState {
            version: AGENT_BRIDGE_VERSION,
            items: vec![StoredResponseItem {
                output_index: 1,
                item: json!({
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "Read",
                    "arguments": "{\"path\":\"a.rs\"}"
                }),
            }],
            omitted_message_indices: vec![0],
        };
        let visible = vec![
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"checking"}]}),
            json!({"type":"function_call","call_id":"call_1","name":"Read","arguments":"{\"path\":\"a.rs\"}"}),
        ];
        let replay = materialize_replay_items(&state, &visible);
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0]["type"], "message");
        assert_eq!(replay[1]["call_id"], "call_1");
    }

    #[tokio::test]
    async fn agent_bridge_sync_oversized_critical_state_degrades_without_partial_save() {
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let handle = new_agent_bridge_handle();
        let runtime_key = runtime_key_from_handle(&handle).unwrap();
        let mut report_context = Some(json!({
            "agent_bridge": {
                "version": AGENT_BRIDGE_VERSION,
                "context": context(),
                "response_handle": handle,
                "compatibility": AgentBridgeCompatibilityReport::default(),
            }
        }));
        let arguments = serde_json::to_string(&json!({
            "payload": "x".repeat(AGENT_BRIDGE_MAX_ROUND_BYTES)
        }))
        .unwrap();
        let response = json!({
            "id": "resp_oversized",
            "model": "gpt-5.3-codex",
            "status": "completed",
            "metadata": {"optional": true},
            "output": [{
                "type": "function_call",
                "id": "fc_oversized",
                "call_id": "call_oversized",
                "name": "Write",
                "arguments": arguments,
            }]
        });

        let projected = finalize_agent_bridge_sync_response(&state, &mut report_context, response)
            .await
            .expect("oversized replay state must degrade instead of failing the response");
        assert_eq!(projected["output"][0]["call_id"], "call_oversized");
        assert!(state
            .runtime_state()
            .kv_get(&runtime_key)
            .await
            .unwrap()
            .is_none());
        let bridge = &report_context.unwrap()["agent_bridge"];
        assert!(bridge.get("response_handle").is_none());
        assert_eq!(bridge["compatibility"]["primary_state"], "state-miss");
        assert!(bridge["compatibility"]["state_truncated"]
            .as_bool()
            .unwrap());
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("state-size-limit")));
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("optional-metadata")));
    }

    #[test]
    fn agent_bridge_primary_state_reports_reasoning_fallback_and_state_miss() {
        let mut report = AgentBridgeCompatibilityReport::default();
        report.mark_reasoning_fallback();
        assert_eq!(
            report.primary_state,
            AgentBridgePrimaryState::ReasoningFallback
        );
        report.mark_state_miss("state-expired");
        assert_eq!(report.primary_state, AgentBridgePrimaryState::StateMiss);
    }

    #[tokio::test]
    async fn agent_bridge_activation_matrix_honors_client_format_family_and_off_switch() {
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("session-1".to_string()),
        );
        let input = AgentBridgeRequestInput {
            client_api_format: "claude:messages",
            provider_api_format: "openai:responses",
            mapped_model: "gpt-5.3-codex",
            user_id: "user-1",
            api_key_id: "api-key-1",
            provider_id: "provider-1",
            endpoint_id: "endpoint-1",
            provider_key_id: "provider-key-1",
            client_session_affinity: Some(&affinity),
        };
        let state = AppState::new().unwrap();
        assert!(agent_bridge_request_is_eligible(&state, input).await);

        let disabled = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_system_config_values_for_tests([(
                "agent_format_bridge_mode".to_string(),
                json!("off"),
            )]),
        );
        assert!(!agent_bridge_request_is_eligible(&disabled, input).await);

        let mut wrong_format = input;
        wrong_format.provider_api_format = "openai:chat";
        assert!(!agent_bridge_request_is_eligible(&state, wrong_format).await);
        let other_affinity =
            ClientSessionAffinity::new(Some("codex".to_string()), Some("session-1".to_string()));
        let mut wrong_family = input;
        wrong_family.client_session_affinity = Some(&other_affinity);
        assert!(!agent_bridge_request_is_eligible(&state, wrong_family).await);
    }

    #[tokio::test]
    async fn agent_bridge_missing_key_keeps_projection_stateless() {
        let state = AppState::new()
            .unwrap()
            .with_data_state_for_tests(GatewayDataState::disabled());
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("claude-session-1".to_string()),
        );
        let claude_body = json!({
            "model":"claude-sonnet",
            "max_tokens":1024,
            "messages":[{"role":"user","content":"hello"}]
        });
        let mut provider_body = json!({"model":"gpt-5.3-codex","input":[]});
        let bridge = prepare_agent_bridge_request(
            &state,
            &claude_body,
            &mut provider_body,
            AgentBridgeRequestInput {
                client_api_format: "claude:messages",
                provider_api_format: "openai:responses",
                mapped_model: "gpt-5.3-codex",
                user_id: "user-1",
                api_key_id: "api-key-1",
                provider_id: "provider-1",
                endpoint_id: "endpoint-1",
                provider_key_id: "provider-key-1",
                client_session_affinity: Some(&affinity),
            },
            AgentBridgeRequestSanitizeReport::default(),
        )
        .await
        .unwrap();
        assert_eq!(bridge["enabled"], true);
        assert!(bridge["state_handle"]
            .as_str()
            .unwrap()
            .starts_with(AGENT_BRIDGE_HANDLE_PREFIX));
        assert!(bridge["response_handle"].is_null());

        let mut report_context = Some(json!({"agent_bridge": bridge}));
        let projected = finalize_agent_bridge_sync_response(
            &state,
            &mut report_context,
            json!({
                "id":"resp_stateless",
                "model":"gpt-5.3-codex",
                "status":"completed",
                "output":[{
                    "type":"message",
                    "role":"assistant",
                    "phase":"final_answer",
                    "content":[{"type":"output_text","text":"done"}]
                }]
            }),
        )
        .await
        .unwrap();
        assert!(projected["output"][0].get("phase").is_none());
        let bridge = &report_context.unwrap()["agent_bridge"];
        assert!(bridge.get("response_handle").is_none());
        assert_eq!(bridge["compatibility"]["primary_state"], "state-miss");
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("encryption-key-missing")));
    }

    #[tokio::test]
    async fn agent_bridge_routes_claude_code_webfetch_through_hosted_web_search() {
        let state = AppState::new()
            .unwrap()
            .with_data_state_for_tests(GatewayDataState::disabled());
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("claude-session-web".to_string()),
        );
        let claude_body = json!({
            "model": "claude-sonnet",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": "inspect https://example.com"}],
            "tools": [{
                "name": "WebFetch",
                "description": "Fetch a URL and answer a question about its content",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "format": "uri"},
                        "prompt": {"type": "string"}
                    },
                    "required": ["url", "prompt"],
                    "additionalProperties": false
                }
            }],
            "tool_choice": {"type": "tool", "name": "WebFetch"}
        });
        let (conversion_body, sanitize_report) =
            sanitize_claude_request_for_agent_bridge_conversion(&claude_body);
        let mut provider_body = convert_request(
            "claude:messages",
            "openai:responses",
            &conversion_body,
            &FormatContext::default().with_mapped_model("gpt-5.4"),
        )
        .unwrap();
        assert_eq!(provider_body["tools"][0]["name"], "WebFetch");

        let bridge = prepare_agent_bridge_request(
            &state,
            &claude_body,
            &mut provider_body,
            AgentBridgeRequestInput {
                client_api_format: "claude:messages",
                provider_api_format: "openai:responses",
                mapped_model: "gpt-5.4",
                user_id: "user-1",
                api_key_id: "api-key-1",
                provider_id: "provider-1",
                endpoint_id: "endpoint-1",
                provider_key_id: "provider-key-1",
                client_session_affinity: Some(&affinity),
            },
            sanitize_report,
        )
        .await
        .expect("bridge should activate");

        assert_eq!(provider_body["tools"], json!([{"type": "web_search"}]));
        assert_eq!(provider_body["tool_choice"], json!({"type": "web_search"}));
        assert!(!provider_body.to_string().contains("\"format\":\"uri\""));
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("claude-code-web-tool-contract")));
    }

    #[tokio::test]
    async fn agent_bridge_multiturn_restores_exact_responses_items_and_tool_result() {
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("claude-session-1".to_string()),
        );
        let handle = new_agent_bridge_handle();
        let runtime_key = runtime_key_from_handle(&handle).unwrap();
        let stored = StoredResponseState {
            version: AGENT_BRIDGE_VERSION,
            items: vec![
                StoredResponseItem {
                    output_index: 0,
                    item: json!({
                        "type":"reasoning",
                        "id":"rs_1",
                        "status":"completed",
                        "encrypted_content":"encrypted-reasoning",
                        "summary":[{"type":"summary_text","text":"inspect"}]
                    }),
                },
                StoredResponseItem {
                    output_index: 1,
                    item: json!({
                        "type":"message",
                        "id":"msg_1",
                        "role":"assistant",
                        "status":"completed",
                        "phase":"commentary",
                        "content":[{"type":"output_text","text":"checking"}]
                    }),
                },
                StoredResponseItem {
                    output_index: 2,
                    item: json!({
                        "type":"function_call",
                        "id":"fc_1",
                        "call_id":"call_1",
                        "name":"Read",
                        "arguments":"{\"path\":\"a.rs\"}"
                    }),
                },
            ],
            omitted_message_indices: Vec::new(),
        };
        let raw = seal_stored_state(&stored, "master-key", &context()).unwrap();
        state
            .runtime_state()
            .kv_set(&runtime_key, raw, Some(Duration::from_secs(1)))
            .await
            .unwrap();
        let claude_body = json!({
            "model":"claude-sonnet",
            "max_tokens":1024,
            "system":[{"type":"text","text":"be exact","cache_control":{"type":"ephemeral"}}],
            "messages":[
                {"role":"user","content":"inspect"},
                {"role":"assistant","content":[
                    {"type":"redacted_thinking","data":handle},
                    {"type":"thinking","thinking":"inspect first"},
                    {"type":"text","text":"checking","cache_control":{"type":"ephemeral"}},
                    {"type":"tool_use","id":"call_1","name":"Read","input":{"path":"a.rs"}}
                ]},
                {"role":"user","content":[
                    {"type":"tool_result","tool_use_id":"call_1","content":[
                        {"type":"tool_reference","tool_name":"Read"},
                        {"type":"tool_reference","tool_name":"TaskList"}
                    ]}
                ]}
            ]
        });
        let (conversion_body, sanitize_report) =
            sanitize_claude_request_for_agent_bridge_conversion(&claude_body);
        let mut provider_body = convert_request(
            "claude:messages",
            "openai:responses",
            &conversion_body,
            &FormatContext::default().with_mapped_model("gpt-5.3-codex"),
        )
        .unwrap();
        let bridge = prepare_agent_bridge_request(
            &state,
            &claude_body,
            &mut provider_body,
            AgentBridgeRequestInput {
                client_api_format: "claude:messages",
                provider_api_format: "openai:responses",
                mapped_model: "gpt-5.3-codex",
                user_id: "user-1",
                api_key_id: "api-key-1",
                provider_id: "provider-1",
                endpoint_id: "endpoint-1",
                provider_key_id: "provider-key-1",
                client_session_affinity: Some(&affinity),
            },
            sanitize_report,
        )
        .await
        .expect("bridge should activate");
        assert!(
            state
                .runtime_state()
                .kv_ttl_seconds(&runtime_key)
                .await
                .unwrap()
                .unwrap()
                > 60
        );
        assert_eq!(bridge["compatibility"]["restored_item_count"], 3);
        assert_eq!(bridge["compatibility"]["primary_state"], "full-state");
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("cache-breakpoint")));
        assert!(bridge["compatibility"]["accepted_loss_types"]
            .as_array()
            .unwrap()
            .contains(&json!("tool-reference-structure")));
        let input = provider_body["input"].as_array().unwrap();
        assert_eq!(
            input
                .iter()
                .filter(|item| item.get("id") == Some(&json!("rs_1")))
                .count(),
            1
        );
        assert_eq!(
            input
                .iter()
                .find(|item| item.get("id") == Some(&json!("msg_1")))
                .unwrap()["phase"],
            "commentary"
        );
        assert_eq!(
            input
                .iter()
                .find(|item| item.get("call_id") == Some(&json!("call_1"))
                    && item.get("type") == Some(&json!("function_call")))
                .unwrap()["arguments"],
            "{\"path\":\"a.rs\"}"
        );
        assert!(input.iter().any(|item| {
            item.get("type") == Some(&json!("function_call_output"))
                && item.get("call_id") == Some(&json!("call_1"))
                && item
                    .get("output")
                    .and_then(Value::as_str)
                    .is_some_and(|output| {
                        output.contains("\"tool_name\":\"Read\"")
                            && output.contains("\"tool_name\":\"TaskList\"")
                    })
        }));
        assert_eq!(provider_body["store"], false);
        assert_eq!(
            provider_body["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    #[tokio::test]
    async fn agent_bridge_restores_sealed_reasoning_when_runtime_state_is_missing() {
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("claude-session-1".to_string()),
        );
        let fallback_item = json!({
            "type":"reasoning",
            "id":"rs_fallback",
            "encrypted_content":"encrypted-fallback",
            "summary":[{"type":"summary_text","text":"inspect"}]
        });
        let fallback = seal_reasoning_fallback(&fallback_item, "master-key", &context()).unwrap();
        let claude_body = json!({
            "model":"claude-sonnet",
            "max_tokens":1024,
            "messages":[{
                "role":"assistant",
                "content":[
                    {"type":"redacted_thinking","data":new_agent_bridge_handle()},
                    {"type":"redacted_thinking","data":fallback},
                    {"type":"text","text":"checking"},
                    {"type":"tool_use","id":"call_1","name":"Read","input":{"path":"a.rs"}}
                ]
            }]
        });
        let (conversion_body, sanitize_report) =
            sanitize_claude_request_for_agent_bridge_conversion(&claude_body);
        let mut provider_body = convert_request(
            "claude:messages",
            "openai:responses",
            &conversion_body,
            &FormatContext::default().with_mapped_model("gpt-5.3-codex"),
        )
        .unwrap();
        let bridge = prepare_agent_bridge_request(
            &state,
            &claude_body,
            &mut provider_body,
            AgentBridgeRequestInput {
                client_api_format: "claude:messages",
                provider_api_format: "openai:responses",
                mapped_model: "gpt-5.3-codex",
                user_id: "user-1",
                api_key_id: "api-key-1",
                provider_id: "provider-1",
                endpoint_id: "endpoint-1",
                provider_key_id: "provider-key-1",
                client_session_affinity: Some(&affinity),
            },
            sanitize_report,
        )
        .await
        .unwrap();
        assert_eq!(
            bridge["compatibility"]["primary_state"],
            "reasoning-fallback"
        );
        assert_eq!(bridge["compatibility"]["phase_inferred"], true);
        let input = provider_body["input"].as_array().unwrap();
        assert!(input
            .iter()
            .any(|item| item.get("id") == Some(&json!("rs_fallback"))));
        assert!(input.iter().any(|item| {
            item.get("type") == Some(&json!("message"))
                && item.get("phase") == Some(&json!("commentary"))
        }));
    }

    #[tokio::test]
    async fn agent_bridge_state_miss_uses_visible_history_without_fake_reasoning() {
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let affinity = ClientSessionAffinity::new(
            Some("claude_code".to_string()),
            Some("claude-session-1".to_string()),
        );
        let claude_body = json!({
            "model":"claude-sonnet",
            "max_tokens":1024,
            "messages":[{
                "role":"assistant",
                "content":[
                    {"type":"redacted_thinking","data":new_agent_bridge_handle()},
                    {"type":"text","text":"done"}
                ]
            }]
        });
        let (conversion_body, sanitize_report) =
            sanitize_claude_request_for_agent_bridge_conversion(&claude_body);
        let mut provider_body = convert_request(
            "claude:messages",
            "openai:responses",
            &conversion_body,
            &FormatContext::default().with_mapped_model("gpt-5.3-codex"),
        )
        .unwrap();
        let bridge = prepare_agent_bridge_request(
            &state,
            &claude_body,
            &mut provider_body,
            AgentBridgeRequestInput {
                client_api_format: "claude:messages",
                provider_api_format: "openai:responses",
                mapped_model: "gpt-5.3-codex",
                user_id: "user-1",
                api_key_id: "api-key-1",
                provider_id: "provider-1",
                endpoint_id: "endpoint-1",
                provider_key_id: "provider-key-1",
                client_session_affinity: Some(&affinity),
            },
            sanitize_report,
        )
        .await
        .unwrap();
        assert_eq!(bridge["compatibility"]["primary_state"], "state-miss");
        assert_eq!(bridge["compatibility"]["phase_inferred"], true);
        let input = provider_body["input"].as_array().unwrap();
        assert!(!input
            .iter()
            .any(|item| item.get("type") == Some(&json!("reasoning"))));
        assert!(input.iter().any(|item| {
            item.get("type") == Some(&json!("message"))
                && item.get("phase") == Some(&json!("final_answer"))
        }));
    }

    #[test]
    fn agent_bridge_stream_capture_reorders_gaps_and_deduplicates_sequences() {
        let mut capture = AgentBridgeStreamCapture {
            context: context(),
            handle: new_agent_bridge_handle(),
            encryption_key: Some("master-key".to_string()),
            buffered: Vec::new(),
            next_sequence: None,
            seen_sequences: HashSet::new(),
            pending_sequences: BTreeMap::new(),
            output_items: BTreeMap::new(),
            terminal_status: None,
            terminal_seen: false,
            failed: false,
            reasoning_fallbacks: Vec::new(),
            fallback_item_keys: HashSet::new(),
            optional_metadata_removed: false,
        };
        let chunk = [
            encode_internal_sse_event(
                "keepalive",
                &json!({"type":"keepalive","sequence_number":1}),
            )
            .unwrap(),
            encode_internal_sse_event(
                "keepalive",
                &json!({"type":"keepalive","sequence_number":3}),
            )
            .unwrap(),
            encode_internal_sse_event(
                "keepalive",
                &json!({"type":"keepalive","sequence_number":2}),
            )
            .unwrap(),
            encode_internal_sse_event(
                "keepalive",
                &json!({"type":"keepalive","sequence_number":2}),
            )
            .unwrap(),
        ]
        .concat();
        let output = String::from_utf8(capture.push_chunk(&chunk).unwrap()).unwrap();
        assert!(
            output.find("\"sequence_number\":1").unwrap()
                < output.find("\"sequence_number\":2").unwrap()
        );
        assert!(
            output.find("\"sequence_number\":2").unwrap()
                < output.find("\"sequence_number\":3").unwrap()
        );
        assert_eq!(output.matches("\"sequence_number\":2").count(), 1);
        assert!(capture.finish().unwrap().is_empty());
    }

    #[test]
    fn agent_bridge_stream_capture_seals_reasoning_and_rejects_malformed_tools() {
        let report_context = json!({
            "agent_bridge": {
                "context": context(),
                "response_handle": new_agent_bridge_handle(),
            }
        });
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let mut capture =
            AgentBridgeStreamCapture::from_report_context(&state, Some(&report_context)).unwrap();
        let reasoning = encode_internal_sse_event(
            "response.output_item.done",
            &json!({
                "type":"response.output_item.done",
                "output_index":0,
                "item":{"type":"reasoning","id":"rs_1","encrypted_content":"sealed","summary":[]}
            }),
        )
        .unwrap();
        let output = String::from_utf8(capture.push_chunk(&reasoning).unwrap()).unwrap();
        assert!(output.contains("response.aether_reasoning_fallback"));
        assert!(output.contains(AGENT_BRIDGE_REASONING_PREFIX));

        let malformed = encode_internal_sse_event(
            "response.output_item.done",
            &json!({
                "type":"response.output_item.done",
                "output_index":1,
                "item":{"type":"function_call","call_id":"call_1","name":"Read","arguments":""}
            }),
        )
        .unwrap();
        assert!(capture.push_chunk(&malformed).is_err());
    }

    #[test]
    fn agent_bridge_stream_capture_keeps_done_arguments_authoritative() {
        let report_context = json!({
            "agent_bridge": {
                "context": context(),
                "response_handle": new_agent_bridge_handle(),
            }
        });
        let state = AppState::new().unwrap().with_data_state_for_tests(
            GatewayDataState::disabled().with_encryption_key_for_tests("master-key"),
        );
        let mut capture =
            AgentBridgeStreamCapture::from_report_context(&state, Some(&report_context)).unwrap();
        let done = encode_internal_sse_event(
            "response.output_item.done",
            &json!({
                "type":"response.output_item.done",
                "output_index":0,
                "item":{"type":"function_call","call_id":"call_1","name":"Read","arguments":"{\"path\":\"a.rs\"}"}
            }),
        )
        .unwrap();
        capture.push_chunk(&done).unwrap();
        let terminal = encode_internal_sse_event(
            "response.completed",
            &json!({
                "type":"response.completed",
                "response":{
                    "status":"completed",
                    "output":[{"type":"function_call","call_id":"call_1","name":"Read","arguments":"{\"path\":\"changed.rs\"}"}]
                }
            }),
        )
        .unwrap();
        capture.push_chunk(&terminal).unwrap();
        assert_eq!(capture.output_items[&0]["arguments"], "{\"path\":\"a.rs\"}");

        let mut missing_done =
            AgentBridgeStreamCapture::from_report_context(&state, Some(&report_context)).unwrap();
        assert!(missing_done.push_chunk(&terminal).is_err());
    }
}
