use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

use aether_ai_formats::formats::shared::response::build_generated_tool_call_id;
use serde_json::{Map, Value};

const CLAUDE_MODEL_PREFIX: &[u8] = b"claude-";
const ID_ALIAS_KEYS: [&str; 2] = ["call_id", "callId"];

/// 仅在 Antigravity 私有 `v1internal` Claude 边界补齐 Gemini 原生工具调用与结果的缺失 `id`。
///
/// 边界与优先级约定：
/// 1. 仅对 `claude-*` 目标模型生效；公共 Gemini 透传与非 Claude 模型完全保持原样。
/// 2. 仅识别协议位置的 `model` 角色 `functionCall`/`function_call` 与 `user`/`tool`/`function`
///    角色 `functionResponse`/`function_response`，绝不递归扫描或改写 `args`/`response` 业务载荷。
/// 3. 显式非空标识遵循 `id` > `call_id` > `callId` 优先级，保留原始非空字节与别名字段；
///    缺失、null、空串或纯空白视为待补齐，非字符串畸形 `id` 原样保留交由上游拒绝。
/// 4. 先预留全量显式调用与结果 ID（含后续轮次）以避免生成碰撞；再按“显式 ID 精确匹配优先 ->
///    因果前置待决队列内同名 FIFO 顺序配对 -> 单侧显式 ID 双向复用 -> 双侧缺失生成 `call_auto_N`”
///    闭环修复。显式冲突对消费配对位但不改写或跨连无关调用，孤儿结果不伪造关联。
pub(super) fn normalize_antigravity_claude_tool_history(
    request: &mut Map<String, Value>,
    model: &str,
) {
    if !is_claude_model(model) {
        return;
    }
    let Some(contents) = request.get_mut("contents").and_then(Value::as_array_mut) else {
        return;
    };
    if !history_needs_tool_id_repair(contents) {
        return;
    }

    let patches = collect_canonical_id_patches(contents);
    for patch in patches {
        write_canonical_id(contents, patch.position, patch.field_key, patch.id);
    }
}

struct CanonicalIdPatch {
    position: (usize, usize),
    field_key: &'static str,
    id: String,
}

fn collect_canonical_id_patches(contents: &[Value]) -> Vec<CanonicalIdPatch> {
    let mut calls = Vec::<ProtocolNode<'_>>::new();
    let mut responses = Vec::<ProtocolNode<'_>>::new();
    let mut reserved_ids = HashSet::<&str>::new();
    let mut assigned_call_ids = HashSet::<&str>::new();

    for (content_index, message) in contents.iter().enumerate() {
        let Some(message_obj) = message.as_object() else {
            continue;
        };
        let role_kind = classify_message_role(message_obj);
        let Some(parts) = message_obj.get("parts").and_then(Value::as_array) else {
            continue;
        };

        for (part_index, part) in parts.iter().enumerate() {
            let Some(part_obj) = part.as_object() else {
                continue;
            };
            reserve_part_explicit_ids(part_obj, &mut reserved_ids);

            match role_kind {
                MessageRoleKind::Model => {
                    let Some((field_key, call_obj)) =
                        protocol_object(part_obj, ["functionCall", "function_call"])
                    else {
                        continue;
                    };
                    let id_state = classify_node_id(call_obj);
                    if let Some(explicit_id) = id_state.explicit_id() {
                        assigned_call_ids.insert(explicit_id);
                    }
                    calls.push(ProtocolNode {
                        position: (content_index, part_index),
                        field_key,
                        name: extract_tool_name(call_obj),
                        id_state,
                    });
                }
                MessageRoleKind::UserOrTool => {
                    let Some((field_key, res_obj)) =
                        protocol_object(part_obj, ["functionResponse", "function_response"])
                    else {
                        continue;
                    };
                    responses.push(ProtocolNode {
                        position: (content_index, part_index),
                        field_key,
                        name: extract_tool_name(res_obj),
                        id_state: classify_node_id(res_obj),
                    });
                }
                MessageRoleKind::Other => {}
            }
        }
    }

    if calls.is_empty() && responses.is_empty() {
        return Vec::new();
    }

    let mut call_matched = vec![false; calls.len()];
    let mut call_resolved_id = calls
        .iter()
        .map(|node| node.id_state.explicit_id().map(Cow::Borrowed))
        .collect::<Vec<_>>();
    let mut response_matched_call = vec![None::<usize>; responses.len()];

    // 第一阶段：在因果前置范围内优先完成显式 ID 精确配对，防止同名无 ID 或乱序结果抢占已绑定调用。
    match_explicit_id_pairs(
        &calls,
        &responses,
        &mut call_matched,
        &mut response_matched_call,
    );

    // 最后在因果前置待决队列中按工具名与发生顺序（FIFO）配对剩余调用和结果。
    match_causal_fifo_pairs(
        &calls,
        &responses,
        &mut call_matched,
        &mut call_resolved_id,
        &mut response_matched_call,
        &mut assigned_call_ids,
    );

    // 单调递增序号本身保证新 ID 互不重复，只需避开输入中预留的显式 ID。
    let mut next_generated_index = 0usize;
    for (c_idx, call) in calls.iter().enumerate() {
        if matches!(call.id_state, NodeIdState::Missing) && call_resolved_id[c_idx].is_none() {
            let generated = loop {
                let candidate = build_generated_tool_call_id(next_generated_index);
                next_generated_index += 1;
                if !reserved_ids.contains(candidate.as_str()) {
                    break candidate;
                }
            };
            call_resolved_id[c_idx] = Some(Cow::Owned(generated));
        }
    }

    // 第四阶段：仅对需要新增或提升 canonical `id` 的节点生成回填补丁。
    let mut patches = Vec::new();
    for (r_idx, response) in responses.iter().enumerate() {
        match response.id_state {
            NodeIdState::Alias(id) => patches.push(CanonicalIdPatch {
                position: response.position,
                field_key: response.field_key,
                id: id.to_owned(),
            }),
            NodeIdState::Missing => {
                if let Some(id) = response_matched_call[r_idx]
                    .and_then(|c_idx| call_resolved_id[c_idx].as_deref())
                {
                    patches.push(CanonicalIdPatch {
                        position: response.position,
                        field_key: response.field_key,
                        id: id.to_owned(),
                    });
                }
            }
            NodeIdState::Canonical(_) | NodeIdState::Malformed => {}
        }
    }
    for (c_idx, call) in calls.iter().enumerate() {
        match call.id_state {
            NodeIdState::Alias(id) => patches.push(CanonicalIdPatch {
                position: call.position,
                field_key: call.field_key,
                id: id.to_owned(),
            }),
            NodeIdState::Missing => {
                if let Some(id) = call_resolved_id[c_idx].take() {
                    patches.push(CanonicalIdPatch {
                        position: call.position,
                        field_key: call.field_key,
                        id: id.into_owned(),
                    });
                }
            }
            NodeIdState::Canonical(_) | NodeIdState::Malformed => {}
        }
    }
    patches
}

