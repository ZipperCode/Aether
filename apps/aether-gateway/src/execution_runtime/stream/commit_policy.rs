use std::time::Duration;

use serde_json::Value;

use crate::execution_runtime::MAX_STREAM_PREFETCH_BYTES;

/// Anthropic 首个语义事件提交前的最长等待时间。
const ANTHROPIC_PRECOMMIT_MAX_WAIT: Duration = Duration::from_millis(750);
/// Gemini 首个语义事件或终止错误提交前的最长等待时间。
const GEMINI_PRECOMMIT_MAX_WAIT: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 决定何时可向下游提交 HTTP 成功状态的流策略。
pub(super) enum StreamCommitPolicy {
    /// 收到响应头即可提交，适用于无需检查首段的直通流。
    ResponseHeaders,
    /// 先分类首个完整正文或事件，保留同格式 Responses 错误回退边界。
    FirstClassifiedBody,
    /// 等待 Anthropic 首个客户端可见语义事件或错误。
    FirstAnthropicSemanticEvent {
        /// 提交前最多缓存的上游字节数，达到上限后为防止无界缓冲而提交。
        max_bytes: usize,
        /// 提交前最长等待时间。
        max_wait: Duration,
    },
    /// 等待 Gemini 首个客户端可见语义事件，并在提交前识别畸形工具调用。
    FirstGeminiSemanticEvent {
        /// 提交前最多缓存的上游字节数，达到上限后为防止无界缓冲而提交。
        max_bytes: usize,
        /// 提交前最长等待时间。
        max_wait: Duration,
    },
}

