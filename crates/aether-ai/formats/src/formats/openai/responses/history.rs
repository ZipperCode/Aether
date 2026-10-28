use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::formats::{context::FormatContext, registry};

const HISTORY_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_HISTORY_ENTRIES: usize = 2_048;
const MAX_HISTORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_HISTORY_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const HISTORY_STORAGE_VERSION: u8 = 1;
const HISTORY_STORAGE_KEY_PREFIX: &str = "ai:responses:history:v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseHistoryRecord {
    pub storage_key: String,
    pub payload: String,
    pub ttl: Duration,
}

#[derive(Serialize, Deserialize)]
struct PersistedResponseHistory {
    version: u8,
    response_id: String,
    scope_fingerprint: String,
    expires_at_unix_secs: u64,
    transcript: Vec<Value>,
}

#[derive(Clone)]
struct ResponseHistoryEntry {
    transcript: Vec<Value>,
    inserted_at: Instant,
    expires_at: Instant,
    size_bytes: usize,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ResponseHistoryKey {
    scope: Option<String>,
    response_id: String,
}

#[derive(Default)]
struct ResponseHistoryStore {
    entries: HashMap<ResponseHistoryKey, ResponseHistoryEntry>,
    insertion_order: VecDeque<(ResponseHistoryKey, Instant)>,
    total_bytes: usize,
}

impl ResponseHistoryStore {
    fn remove(&mut self, key: &ResponseHistoryKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.size_bytes);
        }
    }

    fn prune(&mut self, now: Instant) {
        let expired_keys = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.expires_at <= now)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired_keys {
            self.remove(&key);
        }

        while self.entries.len() > MAX_HISTORY_ENTRIES || self.total_bytes > MAX_HISTORY_BYTES {
            let Some((key, inserted_at)) = self.insertion_order.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.inserted_at == inserted_at)
            {
                self.remove(&key);
            }
        }

        while let Some((key, inserted_at)) = self.insertion_order.front() {
            if self
                .entries
                .get(key)
                .is_some_and(|entry| entry.inserted_at == *inserted_at)
            {
                break;
            }
            self.insertion_order.pop_front();
        }
    }

    fn get(
        &mut self,
        response_id: &str,
        history_scope: Option<&str>,
        now: Instant,
    ) -> Option<Vec<Value>> {
        self.prune(now);
        self.entries
            .get(&response_history_key(response_id, history_scope))
            .map(|entry| entry.transcript.clone())
    }

    fn insert(
        &mut self,
        response_id: String,
        history_scope: Option<&str>,
        transcript: Vec<Value>,
        now: Instant,
        ttl: Duration,
    ) {
        if ttl.is_zero() {
            return;
        }
        let size_bytes = serde_json::to_vec(&transcript)
            .map(|bytes| bytes.len())
            .unwrap_or(MAX_HISTORY_ENTRY_BYTES.saturating_add(1));
        if size_bytes > MAX_HISTORY_ENTRY_BYTES {
            return;
        }
        let key = response_history_key(&response_id, history_scope);
        self.remove(&key);
        self.total_bytes = self.total_bytes.saturating_add(size_bytes);
        self.entries.insert(
            key.clone(),
            ResponseHistoryEntry {
                transcript,
                inserted_at: now,
                expires_at: now.checked_add(ttl).unwrap_or(now),
                size_bytes,
            },
        );
        self.insertion_order.push_back((key, now));
        self.prune(now);
    }
}

fn response_history_key(response_id: &str, history_scope: Option<&str>) -> ResponseHistoryKey {
    ResponseHistoryKey {
        scope: history_scope
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned),
        response_id: response_id.to_string(),
    }
}

fn normalized_history_scope(history_scope: Option<&str>) -> Option<&str> {
    history_scope
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
}

fn sha256_hex(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn history_scope_fingerprint(history_scope: Option<&str>) -> String {
    sha256_hex(
        normalized_history_scope(history_scope)
            .unwrap_or("<unscoped>")
            .as_bytes(),
    )
}

pub fn response_history_storage_key(response_id: &str, history_scope: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(history_scope_fingerprint(history_scope));
    hasher.update([0]);
    hasher.update(response_id.trim().as_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{HISTORY_STORAGE_KEY_PREFIX}:{digest}")
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn response_history_store() -> &'static Mutex<ResponseHistoryStore> {
    static STORE: OnceLock<Mutex<ResponseHistoryStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(ResponseHistoryStore::default()))
}

pub fn response_history_is_loaded(response_id: &str, history_scope: Option<&str>) -> bool {
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(response_id, history_scope, Instant::now())
        .is_some()
}

pub fn hydrate_response_history(
    response_id: &str,
    history_scope: Option<&str>,
    payload: &str,
) -> Result<(), String> {
    if payload.len() > MAX_HISTORY_ENTRY_BYTES {
        return Err("persisted response history exceeds the maximum entry size".to_string());
    }
    let persisted: PersistedResponseHistory = serde_json::from_str(payload)
        .map_err(|error| format!("invalid persisted response history: {error}"))?;
    if persisted.version != HISTORY_STORAGE_VERSION {
        return Err(format!(
            "unsupported response history version {}",
            persisted.version
        ));
    }
    if persisted.response_id != response_id.trim() {
        return Err(
            "persisted response history id does not match the requested response".to_string(),
        );
    }
    if persisted.scope_fingerprint != history_scope_fingerprint(history_scope) {
        return Err("persisted response history scope does not match the requester".to_string());
    }
    let now_unix_secs = current_unix_secs();
    let remaining_ttl = persisted
        .expires_at_unix_secs
        .checked_sub(now_unix_secs)
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .ok_or_else(|| "persisted response history has expired".to_string())?;
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            response_id.trim().to_string(),
            history_scope,
            persisted.transcript,
            Instant::now(),
            remaining_ttl.min(HISTORY_TTL),
        );
    Ok(())
}