fn match_explicit_id_pairs<'a>(
    calls: &[ProtocolNode<'a>],
    responses: &[ProtocolNode<'a>],
    call_matched: &mut [bool],
    response_matched_call: &mut [Option<usize>],
) {
    let mut pending_by_id = HashMap::<&'a str, VecDeque<usize>>::new();
    let mut c_cursor = 0usize;
    for (r_idx, response) in responses.iter().enumerate() {
        while c_cursor < calls.len() && calls[c_cursor].position < response.position {
            if let Some(call_id) = calls[c_cursor].id_state.explicit_id() {
                pending_by_id
                    .entry(call_id)
                    .or_default()
                    .push_back(c_cursor);
            }
            c_cursor += 1;
        }
        let Some(res_id) = response.id_state.explicit_id() else {
            continue;
        };
        let Some(queue) = pending_by_id.get_mut(res_id) else {
            continue;
        };
        if let Some(c_idx) = pop_compatible_explicit_call(queue, calls, call_matched, response.name)
        {
            call_matched[c_idx] = true;
            response_matched_call[r_idx] = Some(c_idx);
        }
    }
}

fn match_causal_fifo_pairs<'a>(
    calls: &[ProtocolNode<'a>],
    responses: &[ProtocolNode<'a>],
    call_matched: &mut [bool],
    call_resolved_id: &mut [Option<Cow<'a, str>>],
    response_matched_call: &mut [Option<usize>],
    assigned_call_ids: &mut HashSet<&'a str>,
) {
    let mut pending_by_name = HashMap::<&'a str, VecDeque<usize>>::new();
    let mut pending_missing_by_name = HashMap::<&'a str, VecDeque<usize>>::new();
    let mut pending_all = VecDeque::<usize>::new();
    let mut c_cursor = 0usize;

    for (r_idx, response) in responses.iter().enumerate() {
        while c_cursor < calls.len() && calls[c_cursor].position < response.position {
            if !call_matched[c_cursor]
                && !matches!(calls[c_cursor].id_state, NodeIdState::Malformed)
            {
                if let Some(name) = calls[c_cursor].name {
                    pending_by_name.entry(name).or_default().push_back(c_cursor);
                    if matches!(calls[c_cursor].id_state, NodeIdState::Missing) {
                        pending_missing_by_name
                            .entry(name)
                            .or_default()
                            .push_back(c_cursor);
                    }
                }
                pending_all.push_back(c_cursor);
            }
            c_cursor += 1;
        }
        if response_matched_call[r_idx].is_some()
            || matches!(response.id_state, NodeIdState::Malformed)
        {
            continue;
        }

        // 仅当前结果有显式 ID 时，优先同名无 ID 调用；不能提前占用已被更早结果消费的调用。
        let result_only_call = match (response.id_state.explicit_id(), response.name) {
            (Some(id), Some(name)) if !assigned_call_ids.contains(id) => pending_missing_by_name
                .get_mut(name)
                .and_then(|queue| pop_next_unmatched_call(queue, call_matched)),
            _ => None,
        };
        let matched_call = result_only_call.or_else(|| {
            if let Some(res_name) = response.name {
                pending_by_name
                    .get_mut(res_name)
                    .and_then(|queue| pop_next_unmatched_call(queue, call_matched))
            } else if response.id_state.explicit_id().is_none() {
                pop_next_unmatched_call(&mut pending_all, call_matched)
            } else {
                None
            }
        });

        let Some(c_idx) = matched_call else {
            continue;
        };
        call_matched[c_idx] = true;
        response_matched_call[r_idx] = Some(c_idx);
        if calls[c_idx].id_state.explicit_id().is_none() {
            if let Some(id) = response.id_state.explicit_id() {
                if assigned_call_ids.insert(id) {
                    call_resolved_id[c_idx] = Some(Cow::Borrowed(id));
                }
            }
        }
    }
}

fn is_claude_model(model: &str) -> bool {
    model
        .trim()
        .as_bytes()
        .get(..CLAUDE_MODEL_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(CLAUDE_MODEL_PREFIX))
}

#[derive(Clone, Copy)]
enum MessageRoleKind {
    Model,
    UserOrTool,
    Other,
}

fn classify_message_role(message: &Map<String, Value>) -> MessageRoleKind {
    match message.get("role").and_then(Value::as_str).map(str::trim) {
        Some(role) if role.eq_ignore_ascii_case("model") => MessageRoleKind::Model,
        None => MessageRoleKind::UserOrTool,
        Some(role)
            if role.eq_ignore_ascii_case("user")
                || role.eq_ignore_ascii_case("tool")
                || role.eq_ignore_ascii_case("function") =>
        {
            MessageRoleKind::UserOrTool
        }
        Some(_) => MessageRoleKind::Other,
    }
}

/// 快速路径：若所有协议位置的工具调用与结果均已具备非空 canonical `id`，无需分配状态或修改请求。
fn history_needs_tool_id_repair(contents: &[Value]) -> bool {
    for message in contents {
        let Some(message_obj) = message.as_object() else {
            continue;
        };
        let role_kind = classify_message_role(message_obj);
        if matches!(role_kind, MessageRoleKind::Other) {
            continue;
        }
        let Some(parts) = message_obj.get("parts").and_then(Value::as_array) else {
            continue;
        };
        for part in parts {
            let Some(part_obj) = part.as_object() else {
                continue;
            };
            let func_obj = match role_kind {
                MessageRoleKind::Model => {
                    protocol_object(part_obj, ["functionCall", "function_call"]).map(|(_, obj)| obj)
                }
                MessageRoleKind::UserOrTool => {
                    protocol_object(part_obj, ["functionResponse", "function_response"])
                        .map(|(_, obj)| obj)
                }
                MessageRoleKind::Other => None,
            };
            if let Some(func_obj) = func_obj {
                let has_canonical_id = func_obj
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.trim().is_empty());
                if !has_canonical_id {
                    return true;
                }
            }
        }
    }
    false
}

fn protocol_object<'a>(
    part_obj: &'a Map<String, Value>,
    keys: [&'static str; 2],
) -> Option<(&'static str, &'a Map<String, Value>)> {
    for key in keys {
        if let Some(obj) = part_obj.get(key).and_then(Value::as_object) {
            return Some((key, obj));
        }
    }
    None
}

#[derive(Clone, Copy, Debug)]
enum NodeIdState<'a> {
    Canonical(&'a str),
    Alias(&'a str),
    Missing,
    Malformed,
}

impl<'a> NodeIdState<'a> {
    fn explicit_id(self) -> Option<&'a str> {
        match self {
            Self::Canonical(id) | Self::Alias(id) => Some(id),
            Self::Missing | Self::Malformed => None,
        }
    }
}

fn classify_node_id(func_obj: &Map<String, Value>) -> NodeIdState<'_> {
    match func_obj.get("id") {
        Some(Value::String(id)) if !id.trim().is_empty() => {
            return NodeIdState::Canonical(id.as_str());
        }
        Some(Value::Null) | Some(Value::String(_)) | None => {}
        Some(_) => return NodeIdState::Malformed,
    }
    for alias in ID_ALIAS_KEYS {
        match func_obj.get(alias) {
            Some(Value::String(id)) if !id.trim().is_empty() => {
                return NodeIdState::Alias(id.as_str());
            }
            Some(Value::Null) | Some(Value::String(_)) | None => {}
            Some(_) => return NodeIdState::Malformed,
        }
    }
    NodeIdState::Missing
}

fn extract_tool_name(func_obj: &Map<String, Value>) -> Option<&str> {
    func_obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn reserve_id<'a>(reserved_ids: &mut HashSet<&'a str>, id: &'a str) {
    reserved_ids.insert(id);
    let trimmed = id.trim();
    if trimmed != id {
        reserved_ids.insert(trimmed);
    }
}

fn reserve_part_explicit_ids<'a>(
    part_obj: &'a Map<String, Value>,
    reserved_ids: &mut HashSet<&'a str>,
) {
    for key in [
        "functionCall",
        "function_call",
        "functionResponse",
        "function_response",
    ] {
        let Some(func_obj) = part_obj.get(key).and_then(Value::as_object) else {
            continue;
        };
        for field in ["id", "call_id", "callId"] {
            if let Some(id) = func_obj
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                reserve_id(reserved_ids, id);
            }
        }
    }
}