impl StreamCommitPolicy {
    /// 根据上下游格式与响应类型选择提交时机；同格式 Responses SSE 必须先分类首段，避免把流内错误提交成 HTTP 200。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn for_response(
        has_direct_finalize: bool,
        content_type: Option<&str>,
        provider_api_format: &str,
        client_api_format: &str,
        has_private_stream_normalizer: bool,
        has_local_stream_rewriter: bool,
        force_prefetch: bool,
    ) -> Self {
        if !has_direct_finalize {
            return Self::FirstClassifiedBody;
        }

        if force_prefetch {
            return Self::FirstClassifiedBody;
        }

        let content_type = content_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if content_type.contains("text/event-stream") {
            if provider_api_format.eq_ignore_ascii_case("openai:responses")
                && provider_api_format.eq_ignore_ascii_case(client_api_format)
            {
                return Self::FirstClassifiedBody;
            }
            if provider_api_format.eq_ignore_ascii_case("claude:messages")
                && provider_api_format.eq_ignore_ascii_case(client_api_format)
                && !has_private_stream_normalizer
                && !has_local_stream_rewriter
            {
                return Self::FirstAnthropicSemanticEvent {
                    max_bytes: MAX_STREAM_PREFETCH_BYTES,
                    max_wait: ANTHROPIC_PRECOMMIT_MAX_WAIT,
                };
            }
            if provider_api_format.eq_ignore_ascii_case("gemini:generate_content") {
                return Self::FirstGeminiSemanticEvent {
                    max_bytes: MAX_STREAM_PREFETCH_BYTES,
                    max_wait: GEMINI_PRECOMMIT_MAX_WAIT,
                };
            }
            return Self::ResponseHeaders;
        }

        if has_private_stream_normalizer || has_local_stream_rewriter {
            return Self::FirstClassifiedBody;
        }

        if !provider_api_format.eq_ignore_ascii_case(client_api_format) {
            return Self::FirstClassifiedBody;
        }

        if content_type.is_empty() {
            return Self::ResponseHeaders;
        }

        if content_type.contains("json") || content_type.ends_with("+json") {
            Self::FirstClassifiedBody
        } else {
            Self::ResponseHeaders
        }
    }

    /// 返回该策略是否允许仅凭响应头提交成功状态。
    pub(super) const fn commits_on_response_headers(self) -> bool {
        matches!(self, Self::ResponseHeaders)
    }

    /// 返回策略是否需要有界等待完整 SSE 语义记录。
    pub(super) const fn requires_bounded_frame_wait(self) -> bool {
        matches!(
            self,
            Self::FirstAnthropicSemanticEvent { .. } | Self::FirstGeminiSemanticEvent { .. }
        )
    }

    /// 返回语义预提交门的最长等待时间；非语义门没有额外等待。
    pub(super) const fn max_precommit_wait(self) -> Option<Duration> {
        match self {
            Self::FirstAnthropicSemanticEvent { max_wait, .. }
            | Self::FirstGeminiSemanticEvent { max_wait, .. } => Some(max_wait),
            Self::ResponseHeaders | Self::FirstClassifiedBody => None,
        }
    }

    /// 判断当前策略是否为原生 Anthropic 语义门。
    pub(super) const fn is_native_anthropic(self) -> bool {
        matches!(self, Self::FirstAnthropicSemanticEvent { .. })
    }

    /// 判断当前策略是否为 Gemini 语义门，供通用正文检查避免重复消费。
    pub(super) const fn is_gemini(self) -> bool {
        matches!(self, Self::FirstGeminiSemanticEvent { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// 流提交门的生命周期状态；终止错误后不可重新提交。
pub(super) enum StreamCommitState {
    /// 尚未向客户端承诺成功状态。
    Uncommitted,
    /// 已出现客户端可见语义，后续错误只能留在当前流内。
    Committed,
    /// 提交前已识别上游错误，当前候选终止。
    Terminal,
}

#[derive(Debug, PartialEq)]
/// 每次观察上游字节后给执行器的预提交决策。
pub(super) enum StreamPrecommitObservation {
    /// 仍需更多字节才能判断。
    Pending,
    /// 已达到可见语义或安全上限，可以提交。
    Commit,
    /// 提交前识别到上游错误，并携带应使用的状态与结构化正文。
    UpstreamError {
        /// 应返回或用于候选重试分类的上游等效 HTTP 状态。
        status_code: u16,
        /// 不含凭据的结构化错误正文。
        body_json: Value,
    },
}

#[derive(Debug)]
/// 按选定策略累计并分类上游流，统一维护提交状态。
pub(super) struct StreamCommitGate {
    /// 本响应选定的提交策略。
    policy: StreamCommitPolicy,
    /// 当前不可逆提交状态。
    state: StreamCommitState,
    /// 提交前累计观察字节数，用于有界缓冲。
    observed_bytes: usize,
    /// Anthropic SSE 增量检查器。
    anthropic: AnthropicSsePrecommitInspector,
    /// Gemini SSE 增量检查器。
    gemini: GeminiSsePrecommitInspector,
}

impl StreamCommitGate {
    /// 按策略初始化提交门；响应头策略从一开始即视为已提交。
    pub(super) fn new(policy: StreamCommitPolicy) -> Self {
        let state = if policy.commits_on_response_headers() {
            StreamCommitState::Committed
        } else {
            StreamCommitState::Uncommitted
        };
        Self {
            policy,
            state,
            observed_bytes: 0,
            anthropic: AnthropicSsePrecommitInspector::default(),
            gemini: GeminiSsePrecommitInspector::default(),
        }
    }

    /// 返回当前提交状态。
    pub(super) const fn state(&self) -> StreamCommitState {
        self.state
    }

    /// 判断是否仍可在不污染客户端输出的情况下切换候选。
    pub(super) const fn is_uncommitted(&self) -> bool {
        matches!(self.state, StreamCommitState::Uncommitted)
    }

    /// 增量观察一段上游字节，并把 Provider 专用分类映射为统一提交决策。
    pub(super) fn observe_provider_bytes(&mut self, chunk: &[u8]) -> StreamPrecommitObservation {
        if self.state != StreamCommitState::Uncommitted {
            return StreamPrecommitObservation::Commit;
        }

        let (max_bytes, observation) = match self.policy {
            StreamCommitPolicy::FirstAnthropicSemanticEvent { max_bytes, .. } => {
                (max_bytes, self.anthropic.observe(chunk, max_bytes))
            }
            StreamCommitPolicy::FirstGeminiSemanticEvent { max_bytes, .. } => {
                (max_bytes, self.gemini.observe(chunk, max_bytes))
            }
            StreamCommitPolicy::ResponseHeaders | StreamCommitPolicy::FirstClassifiedBody => {
                return StreamPrecommitObservation::Pending;
            }
        };

        self.observed_bytes = self.observed_bytes.saturating_add(chunk.len());
        match observation {
            SemanticSseObservation::Pending => {}
            SemanticSseObservation::SemanticEvent => {
                self.state = StreamCommitState::Committed;
                return StreamPrecommitObservation::Commit;
            }
            SemanticSseObservation::Error {
                status_code,
                body_json,
            } => {
                self.state = StreamCommitState::Terminal;
                return StreamPrecommitObservation::UpstreamError {
                    status_code,
                    body_json,
                };
            }
        }

        if self.observed_bytes >= max_bytes {
            self.commit();
            StreamPrecommitObservation::Commit
        } else {
            StreamPrecommitObservation::Pending
        }
    }

    /// 显式提交尚未终止的流；终止状态保持不可逆。
    pub(super) fn commit(&mut self) {
        if self.state == StreamCommitState::Uncommitted {
            self.state = StreamCommitState::Committed;
        }
    }
}

#[derive(Debug)]
/// Provider 专用 SSE 检查器返回的内部语义分类。
enum SemanticSseObservation {
    /// 当前完整记录没有客户端可见语义。
    Pending,
    /// 已出现应提交给客户端的语义内容。
    SemanticEvent,
    /// 记录表示提交前上游错误。
    Error {
        /// Provider 事件映射后的 HTTP 状态。
        status_code: u16,
        /// 供统一错误处理路径消费的结构化正文。
        body_json: Value,
    },
}

#[derive(Debug, Default)]
/// Anthropic SSE 的跨 chunk 有界记录缓冲器。
struct AnthropicSsePrecommitInspector {
    /// 尚未形成完整 SSE 记录的字节。
    buffered: Vec<u8>,
}

impl AnthropicSsePrecommitInspector {
    /// 追加字节并逐条分类完整 Anthropic SSE 记录；超限时按可见语义提交。
    fn observe(&mut self, chunk: &[u8], max_bytes: usize) -> SemanticSseObservation {
        let remaining = max_bytes.saturating_sub(self.buffered.len());
        let truncated = chunk.len() > remaining;
        self.buffered
            .extend_from_slice(&chunk[..chunk.len().min(remaining)]);

        while let Some((record_end, separator_len)) = find_sse_record_boundary(&self.buffered) {
            let record = self.buffered[..record_end].to_vec();
            self.buffered.drain(..record_end + separator_len);
            match classify_anthropic_sse_record(&record) {
                SemanticSseObservation::Pending => {}
                decision => return decision,
            }
        }

        if truncated {
            SemanticSseObservation::SemanticEvent
        } else {
            SemanticSseObservation::Pending
        }
    }
}

#[derive(Debug, Default)]
/// Gemini SSE 的跨 chunk 有界记录缓冲器。
struct GeminiSsePrecommitInspector {
    /// 尚未形成完整 SSE 记录的字节。
    buffered: Vec<u8>,
}

impl GeminiSsePrecommitInspector {
    /// 追加字节并逐条分类完整 Gemini SSE 记录；超限时按可见语义提交。
    fn observe(&mut self, chunk: &[u8], max_bytes: usize) -> SemanticSseObservation {
        let remaining = max_bytes.saturating_sub(self.buffered.len());
        let truncated = chunk.len() > remaining;
        self.buffered
            .extend_from_slice(&chunk[..chunk.len().min(remaining)]);

        while let Some((record_end, separator_len)) = find_sse_record_boundary(&self.buffered) {
            let record = self.buffered[..record_end].to_vec();
            self.buffered.drain(..record_end + separator_len);
            match classify_gemini_sse_record(&record) {
                SemanticSseObservation::Pending => {}
                decision => return decision,
            }
        }

        if truncated {
            SemanticSseObservation::SemanticEvent
        } else {
            SemanticSseObservation::Pending
        }
    }
}

/// 查找同时兼容 CRLF、LF 与 CR 的首个空行记录边界。
pub(super) fn find_sse_record_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut cursor = 0;
    while cursor < buffer.len() {
        let (line_end, line_ending_len) = next_sse_line_ending(buffer, cursor)?;
        let next_line_start = line_end + line_ending_len;
        let Some((next_line_end, next_line_ending_len)) =
            next_sse_line_ending(buffer, next_line_start)
        else {
            return None;
        };
        if next_line_end == next_line_start {
            return Some((
                line_end,
                line_ending_len.saturating_add(next_line_ending_len),
            ));
        }
        cursor = next_line_start;
    }
    None
}

/// 从指定偏移查找下一行结尾，并返回起点与实际结尾字节数。
fn next_sse_line_ending(buffer: &[u8], start: usize) -> Option<(usize, usize)> {
    let relative = buffer
        .get(start..)?
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))?;
    let index = start + relative;
    let ending_len = if buffer[index] == b'\r' && buffer.get(index + 1) == Some(&b'\n') {
        2
    } else {
        1
    };
    Some((index, ending_len))
}