fn request_input_items(request: &Value) -> Vec<Value> {
    match request.get("input") {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(text)) if !text.is_empty() => vec![json!({
            "type": "message",
            "role": "user",
            "content": text,
        })],
        _ if request.get("messages").and_then(Value::as_array).is_some() => {
            registry::convert_request(
                "openai:chat",
                "openai:responses",
                request,
                &FormatContext::default(),
            )
            .ok()
            .and_then(|converted| converted.get("input").and_then(Value::as_array).cloned())
            .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn history_conversion_enabled(report_context: &Value) -> bool {
    report_context
        .get("needs_conversion")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && report_context
            .get("client_api_format")
            .and_then(Value::as_str)
            .is_some_and(|format| format.eq_ignore_ascii_case("openai:responses"))
        && report_context
            .get("provider_api_format")
            .and_then(Value::as_str)
            .is_some_and(|format| format.eq_ignore_ascii_case("openai:chat"))
}

pub(crate) fn expand_previous_response_for_chat(
    request: &Value,
    history_scope: Option<&str>,
) -> Result<Value, String> {
    let Some(previous_response_id) = request
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(request.clone());
    };
    let mut store = response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(mut transcript) = store.get(previous_response_id, history_scope, Instant::now())
    else {
        return Err(format!(
            "response history not found for previous_response_id {previous_response_id}"
        ));
    };
    transcript.extend(request_input_items(request));
    let mut expanded = request.clone();
    let Some(object) = expanded.as_object_mut() else {
        return Err("OpenAI Responses request must be a JSON object".to_string());
    };
    object.remove("previous_response_id");
    object.insert("input".to_string(), Value::Array(transcript));
    Ok(expanded)
}

/// Expand a client Responses request for a cross-format conversion exactly
/// once.
///
/// Callers must hydrate the scoped previous response first (the gateway does
/// that before provider conversion). The returned body has
/// `previous_response_id` removed and the client transcript merged into
/// `input`; feed it to both the conversion and
/// [`crate::formats::openai::namespace::openai_responses_tool_identity_context`].
/// An unknown, expired or wrongly scoped `previous_response_id` is a typed
/// error: identity restoration must never guess.
pub fn expand_openai_responses_request_body(
    request: &Value,
    history_scope: Option<&str>,
) -> Result<Value, String> {
    expand_previous_response_for_chat(request, history_scope)
}

pub fn record_converted_response_history(
    report_context: &Value,
    response: &Value,
) -> Option<ResponseHistoryRecord> {
    if !history_conversion_enabled(report_context)
        || response.get("status").and_then(Value::as_str) != Some("completed")
    {
        return None;
    }
    let response_id = response
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let request = report_context.get("original_request_body")?;
    let history_scope = report_context
        .get("api_key_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|scope| !scope.is_empty());
    let previous_response_id = request
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut transcript = if request.get("messages").and_then(Value::as_array).is_some() {
        Vec::new()
    } else if let Some(previous_response_id) = previous_response_id {
        // The continued transcript is the authoritative source. Never fabricate
        // a partial (tool-only) transcript when it was evicted or expired before
        // this response landed: recording that would present an incomplete
        // conversation as complete. Without a record the next continuation fails
        // closed instead; tools for *this* response still restore from the
        // pinned identity context.
        response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(previous_response_id, history_scope, Instant::now())?
    } else {
        Vec::new()
    };
    transcript.extend(request_input_items(request));
    if let Some(output) = response.get("output").and_then(Value::as_array) {
        transcript.extend(output.iter().cloned());
    }
    let expires_at_unix_secs = current_unix_secs().saturating_add(HISTORY_TTL.as_secs());
    let payload = serde_json::to_string(&PersistedResponseHistory {
        version: HISTORY_STORAGE_VERSION,
        response_id: response_id.to_string(),
        scope_fingerprint: history_scope_fingerprint(history_scope),
        expires_at_unix_secs,
        transcript: transcript.clone(),
    })
    .ok()?;
    if payload.len() > MAX_HISTORY_ENTRY_BYTES {
        return None;
    }
    response_history_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            response_id.to_string(),
            history_scope,
            transcript,
            Instant::now(),
            HISTORY_TTL,
        );
    Some(ResponseHistoryRecord {
        storage_key: response_history_storage_key(response_id, history_scope),
        payload,
        ttl: HISTORY_TTL,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use crate::formats::openai::namespace::{
        openai_responses_tool_identity_context, NamespaceToolAliases, ToolIdentityRestoreError,
        OPENAI_RESPONSES_TOOL_IDENTITY_CONTEXT_KEY,
    };
    use crate::formats::openai::tool_lowering::ResponsesToolRestore;
    use crate::formats::shared::sync_products::convert_standard_chat_response;
    use crate::formats::shared::sync_to_stream::maybe_bridge_standard_sync_json_to_stream;
    use crate::formats::{
        context::{FormatContext, FormatError},
        registry::{convert_request, convert_response},
    };

    use super::{
        expand_openai_responses_request_body, expand_previous_response_for_chat,
        hydrate_response_history, record_converted_response_history, response_history_is_loaded,
        response_history_key, response_history_storage_key, response_history_store,
    };

    fn conversion_report_context(original_request_body: serde_json::Value) -> serde_json::Value {
        json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "original_request_body": original_request_body,
        })
    }

    #[test]
    fn expands_previous_response_with_assistant_call_before_tool_output() {
        let report_context = conversion_report_context(json!({
            "model": "deepseek-v4-flash",
            "input": [{"role": "user", "content": "scan the repository"}]
        }));
        record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_history_expand_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_expand_test_1",
                    "call_id": "call_history_expand_test_1",
                    "name": "security_scan",
                    "arguments": "{\"depth\":\"deep\"}"
                }]
            }),
        );

        let expanded = expand_previous_response_for_chat(
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_expand_test_1",
                "input": [{
                    "type": "function_call_output",
                    "call_id": "call_history_expand_test_1",
                    "output": "manifest-created"
                }]
            }),
            None,
        )
        .expect("stored previous response should expand");

        assert!(expanded.get("previous_response_id").is_none());
        assert_eq!(expanded["input"][1]["type"], "function_call");
        assert_eq!(expanded["input"][1]["id"], "fc_history_expand_test_1");
        assert_eq!(
            expanded["input"][1]["call_id"],
            "call_history_expand_test_1"
        );
        assert_eq!(expanded["input"][2]["type"], "function_call_output");
        assert_eq!(
            expanded["input"][2]["call_id"],
            "call_history_expand_test_1"
        );
    }

    #[test]
    fn restores_persisted_history_after_local_cache_reset() {
        let report_context = json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "api_key_id": "distributed-history-key-a",
            "original_request_body": {
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "inspect the repository"}]
            }
        });
        let record = record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_distributed_history_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_distributed_history_1",
                    "call_id": "call_distributed_history_1",
                    "name": "inspect_repository",
                    "arguments": "{}"
                }]
            }),
        )
        .expect("completed conversion should produce a persistence record");
        assert_eq!(
            record.storage_key,
            response_history_storage_key(
                "resp_distributed_history_1",
                Some("distributed-history-key-a")
            )
        );
        assert!(!record.storage_key.contains("distributed-history-key-a"));
        assert!(!record.storage_key.contains("resp_distributed_history_1"));

        response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&response_history_key(
                "resp_distributed_history_1",
                Some("distributed-history-key-a"),
            ));
        let continuation = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_distributed_history_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_distributed_history_1",
                "output": "inspection-complete"
            }]
        });
        assert!(expand_previous_response_for_chat(
            &continuation,
            Some("distributed-history-key-a")
        )
        .is_err());

        hydrate_response_history(
            "resp_distributed_history_1",
            Some("distributed-history-key-a"),
            &record.payload,
        )
        .expect("another instance should hydrate the persisted transcript");
        let expanded =
            expand_previous_response_for_chat(&continuation, Some("distributed-history-key-a"))
                .expect("hydrated history should support the continuation");
        assert_eq!(
            expanded["input"][1]["call_id"],
            "call_distributed_history_1"
        );
        assert_eq!(
            expanded["input"][2]["call_id"],
            "call_distributed_history_1"
        );
    }

    #[test]
    fn restores_history_recorded_from_redacted_chat_messages() {
        record_converted_response_history(
            &conversion_report_context(json!({
                "model": "deepseek-v4-flash",
                "messages": [
                    {"role": "user", "content": "scan the redacted repository"},
                    {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_redacted_previous_1",
                            "type": "function",
                            "function": {
                                "name": "security_scan",
                                "arguments": "{\"depth\":\"quick\"}"
                            }
                        }]
                    },
                    {
                        "role": "tool",
                        "tool_call_id": "call_redacted_previous_1",
                        "content": "quick-scan-complete"
                    }
                ]
            })),
            &json!({
                "id": "resp_history_redacted_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_redacted_test_1",
                    "call_id": "call_redacted_next_1",
                    "name": "deep_scan",
                    "arguments": "{\"scope\":\"changed-files\"}"
                }]
            }),
        );

        let expanded = expand_previous_response_for_chat(
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_redacted_test_1",
                "input": [{
                    "type": "function_call_output",
                    "call_id": "call_redacted_next_1",
                    "output": "deep-scan-complete"
                }]
            }),
            None,
        )
        .expect("redacted Chat history should expand");
        let input = expanded["input"]
            .as_array()
            .expect("expanded input should be an array");

        assert_eq!(input.len(), 5);
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_redacted_previous_1");
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call_redacted_previous_1");
        assert_eq!(input[3]["id"], "fc_history_redacted_test_1");
        assert_eq!(input[3]["call_id"], "call_redacted_next_1");
        assert_eq!(input[4]["type"], "function_call_output");
        assert_eq!(input[4]["call_id"], "call_redacted_next_1");
    }

    #[test]
    fn isolates_response_history_by_api_key_scope() {
        let report_context = json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": "openai:chat",
            "api_key_id": "key-history-scope-a",
            "original_request_body": {
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "private request"}]
            }
        });
        record_converted_response_history(
            &report_context,
            &json!({
                "id": "resp_history_scope_test_1",
                "status": "completed",
                "output": [{"type": "message", "role": "assistant", "content": []}]
            }),
        );
        let continuation = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_history_scope_test_1",
            "input": "continue"
        });

        let owner_result = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &FormatContext::default().with_history_scope("key-history-scope-a"),
        );
        assert!(owner_result.is_ok());

        let other_user_error = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &FormatContext::default().with_history_scope("key-history-scope-b"),
        )
        .expect_err("another API key must not recover scoped history");
        assert!(matches!(
            other_user_error,
            crate::formats::context::FormatError::UnsupportedField { ref field, .. }
                if field == "previous_response_id"
        ));
    }

    #[test]
    fn converts_all_forty_two_tools_and_forces_late_tools() {
        let tools = (0..42)
            .map(|index| {
                json!({
                    "type": "function",
                    "name": format!("security_tool_{index:02}"),
                    "description": format!("Security tool {index}"),
                    "parameters": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"]
                    }
                })
            })
            .collect::<Vec<_>>();

        for forced_index in [18usize, 41usize] {
            let body = json!({
                "model": "deepseek-v4-flash",
                "input": [{"role": "user", "content": "run the selected scan"}],
                "tools": tools,
                "tool_choice": {
                    "type": "function",
                    "name": format!("security_tool_{forced_index:02}")
                }
            });
            let converted = convert_request(
                "openai:responses",
                "openai:chat",
                &body,
                &FormatContext::default(),
            )
            .expect("all Responses tools should convert to Chat");
            let converted_tools = converted["tools"]
                .as_array()
                .expect("chat tools should be an array");

            assert_eq!(converted_tools.len(), 42);
            for (index, tool) in converted_tools.iter().enumerate() {
                assert_eq!(
                    tool["function"]["name"],
                    json!(format!("security_tool_{index:02}"))
                );
            }
            assert_eq!(
                converted["tool_choice"]["function"]["name"],
                json!(format!("security_tool_{forced_index:02}"))
            );
        }
    }

    #[test]
    fn restores_two_consecutive_tool_call_rounds() {
        let first_request = json!({
            "model": "deepseek-v4-flash",
            "input": [{"role": "user", "content": "perform a deep scan"}],
            "parallel_tool_calls": true
        });
        record_converted_response_history(
            &conversion_report_context(first_request.clone()),
            &json!({
                "id": "resp_history_multiturn_test_1",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_history_multiturn_test_1",
                    "call_id": "call_discovery_manifest_1",
                    "name": "create_discovery_manifest",
                    "arguments": "{\"root\":\"src\"}"
                }]
            }),
        );

        let second_request = json!({
            "model": "deepseek-v4-flash",
            "previous_response_id": "resp_history_multiturn_test_1",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_discovery_manifest_1",
                "output": "manifest-1"
            }],
            "parallel_tool_calls": true
        });
        let second_chat = convert_request(
            "openai:responses",
            "openai:chat",
            &second_request,
            &FormatContext::default(),
        )
        .expect("first continuation should convert");
        assert_eq!(second_chat["messages"][1]["role"], "assistant");
        assert_eq!(
            second_chat["messages"][1]["tool_calls"][0]["id"],
            "call_discovery_manifest_1"
        );
        assert_eq!(second_chat["messages"][2]["role"], "tool");
        assert_eq!(
            second_chat["messages"][2]["tool_call_id"],
            "call_discovery_manifest_1"
        );

        record_converted_response_history(
            &conversion_report_context(second_request.clone()),
            &json!({
                "id": "resp_history_multiturn_test_2",
                "status": "completed",
                "output": [
                    {
                        "type": "function_call",
                        "id": "fc_history_multiturn_test_2a",
                        "call_id": "call_deep_scan_2a",
                        "name": "deep_scan",
                        "arguments": "{\"manifest\":\"manifest-1\"}"
                    },
                    {
                        "type": "function_call",
                        "id": "fc_history_multiturn_test_2b",
                        "call_id": "call_audit_2b",
                        "name": "audit_results",
                        "arguments": "{\"manifest\":\"manifest-1\"}"
                    }
                ]
            }),
        );

        let third_chat = convert_request(
            "openai:responses",
            "openai:chat",
            &json!({
                "model": "deepseek-v4-flash",
                "previous_response_id": "resp_history_multiturn_test_2",
                "input": [
                    {
                        "type": "function_call_output",
                        "call_id": "call_deep_scan_2a",
                        "output": "scan-complete"
                    },
                    {
                        "type": "function_call_output",
                        "call_id": "call_audit_2b",
                        "output": "audit-complete"
                    }
                ]
            }),
            &FormatContext::default(),
        )
        .expect("second continuation should convert");
        let messages = third_chat["messages"]
            .as_array()
            .expect("chat messages should be an array");
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[3]["role"], "assistant");
        assert_eq!(messages[3]["tool_calls"].as_array().map(Vec::len), Some(2));
        assert_eq!(messages[3]["tool_calls"][0]["id"], "call_deep_scan_2a");
        assert_eq!(messages[3]["tool_calls"][1]["id"], "call_audit_2b");
        assert_eq!(messages[4]["tool_call_id"], "call_deep_scan_2a");
        assert_eq!(messages[5]["tool_call_id"], "call_audit_2b");
        // Every assistant call pairs with exactly one tool result, by id.
        let paired = messages[3..]
            .iter()
            .filter_map(|message| message["tool_call_id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(paired, vec!["call_deep_scan_2a", "call_audit_2b"]);
    }

    fn tool_search_tools() -> serde_json::Value {
        json!([{
            "type": "tool_search",
            "execution": "client",
            "description": "Find tools",
            "parameters": {"type": "object", "properties": {"goal": {"type": "string"}}}
        }])
    }

    fn discovered_namespace_output(call_id: &str) -> serde_json::Value {
        json!({
            "type": "tool_search_output",
            "call_id": call_id,
            "execution": "client",
            "tools": [{
                "type": "namespace",
                "name": "crm",
                "description": "CRM tools",
                "tools": [{
                    "type": "function",
                    "name": "lookup",
                    "parameters": {"type": "object"}
                }]
            }]
        })
    }

    fn identity_report_context(
        original_request_body: serde_json::Value,
        provider_api_format: &str,
        scope: &str,
    ) -> serde_json::Value {
        json!({
            "needs_conversion": true,
            "client_api_format": "openai:responses",
            "provider_api_format": provider_api_format,
            "api_key_id": scope,
            "original_request_body": original_request_body,
        })
    }

    fn chat_lookup_response() -> serde_json::Value {
        json!({
            "id": "chatcmpl-identity",
            "object": "chat.completion",
            "model": "gpt-source",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_lookup",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"42\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        })
    }

    fn claude_lookup_response() -> serde_json::Value {
        json!({
            "id": "msg_identity",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-5",
            "content": [{
                "type": "tool_use",
                "id": "toolu_lookup",
                "name": "lookup",
                "input": {"q": "42"}
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        })
    }

    fn restored_lookup_item(body: &serde_json::Value) -> serde_json::Value {
        body["output"]
            .as_array()
            .expect("responses output")
            .iter()
            .find(|item| item["type"] == "function_call")
            .cloned()
            .expect("restored function call item")
    }

    /// Records turn one (with a discovered namespace child in its input) and
    /// returns the continuation body a client would send next.
    fn record_turn_one(scope: &str, response_id: &str) -> serde_json::Value {
        record_converted_response_history(
            &identity_report_context(
                json!({
                    "model": "gpt-source",
                    "tools": tool_search_tools(),
                    "input": [
                        {"role": "user", "content": "find crm tools"},
                        discovered_namespace_output("call_search_t1"),
                    ]
                }),
                "openai:chat",
                scope,
            ),
            &json!({
                "id": response_id,
                "status": "completed",
                "output": [{
                    "type": "tool_search_call",
                    "call_id": "call_search_t1",
                    "execution": "client",
                    "arguments": {"goal": "crm"}
                }]
            }),
        )
        .expect("turn one history should record");
        json!({
            "model": "gpt-source",
            "previous_response_id": response_id,
            "tools": tool_search_tools(),
            "input": [{"role": "user", "content": "look up the customer"}]
        })
    }

    #[test]
    fn library_roundtrip_restores_namespace_identity_from_scoped_history() {
        let scope = "history-identity-roundtrip";
        let continuation = record_turn_one(scope, "resp_identity_roundtrip_t1");
        let request_ctx = FormatContext::default().with_history_scope(scope);

        let chat_body = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &request_ctx,
        )
        .expect("continuation should convert with scoped history");
        assert!(
            chat_body["tools"]
                .as_array()
                .expect("chat tools")
                .iter()
                .any(|tool| tool["function"]["name"] == "lookup"),
            "{chat_body}"
        );

        let response_ctx = FormatContext::default().with_report_context(identity_report_context(
            continuation.clone(),
            "openai:chat",
            scope,
        ));
        let responses_body = convert_response(
            "openai:chat",
            "openai:responses",
            &chat_lookup_response(),
            &response_ctx,
        )
        .expect("response conversion should restore the scoped identity");
        let item = restored_lookup_item(&responses_body);
        assert_eq!(item["namespace"], "crm");
        assert_eq!(item["name"], "lookup");

        let expanded = expand_openai_responses_request_body(&continuation, Some(scope))
            .expect("library callers expand the continuation once with the scope");
        let claude_body = convert_request(
            "openai:responses",
            "claude:messages",
            &expanded,
            &FormatContext::default(),
        )
        .expect("expanded continuation should convert for Claude");
        assert!(
            claude_body["tools"]
                .as_array()
                .expect("claude tools")
                .iter()
                .any(|tool| tool["name"] == "lookup"),
            "{claude_body}"
        );
        let claude_ctx = FormatContext::default().with_report_context(identity_report_context(
            continuation,
            "claude:messages",
            scope,
        ));
        let responses_body = convert_response(
            "claude:messages",
            "openai:responses",
            &claude_lookup_response(),
            &claude_ctx,
        )
        .expect("claude response should restore the scoped identity");
        let item = restored_lookup_item(&responses_body);
        assert_eq!(item["namespace"], "crm");
        assert_eq!(item["name"], "lookup");
    }

    #[test]
    fn real_chat_response_replays_into_a_chat_request() {
        let scope = "history-identity-real-replay";
        let first_request = json!({
            "model": "gpt-source",
            "tools": tool_search_tools(),
            "input": [
                {"role": "user", "content": "find crm tools"},
                discovered_namespace_output("call_search_t1")
            ]
        });
        let report_context = identity_report_context(first_request, "openai:chat", scope);

        // Real provider conversion: a Chat sync completion becomes a Responses
        // body whose synthesized assistant message item carries item id/status
        // metadata. History replay must downgrade exactly that metadata.
        let responses_body = convert_response(
            "openai:chat",
            "openai:responses",
            &json!({
                "id": "chatcmpl-identity-replay",
                "object": "chat.completion",
                "model": "gpt-source",
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "crm tools discovered"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
            }),
            &FormatContext::default().with_report_context(report_context.clone()),
        )
        .expect("Chat sync body converts to Responses");
        let message_item = responses_body["output"]
            .as_array()
            .expect("responses output")
            .iter()
            .find(|item| item["type"] == "message")
            .expect("assistant message item")
            .clone();
        assert!(message_item.get("id").is_some(), "{message_item}");
        assert_eq!(message_item["status"], "completed");
        let response_id = responses_body["id"]
            .as_str()
            .expect("responses id")
            .to_string();
        assert!(
            record_converted_response_history(&report_context, &responses_body)
                .expect("completed conversion records history")
                .payload
                .contains("crm tools discovered")
        );

        let continuation = json!({
            "model": "gpt-source",
            "previous_response_id": response_id,
            "tools": tool_search_tools(),
            "input": [{"role": "user", "content": "now use it"}]
        });
        let chat_body = convert_request(
            "openai:responses",
            "openai:chat",
            &continuation,
            &FormatContext::default().with_history_scope(scope),
        )
        .expect("recorded Responses history must replay into Chat");
        let messages = chat_body["messages"].as_array().expect("chat messages");
        assert!(
            messages.iter().any(|message| {
                message["role"] == "assistant" && message["content"] == "crm tools discovered"
            }),
            "{chat_body}"
        );
        assert!(
            chat_body["tools"]
                .as_array()
                .expect("chat tools")
                .iter()
                .any(|tool| tool["function"]["name"] == "lookup"),
            "{chat_body}"
        );

        // Only established message metadata is downgraded; unknown source
        // fields still fail closed rather than being stripped.
        let smuggled = json!({
            "model": "gpt-source",
            "input": [{
                "type": "message",
                "role": "assistant",
                "id": "msg_smuggled",
                "status": "completed",
                "future_semantic": {"k": 1},
                "content": [{"type": "output_text", "text": "hi", "annotations": []}]
            }]
        });
        assert!(convert_request(
            "openai:responses",
            "openai:chat",
            &smuggled,
            &FormatContext::default(),
        )
        .is_err());
    }

    #[test]
    fn pinned_tool_identity_survives_history_expiration() {
        let scope = "history-identity-expired";
        let previous_response_id = "resp_identity_expired_t1";
        response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                previous_response_id.to_string(),
                Some(scope),
                vec![json!({"role": "user", "content": "earlier"})],
                Instant::now() - Duration::from_secs(10),
                Duration::from_secs(1),
            );
        assert!(!response_history_is_loaded(
            previous_response_id,
            Some(scope)
        ));

        let continuation = json!({
            "model": "gpt-source",
            "previous_response_id": previous_response_id,
            "tools": tool_search_tools(),
            "input": [{"role": "user", "content": "look up the customer"}]
        });
        let effective_body = json!({
            "model": "gpt-source",
            "tools": tool_search_tools(),
            "input": [
                discovered_namespace_output("call_search_t1"),
                {"role": "user", "content": "look up the customer"}
            ]
        });
        let mut report_context =
            identity_report_context(continuation.clone(), "openai:chat", scope);
        report_context[OPENAI_RESPONSES_TOOL_IDENTITY_CONTEXT_KEY] =
            openai_responses_tool_identity_context(&effective_body);

        // The pinned projection keeps tool identities only, never the search
        // call envelope or conversation content.
        let pinned = &report_context[OPENAI_RESPONSES_TOOL_IDENTITY_CONTEXT_KEY];
        assert!(pinned["input"][0].get("call_id").is_none());
        assert!(pinned["input"][0].get("execution").is_none());
        assert!(pinned["input"][0].get("content").is_none());
        assert_eq!(pinned["input"][0]["tools"][0]["name"], "crm");

        let aliases = NamespaceToolAliases::try_from_report_context(&report_context)
            .expect("pinned identity must not depend on cached history");
        assert_eq!(
            aliases.restore_tool_call("lookup"),
            ResponsesToolRestore::NamespaceChild {
                namespace: "crm".to_string(),
                name: "lookup".to_string(),
            }
        );

        let pinned_response = convert_response(
            "openai:chat",
            "openai:responses",
            &chat_lookup_response(),
            &FormatContext::default().with_report_context(report_context),
        )
        .expect("pinned identity restores the namespace");
        assert_eq!(restored_lookup_item(&pinned_response)["namespace"], "crm");

        // Without the pinned sidecar the expired history must fail closed.
        let legacy_context = identity_report_context(continuation, "openai:chat", scope);
        assert_eq!(
            NamespaceToolAliases::try_from_report_context(&legacy_context),
            Err(ToolIdentityRestoreError::HistoryUnavailable)
        );
        let error = convert_response(
            "openai:chat",
            "openai:responses",
            &chat_lookup_response(),
            &FormatContext::default().with_report_context(legacy_context.clone()),
        )
        .expect_err("unavailable identity must fail closed");
        match error {
            FormatError::LossyConversionBlocked { field, .. } => {
                assert_eq!(field, "previous_response_id");
            }
            other => panic!("expected LossyConversionBlocked, got {other:?}"),
        }
        // The sync product bridge must not fall back to a pairwise converter.
        assert!(convert_standard_chat_response(
            &chat_lookup_response(),
            "openai:chat",
            "openai:responses",
            &legacy_context,
        )
        .is_none());
    }

    #[test]
    fn wrong_scope_or_unknown_previous_response_rejects_identity() {
        let owner_scope = "history-identity-owner";
        let other_scope = "history-identity-other";
        let continuation = record_turn_one(owner_scope, "resp_identity_scoped_t1");

        let record = record_converted_response_history(
            &identity_report_context(
                json!({"model": "gpt-source", "input": [{"role": "user", "content": "inspect"}]}),
                "openai:chat",
                owner_scope,
            ),
            &json!({
                "id": "resp_identity_scoped_other",
                "status": "completed",
                "output": []
            }),
        )
        .expect("completed conversion records history");
        assert!(hydrate_response_history(
            "resp_identity_scoped_other",
            Some(other_scope),
            &record.payload
        )
        .is_err());

        let wrong_scope = identity_report_context(continuation.clone(), "openai:chat", other_scope);
        assert_eq!(
            NamespaceToolAliases::try_from_report_context(&wrong_scope),
            Err(ToolIdentityRestoreError::HistoryUnavailable)
        );
        assert!(convert_response(
            "openai:chat",
            "openai:responses",
            &chat_lookup_response(),
            &FormatContext::default().with_report_context(wrong_scope),
        )
        .is_err());

        let unknown = identity_report_context(
            json!({
                "model": "gpt-source",
                "previous_response_id": "resp_identity_missing",
                "input": []
            }),
            "openai:chat",
            owner_scope,
        );
        assert_eq!(
            NamespaceToolAliases::try_from_report_context(&unknown),
            Err(ToolIdentityRestoreError::HistoryUnavailable)
        );
        // A recorded history for the same id but a different scope cannot be
        // re-read by the owner of another scope.
        assert!(expand_previous_response_for_chat(&continuation, Some(other_scope)).is_err());
    }

    #[test]
    fn continuation_record_preserves_the_full_transcript() {
        let scope = "history-identity-two-turn";
        let turn_two_body = record_turn_one(scope, "resp_identity_two_turn_t1");
        let expanded_turn_two = expand_previous_response_for_chat(&turn_two_body, Some(scope))
            .expect("turn two expands while the entry exists");
        let mut turn_two_context = identity_report_context(turn_two_body, "openai:chat", scope);
        turn_two_context[OPENAI_RESPONSES_TOOL_IDENTITY_CONTEXT_KEY] =
            openai_responses_tool_identity_context(&expanded_turn_two);

        record_converted_response_history(
            &turn_two_context,
            &json!({
                "id": "resp_identity_two_turn_t2",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_two_turn",
                    "call_id": "call_two_turn",
                    "name": "lookup",
                    "arguments": "{\"q\":\"42\"}"
                }]
            }),
        )
        .expect("a continuation records the full transcript");

        let transcript = response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get("resp_identity_two_turn_t2", Some(scope), Instant::now())
            .expect("turn two transcript");
        // The full stored transcript survives: turn one's turns, the discovery
        // item, turn two's input and the response output.
        assert!(transcript
            .iter()
            .any(|item| item["type"] == "tool_search_output" && item["tools"][0]["name"] == "crm"));
        assert!(transcript
            .iter()
            .any(|item| item["content"] == "find crm tools"));
        assert!(transcript
            .iter()
            .any(|item| item["content"] == "look up the customer"));
        assert!(transcript
            .iter()
            .any(|item| item["type"] == "function_call" && item["name"] == "lookup"));

        let turn_three = identity_report_context(
            json!({
                "model": "gpt-source",
                "previous_response_id": "resp_identity_two_turn_t2",
                "input": [{"role": "user", "content": "again"}]
            }),
            "openai:chat",
            scope,
        );
        let aliases = NamespaceToolAliases::try_from_report_context(&turn_three)
            .expect("second continuation keeps the discovered identity");
        assert_eq!(
            aliases.restore_tool_call("lookup"),
            ResponsesToolRestore::NamespaceChild {
                namespace: "crm".to_string(),
                name: "lookup".to_string(),
            }
        );
    }

    #[test]
    fn continuation_record_refuses_incomplete_transcript_after_eviction() {
        let scope = "history-identity-evicted";
        let turn_two_body = record_turn_one(scope, "resp_identity_evicted_t1");
        let expanded_turn_two = expand_previous_response_for_chat(&turn_two_body, Some(scope))
            .expect("turn two expands while the entry exists");
        let mut turn_two_context = identity_report_context(turn_two_body, "openai:chat", scope);
        turn_two_context[OPENAI_RESPONSES_TOOL_IDENTITY_CONTEXT_KEY] =
            openai_responses_tool_identity_context(&expanded_turn_two);

        // The continued transcript is evicted between request and response.
        response_history_store()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&response_history_key(
                "resp_identity_evicted_t1",
                Some(scope),
            ));
        assert!(!response_history_is_loaded(
            "resp_identity_evicted_t1",
            Some(scope)
        ));

        // Recording a tool-only + new-turn transcript would present an
        // incomplete conversation as complete: refuse it instead.
        assert!(record_converted_response_history(
            &turn_two_context,
            &json!({
                "id": "resp_identity_evicted_t2",
                "status": "completed",
                "output": [{
                    "type": "function_call",
                    "id": "fc_evicted",
                    "call_id": "call_evicted",
                    "name": "lookup",
                    "arguments": "{\"q\":\"42\"}"
                }]
            }),
        )
        .is_none());
        assert!(!response_history_is_loaded(
            "resp_identity_evicted_t2",
            Some(scope)
        ));

        // The pinned identity still restores the current response...
        let aliases = NamespaceToolAliases::try_from_report_context(&turn_two_context)
            .expect("pinned identity restores the current response");
        assert_eq!(
            aliases.restore_tool_call("lookup"),
            ResponsesToolRestore::NamespaceChild {
                namespace: "crm".to_string(),
                name: "lookup".to_string(),
            }
        );
        // ...while the next continuation fails closed instead of silently
        // losing the earlier conversation and discovery.
        let turn_three = identity_report_context(
            json!({
                "model": "gpt-source",
                "previous_response_id": "resp_identity_evicted_t2",
                "input": [{"role": "user", "content": "again"}]
            }),
            "openai:chat",
            scope,
        );
        assert_eq!(
            NamespaceToolAliases::try_from_report_context(&turn_three),
            Err(ToolIdentityRestoreError::HistoryUnavailable)
        );
    }

    #[test]
    fn sync_to_stream_bridge_uses_pinned_identity_when_history_is_gone() {
        let report_context = json!({
            "provider_api_format": "openai:chat",
            "client_api_format": "openai:responses",
            "needs_conversion": true,
            "api_key_id": "history-identity-bridge",
            "original_request_body": {
                "model": "gpt-source",
                "previous_response_id": "resp_identity_bridge_missing",
                "input": [{"role": "user", "content": "look up"}]
            },
            "responses_tool_identity": {
                "tools": [{"type": "tool_search", "execution": "client"}],
                "input": [{
                    "type": "tool_search_output",
                    "tools": [{
                        "type": "namespace",
                        "name": "crm",
                        "description": "CRM tools",
                        "tools": [{
                            "type": "function",
                            "name": "lookup",
                            "parameters": {"type": "object"}
                        }]
                    }]
                }]
            }
        });
        let outcome = maybe_bridge_standard_sync_json_to_stream(
            &chat_lookup_response(),
            "openai:chat",
            "openai:responses",
            Some(&report_context),
        )
        .expect("bridge should accept the pinned identity")
        .expect("bridge should produce SSE");
        let sse = String::from_utf8(outcome.sse_body).expect("sse should be utf8");
        assert!(sse.contains("\"namespace\":\"crm\""), "{sse}");
        assert!(sse.contains("\"name\":\"lookup\""), "{sse}");
    }
}