struct ProtocolNode<'a> {
    position: (usize, usize),
    field_key: &'static str,
    name: Option<&'a str>,
    id_state: NodeIdState<'a>,
}

fn names_compatible(call_name: Option<&str>, response_name: Option<&str>) -> bool {
    match (call_name, response_name) {
        (Some(c_name), Some(r_name)) => c_name == r_name,
        _ => true,
    }
}

fn pop_compatible_explicit_call(
    queue: &mut VecDeque<usize>,
    calls: &[ProtocolNode<'_>],
    call_matched: &[bool],
    response_name: Option<&str>,
) -> Option<usize> {
    while queue.front().is_some_and(|&idx| call_matched[idx]) {
        queue.pop_front();
    }
    let pos = queue.iter().position(|&c_idx| {
        !call_matched[c_idx] && names_compatible(calls[c_idx].name, response_name)
    })?;
    queue.remove(pos)
}

fn pop_next_unmatched_call(queue: &mut VecDeque<usize>, call_matched: &[bool]) -> Option<usize> {
    while let Some(c_idx) = queue.pop_front() {
        if !call_matched[c_idx] {
            return Some(c_idx);
        }
    }
    None
}

fn write_canonical_id(
    contents: &mut [Value],
    position: (usize, usize),
    field_key: &str,
    id: String,
) {
    if let Some(func_obj) = contents
        .get_mut(position.0)
        .and_then(Value::as_object_mut)
        .and_then(|msg| msg.get_mut("parts"))
        .and_then(Value::as_array_mut)
        .and_then(|parts| parts.get_mut(position.1))
        .and_then(Value::as_object_mut)
        .and_then(|part| part.get_mut(field_key))
        .and_then(Value::as_object_mut)
    {
        func_obj.insert("id".to_string(), Value::String(id));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::{json, Value};

    use crate::antigravity::{
        build_antigravity_safe_v1internal_request, AntigravityEnvelopeRequestType,
        AntigravityRequestAuth, AntigravityRequestEnvelopeSupport,
    };

    fn sample_auth() -> AntigravityRequestAuth {
        AntigravityRequestAuth {
            project_id: "project-ant-123".to_string(),
            client_version: None,
            session_id: Some("session-ant-456".to_string()),
        }
    }

    fn build_envelope(body: &Value, model: &str) -> Value {
        match build_antigravity_safe_v1internal_request(
            &sample_auth(),
            "req-tool-history-test",
            model,
            body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("request should be supported: {reason:?}")
            }
        }
    }

    fn non_empty_id(value: &Value) -> &str {
        let id = value["id"].as_str().expect("id should be string");
        assert!(!id.trim().is_empty(), "id must be non-empty");
        id
    }

    #[test]
    fn antigravity_claude_repairs_idless_tool_pairs_across_turns_and_envelope_branches() {
        let raw_body = json!({
            "contents": [
                {"role": "user", "parts": [{"text": "check cluster status"}]},
                {
                    "role": "model",
                    "parts": [{
                        "thought": true,
                        "text": "thinking step 1",
                        "thoughtSignature": "sig-turn-1"
                    }, {
                        "functionCall": {"name": "exec_command", "args": {"cmd": "uptime"}},
                        "thoughtSignature": "sig-call-1"
                    }]
                },
                {
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": "exec_command",
                            "response": {"result": "up 10 days"}
                        }
                    }]
                },
                {
                    "role": "model",
                    "parts": [{
                        "function_call": {"name": "read_file", "args": {"path": "/etc/hosts"}}
                    }]
                },
                {
                    "role": "tool",
                    "parts": [{
                        "function_response": {
                            "name": "read_file",
                            "response": {"result": "127.0.0.1 localhost"}
                        }
                    }]
                }
            ]
        });
        let pre_enveloped = json!({
            "project": "caller-project",
            "requestId": "caller-req-id",
            "userAgent": "custom-antigravity-ua/1.0",
            "request": raw_body
        });

        for (wrapped, input) in [(false, &raw_body), (true, &pre_enveloped)] {
            let before = input.clone();
            let envelope = build_envelope(input, "claude-opus-4-6-thinking");
            let contents = envelope["request"]["contents"]
                .as_array()
                .expect("contents array");

            let first_call_id = non_empty_id(&contents[1]["parts"][1]["functionCall"]);
            let first_resp_id = non_empty_id(&contents[2]["parts"][0]["functionResponse"]);
            let second_call_id = non_empty_id(&contents[3]["parts"][0]["function_call"]);
            let second_resp_id = non_empty_id(&contents[4]["parts"][0]["function_response"]);
            assert_eq!(first_resp_id, first_call_id, "wrapped={wrapped}");
            assert_eq!(second_resp_id, second_call_id, "wrapped={wrapped}");
            assert_ne!(first_call_id, second_call_id, "wrapped={wrapped}");

            assert_eq!(input, &before, "caller input must remain immutable");
            let rebuilt_from_same_source = build_envelope(input, "claude-opus-4-6-thinking");
            assert_eq!(
                rebuilt_from_same_source, envelope,
                "same source rebuild must be deterministic"
            );
            let repeated = build_envelope(&envelope, "claude-opus-4-6-thinking");
            assert_eq!(
                repeated, envelope,
                "repeated normalization must be idempotent"
            );
        }
    }

    #[test]
    fn antigravity_claude_pairs_repeated_parallel_and_reordered_tool_calls() {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "lookup", "args": {"q": "first"}}},
                        {"functionCall": {"name": "lookup", "args": {"q": "second"}}},
                        {"functionCall": {"name": "lookup", "args": {"q": "third"}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"slot": 1}}},
                        {"functionResponse": {"name": "lookup", "response": {"slot": 2}}},
                        {"functionResponse": {"name": "lookup", "response": {"slot": 3}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "alpha", "args": {"step": 1}}},
                        {"functionCall": {"name": "beta", "args": {"step": 2}}},
                        {"functionCall": {"name": "gamma", "args": {"step": 3}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "gamma", "response": {"out": "g"}}},
                        {"functionResponse": {"name": "alpha", "response": {"out": "a"}}},
                        {"functionResponse": {"name": "beta", "response": {"out": "b"}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-sonnet-4-5");
        let contents = envelope["request"]["contents"].as_array().unwrap();

        let mut seen_ids = HashSet::new();
        for idx in 0..3 {
            let call_id = non_empty_id(&contents[0]["parts"][idx]["functionCall"]);
            let resp_id = non_empty_id(&contents[1]["parts"][idx]["functionResponse"]);
            assert_eq!(resp_id, call_id, "lookup pair {idx} must share ID");
            assert!(
                seen_ids.insert(call_id),
                "lookup pair {idx} ID must be unique"
            );
        }

        let alpha_id = non_empty_id(&contents[2]["parts"][0]["functionCall"]);
        let beta_id = non_empty_id(&contents[2]["parts"][1]["functionCall"]);
        let gamma_id = non_empty_id(&contents[2]["parts"][2]["functionCall"]);
        assert!(seen_ids.insert(alpha_id));
        assert!(seen_ids.insert(beta_id));
        assert!(seen_ids.insert(gamma_id));

        assert_eq!(contents[3]["parts"][0]["functionResponse"]["id"], gamma_id);
        assert_eq!(contents[3]["parts"][1]["functionResponse"]["id"], alpha_id);
        assert_eq!(contents[3]["parts"][2]["functionResponse"]["id"], beta_id);
    }

    #[test]
    fn antigravity_claude_causal_fifo_pairs_earlier_unanswered_call_before_later_same_name_call() {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "lookup", "args": {"turn": 1, "slot": 1}}},
                        {"functionCall": {"name": "lookup", "args": {"turn": 1, "slot": 2}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"slot": 1}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "lookup", "args": {"turn": 2, "slot": 3}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"slot": 2}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let contents = envelope["request"]["contents"].as_array().unwrap();

        let call_1_id = non_empty_id(&contents[0]["parts"][0]["functionCall"]);
        let call_2_id = non_empty_id(&contents[0]["parts"][1]["functionCall"]);
        let resp_1_id = non_empty_id(&contents[1]["parts"][0]["functionResponse"]);
        let call_3_id = non_empty_id(&contents[2]["parts"][0]["functionCall"]);
        let resp_2_id = non_empty_id(&contents[3]["parts"][0]["functionResponse"]);

        assert_eq!(resp_1_id, call_1_id);
        assert_eq!(
            resp_2_id, call_2_id,
            "later lookup result must pair with earlier unanswered pending lookup call in causal FIFO order"
        );
        assert_ne!(call_1_id, call_2_id);
        assert_ne!(call_3_id, call_1_id);
        assert_ne!(call_3_id, call_2_id);

        let repeated = build_envelope(&envelope, "claude-opus-4-6-thinking");
        assert_eq!(repeated, envelope);
    }

    #[test]
    fn antigravity_claude_preserves_explicit_ids_and_repairs_mixed_one_sided_identities() {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "call_explicit_both", "name": "lookup", "args": {"k": 1}}},
                        {"functionCall": {"id": " call_raw_bytes ", "name": "raw_tool", "args": {"k": 2}}},
                        {"functionCall": {"id": "call_only_explicit", "name": "fetch", "args": {"k": 3}}},
                        {"functionCall": {"name": "compute", "args": {"k": 4}}},
                        {"functionCall": {"name": "compute", "args": {"k": 5}}},
                        {"functionCall": {"name": "dup_res", "args": {"k": 6}}},
                        {"functionCall": {"name": "dup_res", "args": {"k": 7}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"id": "call_explicit_both", "name": "lookup", "response": {"v": 1}}},
                        {"functionResponse": {"name": "raw_tool", "response": {"v": 2}}},
                        {"functionResponse": {"name": "fetch", "response": {"v": 3}}},
                        {"functionResponse": {"id": "result_only_explicit", "name": "compute", "response": {"v": 4}}},
                        {"functionResponse": {"call_id": "result_alias_only", "name": "compute", "response": {"v": 5}}},
                        {"functionResponse": {"id": "result_dup_id", "name": "dup_res", "response": {"v": 6}}},
                        {"functionResponse": {"id": "result_dup_id", "name": "dup_res", "response": {"v": 7}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let contents = envelope["request"]["contents"].as_array().unwrap();

        assert_eq!(
            contents[0]["parts"][0]["functionCall"]["id"],
            "call_explicit_both"
        );
        assert_eq!(
            contents[1]["parts"][0]["functionResponse"]["id"],
            "call_explicit_both"
        );

        assert_eq!(
            contents[0]["parts"][1]["functionCall"]["id"],
            " call_raw_bytes "
        );
        assert_eq!(
            contents[1]["parts"][1]["functionResponse"]["id"],
            " call_raw_bytes "
        );

        assert_eq!(
            contents[0]["parts"][2]["functionCall"]["id"],
            "call_only_explicit"
        );
        assert_eq!(
            contents[1]["parts"][2]["functionResponse"]["id"],
            "call_only_explicit"
        );

        assert_eq!(
            contents[0]["parts"][3]["functionCall"]["id"],
            "result_only_explicit"
        );
        assert_eq!(
            contents[1]["parts"][3]["functionResponse"]["id"],
            "result_only_explicit"
        );

        assert_eq!(
            contents[0]["parts"][4]["functionCall"]["id"],
            "result_alias_only"
        );
        assert_eq!(
            contents[1]["parts"][4]["functionResponse"]["id"],
            "result_alias_only"
        );
        assert_eq!(
            contents[1]["parts"][4]["functionResponse"]["call_id"],
            "result_alias_only"
        );

        assert_eq!(
            contents[0]["parts"][5]["functionCall"]["id"],
            "result_dup_id"
        );
        assert_eq!(
            contents[1]["parts"][5]["functionResponse"]["id"],
            "result_dup_id"
        );
        let second_dup_call_id = non_empty_id(&contents[0]["parts"][6]["functionCall"]);
        assert_ne!(
            second_dup_call_id, "result_dup_id",
            "duplicate result-only explicit ID must not be assigned to two calls"
        );
        assert_eq!(
            contents[1]["parts"][6]["functionResponse"]["id"],
            "result_dup_id"
        );
    }

    #[test]
    fn antigravity_claude_explicit_id_matching_outranks_ambiguous_same_name_fifo() {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "call_first_explicit", "name": "lookup", "args": {"slot": 1}}},
                        {"functionCall": {"name": "lookup", "args": {"slot": 2}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"for_slot": 2}}},
                        {"functionResponse": {"id": "call_first_explicit", "name": "lookup", "response": {"for_slot": 1}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "call_a", "name": "search", "args": {"slot": 3}}},
                        {"functionCall": {"id": "call_b", "name": "search", "args": {"slot": 4}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"id": "call_b", "name": "search", "response": {"for_slot": 4}}},
                        {"functionResponse": {"name": "search", "response": {"for_slot": 3}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let contents = envelope["request"]["contents"].as_array().unwrap();

        assert_eq!(
            contents[0]["parts"][0]["functionCall"]["id"],
            "call_first_explicit"
        );
        let second_lookup_call_id = non_empty_id(&contents[0]["parts"][1]["functionCall"]);
        let first_lookup_resp_id = non_empty_id(&contents[1]["parts"][0]["functionResponse"]);
        assert_eq!(first_lookup_resp_id, second_lookup_call_id);
        assert_ne!(second_lookup_call_id, "call_first_explicit");
        assert_eq!(
            contents[1]["parts"][1]["functionResponse"]["id"],
            "call_first_explicit"
        );

        assert_eq!(contents[2]["parts"][0]["functionCall"]["id"], "call_a");
        assert_eq!(contents[2]["parts"][1]["functionCall"]["id"], "call_b");
        assert_eq!(contents[3]["parts"][0]["functionResponse"]["id"], "call_b");
        assert_eq!(contents[3]["parts"][1]["functionResponse"]["id"], "call_a");
    }

    #[test]
    fn antigravity_claude_pairs_reordered_complementary_one_sided_ids() {
        for reverse_results in [false, true] {
            let mut results = vec![
                json!({"functionResponse": {"id": "call_b", "name": "lookup", "response": {"slot": 2}}}),
                json!({"functionResponse": {"name": "lookup", "response": {"slot": 1}}}),
            ];
            if reverse_results {
                results.reverse();
            }
            let body = json!({"contents": [
                {"role": "model", "parts": [
                    {"functionCall": {"id": "call_a", "name": "lookup", "args": {"slot": 1}}},
                    {"functionCall": {"name": "lookup", "args": {"slot": 2}}},
                ]},
                {"role": "user", "parts": results},
            ]});
            let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
            let contents = &envelope["request"]["contents"];
            assert_eq!(contents[0]["parts"][0]["functionCall"]["id"], "call_a");
            assert_eq!(contents[0]["parts"][1]["functionCall"]["id"], "call_b");
            for part in contents[1]["parts"].as_array().unwrap() {
                let response = &part["functionResponse"];
                let expected = if response["response"]["slot"] == 1 {
                    "call_a"
                } else {
                    "call_b"
                };
                assert_eq!(response["id"], expected);
            }
            assert_eq!(
                build_envelope(&envelope, "claude-opus-4-6-thinking"),
                envelope
            );
        }
    }

    #[test]
    fn antigravity_claude_result_only_ids_do_not_steal_earlier_results() {
        for parallel in [false, true] {
            let first_call = json!({"functionCall": {"name": "lookup", "args": {"slot": 1}}});
            let second_call = json!({"functionCall": {"name": "lookup", "args": {"slot": 2}}});
            let first_result =
                json!({"functionResponse": {"name": "lookup", "response": {"slot": 1}}});
            let second_result = json!({"functionResponse": {"id": "call_b", "name": "lookup", "response": {"slot": 2}}});
            let contents = if parallel {
                json!([
                    {"role": "model", "parts": [first_call, second_call]},
                    {"role": "user", "parts": [first_result, second_result]},
                ])
            } else {
                json!([
                    {"role": "model", "parts": [first_call]},
                    {"role": "user", "parts": [first_result]},
                    {"role": "model", "parts": [second_call]},
                    {"role": "user", "parts": [second_result]},
                ])
            };
            let envelope =
                build_envelope(&json!({"contents": contents}), "claude-opus-4-6-thinking");
            let output = &envelope["request"]["contents"];
            let first_id = non_empty_id(&output[0]["parts"][0]["functionCall"]);
            assert_ne!(first_id, "call_b");
            assert_eq!(output[1]["parts"][0]["functionResponse"]["id"], first_id);
            let (second_call, second_result) = if parallel {
                (
                    &output[0]["parts"][1]["functionCall"],
                    &output[1]["parts"][1]["functionResponse"],
                )
            } else {
                (
                    &output[2]["parts"][0]["functionCall"],
                    &output[3]["parts"][0]["functionResponse"],
                )
            };
            assert_eq!(second_call["id"], "call_b");
            assert_eq!(second_result["id"], "call_b");
        }
    }

    #[test]
    fn antigravity_claude_generated_ids_avoid_collisions_with_explicit_call_and_future_response_ids(
    ) {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "first_generated", "args": {}}},
                        {"functionCall": {"name": "second_generated", "args": {}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "first_generated", "response": {"ok": 1}}},
                        {"functionResponse": {"name": "second_generated", "response": {"ok": 2}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "call_auto_0", "name": "later_explicit_call", "args": {}}},
                        {"functionCall": {"call_id": "call_auto_2", "name": "later_alias_call", "args": {}}},
                        {"functionCall": {"name": "later_result_seeded", "args": {}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"id": "call_auto_0", "name": "later_explicit_call", "response": {}}},
                        {"functionResponse": {"name": "later_alias_call", "response": {}}},
                        {"functionResponse": {"id": "call_auto_1", "name": "later_result_seeded", "response": {}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let contents = envelope["request"]["contents"].as_array().unwrap();

        assert_eq!(contents[0]["parts"][0]["functionCall"]["id"], "call_auto_3");
        assert_eq!(
            contents[1]["parts"][0]["functionResponse"]["id"],
            "call_auto_3"
        );
        assert_eq!(contents[0]["parts"][1]["functionCall"]["id"], "call_auto_4");
        assert_eq!(
            contents[1]["parts"][1]["functionResponse"]["id"],
            "call_auto_4"
        );

        assert_eq!(contents[2]["parts"][0]["functionCall"]["id"], "call_auto_0");
        assert_eq!(contents[2]["parts"][1]["functionCall"]["id"], "call_auto_2");
        assert_eq!(
            contents[3]["parts"][1]["functionResponse"]["id"],
            "call_auto_2"
        );
        assert_eq!(contents[2]["parts"][2]["functionCall"]["id"], "call_auto_1");
        assert_eq!(
            contents[3]["parts"][2]["functionResponse"]["id"],
            "call_auto_1"
        );
    }

    #[test]
    fn antigravity_claude_repairs_empty_whitespace_and_null_ids_and_promotes_aliases() {
        let body = json!({
            "contents": [
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "", "name": "empty_id", "args": {}}},
                        {"functionCall": {"id": "   \t\n ", "name": "blank_id", "args": {}}},
                        {"functionCall": {"id": null, "name": "null_id", "args": {}}},
                        {"functionCall": {"id": "", "call_id": "snake_alias_1", "callId": "camel_ignored", "name": "alias_snake", "args": {}}},
                        {"functionCall": {"callId": "camel_alias_2", "name": "alias_camel", "args": {}}},
                        {"functionCall": {"id": "canonical_wins", "call_id": "snake_kept", "name": "both_ids", "args": {}}},
                        {"functionCall": {"id": 999, "name": "malformed_id", "args": {}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"id": "", "name": "empty_id", "response": {}}},
                        {"functionResponse": {"id": "  ", "name": "blank_id", "response": {}}},
                        {"functionResponse": {"id": null, "name": "null_id", "response": {}}},
                        {"functionResponse": {"name": "alias_snake", "response": {}}},
                        {"functionResponse": {"id": null, "callId": "camel_alias_2", "name": "alias_camel", "response": {}}},
                        {"functionResponse": {"call_id": "canonical_wins", "name": "both_ids", "response": {}}},
                        {"functionResponse": {"id": 999, "name": "malformed_id", "response": {}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let calls = &envelope["request"]["contents"][0]["parts"];
        let results = &envelope["request"]["contents"][1]["parts"];

        let mut generated_ids = HashSet::new();
        for idx in 0..3 {
            let call_id = non_empty_id(&calls[idx]["functionCall"]);
            let res_id = non_empty_id(&results[idx]["functionResponse"]);
            assert_eq!(res_id, call_id);
            assert!(generated_ids.insert(call_id));
        }
        assert_eq!(calls[3]["functionCall"]["id"], "snake_alias_1");
        assert_eq!(calls[3]["functionCall"]["call_id"], "snake_alias_1");
        assert_eq!(calls[3]["functionCall"]["callId"], "camel_ignored");
        assert_eq!(results[3]["functionResponse"]["id"], "snake_alias_1");

        assert_eq!(calls[4]["functionCall"]["id"], "camel_alias_2");
        assert_eq!(calls[4]["functionCall"]["callId"], "camel_alias_2");
        assert_eq!(results[4]["functionResponse"]["id"], "camel_alias_2");
        assert_eq!(results[4]["functionResponse"]["callId"], "camel_alias_2");

        assert_eq!(calls[5]["functionCall"]["id"], "canonical_wins");
        assert_eq!(calls[5]["functionCall"]["call_id"], "snake_kept");
        assert_eq!(results[5]["functionResponse"]["id"], "canonical_wins");
        assert_eq!(results[5]["functionResponse"]["call_id"], "canonical_wins");

        assert_eq!(calls[6]["functionCall"]["id"], 999);
        assert_eq!(results[6]["functionResponse"]["id"], 999);
    }

    #[test]
    fn antigravity_claude_does_not_rewire_orphans_or_explicit_mismatches() {
        let body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"orphan": "before_any_call"}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"id": "call_explicit_a", "name": "lookup", "args": {"turn": 1}}},
                        {"functionCall": {"name": "unanswered_tool", "args": {"turn": 1}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"id": "call_explicit_b", "name": "lookup", "response": {"mismatch": true}}},
                        {"functionResponse": {"name": "wrong_tool_name", "response": {"orphan": "wrong_name"}}}
                    ]
                },
                {
                    "role": "model",
                    "parts": [
                        {"functionCall": {"name": "lookup", "args": {"turn": 2}}}
                    ]
                },
                {
                    "role": "user",
                    "parts": [
                        {"functionResponse": {"name": "lookup", "response": {"turn": 2}}}
                    ]
                }
            ]
        });

        let envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let contents = envelope["request"]["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 5, "must not add or remove messages");

        assert!(
            contents[0]["parts"][0]["functionResponse"]
                .get("id")
                .is_none(),
            "orphan result before any call must not be wired to future calls"
        );
        assert_eq!(
            contents[1]["parts"][0]["functionCall"]["id"], "call_explicit_a",
            "explicit call ID in mismatch must be preserved"
        );
        let orphan_call_id = non_empty_id(&contents[1]["parts"][1]["functionCall"]);
        assert_ne!(orphan_call_id, "call_explicit_a");
        assert_ne!(orphan_call_id, "call_explicit_b");
        assert_eq!(
            contents[2]["parts"][0]["functionResponse"]["id"], "call_explicit_b",
            "explicit response ID in mismatch must be preserved"
        );
        assert!(
            contents[2]["parts"][1]["functionResponse"]
                .get("id")
                .is_none(),
            "unmatched tool-name result must not be wired to unrelated call"
        );
        let turn2_call_id = non_empty_id(&contents[3]["parts"][0]["functionCall"]);
        let turn2_resp_id = non_empty_id(&contents[4]["parts"][0]["functionResponse"]);
        assert_eq!(
            turn2_resp_id, turn2_call_id,
            "later lookup pair must not inherit earlier mismatched call_explicit_a"
        );
        assert_ne!(turn2_call_id, "call_explicit_a");
        assert_ne!(turn2_call_id, "call_explicit_b");
        assert_ne!(turn2_call_id, orphan_call_id);

        let repeated = build_envelope(&envelope, "claude-opus-4-6-thinking");
        assert_eq!(
            repeated, envelope,
            "orphan/mismatch history must remain idempotent"
        );
    }

    #[test]
    fn antigravity_claude_preserves_payloads_signatures_business_fields_and_non_claude_scope() {
        let body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [{
                        "text": "user message with non-protocol functionCall",
                        "functionCall": {"name": "not_a_model_call", "args": {}}
                    }]
                },
                {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "complex_tool",
                            "args": {
                                "id": "",
                                "functionCall": {"name": "nested_business_call"},
                                "functionResponse": {"name": "nested_business_response"}
                            },
                            "customCallField": "preserve-me"
                        },
                        "thoughtSignature": "sig-keep-exact-bytes",
                        "extraPartField": {"nested": [1, 2, 3]}
                    }, {
                        "functionResponse": {"name": "not_a_user_response", "response": {}}
                    }],
                    "extraMessageField": "keep-msg-meta"
                },
                {
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": "complex_tool",
                            "response": {
                                "result": {
                                    "id": null,
                                    "functionCall": {"name": "inner_call"},
                                    "functionResponse": {"name": "inner_resp"}
                                }
                            },
                            "customResponseField": 42
                        }
                    }]
                }
            ]
        });

        let original = body.clone();
        let claude_envelope = build_envelope(&body, "claude-opus-4-6-thinking");
        let claude_contents = &claude_envelope["request"]["contents"];

        assert!(
            claude_contents[0]["parts"][0]["functionCall"]
                .get("id")
                .is_none(),
            "user-role functionCall is not a protocol model call"
        );
        assert!(
            claude_contents[1]["parts"][1]["functionResponse"]
                .get("id")
                .is_none(),
            "model-role functionResponse is not a protocol tool result"
        );

        let call_id = non_empty_id(&claude_contents[1]["parts"][0]["functionCall"]);
        let resp_id = non_empty_id(&claude_contents[2]["parts"][0]["functionResponse"]);
        assert_eq!(resp_id, call_id);
        assert_eq!(
            claude_contents[1]["parts"][0]["functionCall"]["args"],
            original["contents"][1]["parts"][0]["functionCall"]["args"]
        );
        assert_eq!(
            claude_contents[1]["parts"][0]["functionCall"]["customCallField"],
            "preserve-me"
        );
        assert_eq!(
            claude_contents[1]["parts"][0]["thoughtSignature"],
            "sig-keep-exact-bytes"
        );
        assert_eq!(
            claude_contents[1]["parts"][0]["extraPartField"],
            json!({"nested": [1, 2, 3]})
        );
        assert_eq!(claude_contents[1]["extraMessageField"], "keep-msg-meta");
        assert_eq!(
            claude_contents[2]["parts"][0]["functionResponse"]["response"],
            original["contents"][2]["parts"][0]["functionResponse"]["response"]
        );
        assert_eq!(
            claude_contents[2]["parts"][0]["functionResponse"]["customResponseField"],
            42
        );
        assert_eq!(body, original, "caller input must remain unchanged");

        for non_claude_model in ["gemini-3.5-pro", "gemini-2.5-flash", "gpt-5"] {
            let non_claude_envelope = build_envelope(&body, non_claude_model);
            assert_eq!(
                non_claude_envelope["request"]["contents"], original["contents"],
                "non-Claude target {non_claude_model} must not mutate tool history"
            );
        }
    }
}