/// 分类一个完整 Anthropic SSE 记录；未知、注释或畸形记录继续等待。
fn classify_anthropic_sse_record(record: &[u8]) -> SemanticSseObservation {
    let Ok(record) = std::str::from_utf8(record) else {
        return SemanticSseObservation::Pending;
    };
    let normalized_record = record.replace("\r\n", "\n").replace('\r', "\n");
    let mut event_type = None;
    let mut data = String::new();
    for line in normalized_record.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            let value = value.trim();
            if !value.is_empty() {
                event_type = Some(value);
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.trim_start());
        }
    }
    if data.trim().is_empty() {
        return SemanticSseObservation::Pending;
    }

    let Ok(body_json) = serde_json::from_str::<Value>(data.trim()) else {
        return SemanticSseObservation::Pending;
    };
    let payload_type = body_json.get("type").and_then(Value::as_str).map(str::trim);
    if event_type == Some("error") || payload_type == Some("error") {
        return SemanticSseObservation::Error {
            status_code: anthropic_error_status_code(&body_json),
            body_json,
        };
    }

    let semantic_type = match (event_type, payload_type) {
        (Some(event_type), Some(payload_type)) if event_type == payload_type => Some(event_type),
        (None, Some(payload_type)) => Some(payload_type),
        _ => None,
    };
    if semantic_type.is_some_and(is_anthropic_semantic_event_type) {
        SemanticSseObservation::SemanticEvent
    } else {
        SemanticSseObservation::Pending
    }
}

/// 分类一个完整 Gemini SSE 记录，在可见输出前把已知畸形终止原因转换为 502。
fn classify_gemini_sse_record(record: &[u8]) -> SemanticSseObservation {
    let Ok(record) = std::str::from_utf8(record) else {
        return SemanticSseObservation::Pending;
    };
    let data = record
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    if data.trim().is_empty() {
        return SemanticSseObservation::Pending;
    }
    if data.trim() == "[DONE]" {
        return SemanticSseObservation::SemanticEvent;
    }

    let Ok(body_json) = serde_json::from_str::<Value>(data.trim()) else {
        return SemanticSseObservation::Pending;
    };
    let response = body_json.get("response").unwrap_or(&body_json);
    let Some(candidates) = response.get("candidates").and_then(Value::as_array) else {
        return SemanticSseObservation::Pending;
    };

    for candidate in candidates {
        let finish_reason = candidate
            .get("finishReason")
            .or_else(|| candidate.get("finish_reason"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(finish_reason) = finish_reason.filter(|reason| {
            matches!(
                *reason,
                "MALFORMED_FUNCTION_CALL"
                    | "UNEXPECTED_TOOL_CALL"
                    | "TOO_MANY_TOOL_CALLS"
                    | "MISSING_THOUGHT_SIGNATURE"
                    | "MALFORMED_RESPONSE"
            )
        }) {
            let message = candidate
                .get("finishMessage")
                .or_else(|| candidate.get("finish_message"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("Gemini stream ended with {finish_reason}"));
            return SemanticSseObservation::Error {
                status_code: 502,
                body_json: serde_json::json!({
                    "error": {
                        "type": "upstream_gemini_finish_error",
                        "code": finish_reason,
                        "message": message,
                        "upstream_status": 200
                    }
                }),
            };
        }

        if finish_reason.is_some() {
            return SemanticSseObservation::SemanticEvent;
        }
        let Some(parts) = candidate
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        if parts.iter().any(gemini_part_is_client_semantic) {
            return SemanticSseObservation::SemanticEvent;
        }
    }

    SemanticSseObservation::Pending
}

/// 判断 Gemini part 是否会产生客户端可见内容；纯 thought/signature 不触发提交。
fn gemini_part_is_client_semantic(part: &Value) -> bool {
    let Some(part) = part.as_object() else {
        return true;
    };
    if part
        .keys()
        .any(|key| !matches!(key.as_str(), "text" | "thought" | "thoughtSignature"))
    {
        return true;
    }
    if part.get("thought").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if part.keys().all(|key| key == "thoughtSignature") {
        return false;
    }
    if part
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
    {
        return true;
    }
    false
}

/// 判断 Anthropic 事件类型是否已产生可向客户端承诺的语义边界。
fn is_anthropic_semantic_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "message_start"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "message_delta"
            | "message_stop"
    )
}

/// 将 Anthropic 结构化错误类型映射为 HTTP 状态，未知错误保守映射为 500。
pub(super) fn anthropic_error_status_code(body_json: &Value) -> u16 {
    let error_type = body_json
        .get("error")
        .and_then(|error| error.get("type"))
        .or_else(|| body_json.get("type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match error_type {
        "invalid_request_error" => 400,
        "authentication_error" => 401,
        "permission_error" => 403,
        "not_found_error" => 404,
        "request_too_large" => 413,
        "rate_limit_error" => 429,
        "overloaded_error" => 529,
        _ => 500,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        anthropic_error_status_code, StreamCommitGate, StreamCommitPolicy, StreamCommitState,
        StreamPrecommitObservation,
    };

    /// 构造测试用原生 Anthropic 有界提交策略。
    fn native_anthropic_policy() -> StreamCommitPolicy {
        StreamCommitPolicy::FirstAnthropicSemanticEvent {
            max_bytes: 16_384,
            max_wait: Duration::from_millis(750),
        }
    }

    /// 构造测试用 Gemini 有界提交策略。
    fn gemini_policy() -> StreamCommitPolicy {
        StreamCommitPolicy::FirstGeminiSemanticEvent {
            max_bytes: 16_384,
            max_wait: Duration::from_millis(750),
        }
    }

    /// 验证只有原生同格式且无改写器的 Anthropic SSE 使用语义门。
    #[test]
    fn policy_selects_bounded_anthropic_gate_only_for_native_same_format_sse() {
        let native = StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream; charset=utf-8"),
            "claude:messages",
            "claude:messages",
            false,
            false,
            false,
        );
        assert!(native.is_native_anthropic());
        assert_eq!(
            native.max_precommit_wait(),
            Some(Duration::from_millis(750))
        );
        assert!(StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream"),
            "openai:chat",
            "claude:messages",
            false,
            false,
            false,
        )
        .commits_on_response_headers());
        assert!(StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream"),
            "claude:messages",
            "claude:messages",
            false,
            true,
            false,
        )
        .commits_on_response_headers());
    }

    /// 验证同格式 Responses SSE 会等待首段分类，同时不改变 Chat Completions 的响应头提交策略。
    #[test]
    fn policy_prefetches_same_format_openai_responses_sse_only() {
        let responses = StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream; charset=utf-8"),
            "openai:responses",
            "openai:responses",
            false,
            false,
            false,
        );
        assert_eq!(responses, StreamCommitPolicy::FirstClassifiedBody);
        assert!(!responses.commits_on_response_headers());

        assert!(StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream"),
            "openai:chat",
            "openai:chat",
            false,
            false,
            false,
        )
        .commits_on_response_headers());
    }

    /// 验证 Gemini SSE 即使存在本地改写器也先经过有界语义门。
    #[test]
    fn policy_selects_bounded_gemini_gate_for_event_streams() {
        let policy = StreamCommitPolicy::for_response(
            true,
            Some("text/event-stream"),
            "gemini:generate_content",
            "openai:responses",
            false,
            true,
            false,
        );

        assert!(policy.is_gemini());
        assert!(policy.requires_bounded_frame_wait());
        assert_eq!(
            policy.max_precommit_wait(),
            Some(Duration::from_millis(750))
        );
    }

    /// 验证纯思考帧保持等待，而首个非空文本触发提交。
    #[test]
    fn gemini_gate_waits_through_thought_and_commits_on_text() {
        let mut gate = StreamCommitGate::new(gemini_policy());
        let thought = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thought\":true,\"text\":\"checking\"}]}}]}}\n\n";
        let text = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"answer\"}]}}]}}\n\n";

        assert_eq!(
            gate.observe_provider_bytes(thought),
            StreamPrecommitObservation::Pending
        );
        assert_eq!(
            gate.observe_provider_bytes(text),
            StreamPrecommitObservation::Commit
        );
        assert_eq!(gate.state(), StreamCommitState::Committed);
    }

    /// 验证函数调用即使带 thought 标记也属于客户端可见语义。
    #[test]
    fn gemini_gate_commits_on_function_call_even_with_thought_marker() {
        let mut gate = StreamCommitGate::new(gemini_policy());
        let tool_call = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thought\":true,\"functionCall\":{\"name\":\"validate\",\"args\":{}}}]}}]}}\n\n";

        assert_eq!(
            gate.observe_provider_bytes(tool_call),
            StreamPrecommitObservation::Commit
        );
        assert_eq!(gate.state(), StreamCommitState::Committed);
    }

    /// 验证畸形函数调用在成功状态提交前转换为结构化 502。
    #[test]
    fn gemini_gate_rejects_malformed_function_call_before_commit() {
        let mut gate = StreamCommitGate::new(gemini_policy());
        let thought = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thought\":true,\"text\":\"calling\"}]}}]}}\n\n";
        let malformed = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thoughtSignature\":\"signature\",\"text\":\"\"}]},\"finishReason\":\"MALFORMED_FUNCTION_CALL\",\"finishMessage\":\"Malformed function call: Function call is empty - no input to parse.\"}]}}\n\n";

        assert_eq!(
            gate.observe_provider_bytes(thought),
            StreamPrecommitObservation::Pending
        );
        let StreamPrecommitObservation::UpstreamError {
            status_code,
            body_json,
        } = gate.observe_provider_bytes(malformed)
        else {
            panic!("malformed Gemini function call should fail before stream commit");
        };

        assert_eq!(status_code, 502);
        assert_eq!(body_json["error"]["code"], "MALFORMED_FUNCTION_CALL");
        assert_eq!(
            body_json["error"]["message"],
            "Malformed function call: Function call is empty - no input to parse."
        );
        assert_eq!(gate.state(), StreamCommitState::Terminal);
    }

    /// 验证畸形 Gemini SSE 在任意 chunk 边界切分时都能被识别。
    #[test]
    fn gemini_gate_detects_malformed_function_call_across_chunk_boundaries() {
        let malformed = b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thoughtSignature\":\"signature\",\"text\":\"\"}]},\"finishReason\":\"MALFORMED_FUNCTION_CALL\",\"finishMessage\":\"empty call\"}]}}\r\n\r\n";

        for split in 1..malformed.len() {
            let mut gate = StreamCommitGate::new(gemini_policy());
            let first_observation = gate.observe_provider_bytes(&malformed[..split]);
            if !matches!(
                first_observation,
                StreamPrecommitObservation::UpstreamError {
                    status_code: 502,
                    ..
                }
            ) {
                assert_eq!(first_observation, StreamPrecommitObservation::Pending);
                assert!(matches!(
                    gate.observe_provider_bytes(&malformed[split..]),
                    StreamPrecommitObservation::UpstreamError {
                        status_code: 502,
                        ..
                    }
                ));
            }
            assert_eq!(gate.state(), StreamCommitState::Terminal);
        }
    }

    #[test]
    fn gate_detects_anthropic_error_across_every_chunk_boundary() {
        let event = b"event: error\r\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\r\n\r\n";
        for split in 1..event.len() {
            let mut gate = StreamCommitGate::new(native_anthropic_policy());
            let first_observation = gate.observe_provider_bytes(&event[..split]);
            if matches!(
                first_observation,
                StreamPrecommitObservation::UpstreamError {
                    status_code: 529,
                    ..
                }
            ) {
                assert_eq!(event[split - 1], b'\r');
            } else {
                assert_eq!(first_observation, StreamPrecommitObservation::Pending);
                assert!(matches!(
                    gate.observe_provider_bytes(&event[split..]),
                    StreamPrecommitObservation::UpstreamError {
                        status_code: 529,
                        ..
                    }
                ));
            }
            assert_eq!(gate.state(), StreamCommitState::Terminal);
        }
    }

    #[test]
    fn gate_detects_cr_only_and_mixed_line_ending_errors() {
        for event in [
            "event: error\rdata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\r\r",
            "event: error\r\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\r",
        ] {
            for split in 1..event.len() {
                let mut gate = StreamCommitGate::new(native_anthropic_policy());
                assert_eq!(
                    gate.observe_provider_bytes(&event.as_bytes()[..split]),
                    StreamPrecommitObservation::Pending,
                    "gate committed before complete mixed-line event at split {split}",
                );
                assert!(matches!(
                    gate.observe_provider_bytes(&event.as_bytes()[split..]),
                    StreamPrecommitObservation::UpstreamError {
                        status_code: 529,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn unknown_and_ping_events_do_not_commit_before_anthropic_error() {
        let mut gate = StreamCommitGate::new(native_anthropic_policy());
        assert_eq!(
            gate.observe_provider_bytes(
                b"event: future_event\ndata: {\"type\":\"future_event\",\"value\":1}\n\n"
            ),
            StreamPrecommitObservation::Pending
        );
        assert_eq!(
            gate.observe_provider_bytes(b"event: ping\ndata: {\"type\":\"ping\"}\n\n"),
            StreamPrecommitObservation::Pending
        );
        assert!(matches!(
            gate.observe_provider_bytes(
                b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\"}}\n\n"
            ),
            StreamPrecommitObservation::UpstreamError {
                status_code: 429,
                ..
            }
        ));
    }

    #[test]
    fn first_semantic_event_commits_before_later_error_in_same_chunk() {
        let mut gate = StreamCommitGate::new(native_anthropic_policy());
        let observation = gate.observe_provider_bytes(
            concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{}}\n\n",
                "event: error\n",
                "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n",
            )
            .as_bytes(),
        );

        assert_eq!(observation, StreamPrecommitObservation::Commit);
        assert_eq!(gate.state(), StreamCommitState::Committed);
    }

    #[test]
    fn transport_fragment_count_does_not_commit_an_incomplete_anthropic_error() {
        let policy = StreamCommitPolicy::FirstAnthropicSemanticEvent {
            max_bytes: 1024,
            max_wait: Duration::from_millis(750),
        };
        let mut gate = StreamCommitGate::new(policy);
        let event = b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n";
        for byte in &event[..event.len() - 1] {
            assert_eq!(
                gate.observe_provider_bytes(std::slice::from_ref(byte)),
                StreamPrecommitObservation::Pending,
            );
        }
        assert!(matches!(
            gate.observe_provider_bytes(&event[event.len() - 1..]),
            StreamPrecommitObservation::UpstreamError {
                status_code: 529,
                ..
            }
        ));
    }

    #[test]
    fn anthropic_error_status_mapping_matches_messages_api_taxonomy() {
        for (error_type, status_code) in [
            ("invalid_request_error", 400),
            ("authentication_error", 401),
            ("permission_error", 403),
            ("not_found_error", 404),
            ("request_too_large", 413),
            ("rate_limit_error", 429),
            ("overloaded_error", 529),
            ("api_error", 500),
        ] {
            let body = serde_json::json!({
                "type": "error",
                "error": { "type": error_type, "message": "upstream failure" }
            });
            assert_eq!(
                anthropic_error_status_code(&body),
                status_code,
                "unexpected status for {error_type}"
            );
        }
    }
}
