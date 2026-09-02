use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use aether_provider_transport::CodexFingerprintConvergenceContext;
use http::{request::Parts, HeaderMap};
use serde_json::Value;
use uuid::Uuid;

use crate::client_session_affinity::codex_request_signals_from_request;

#[derive(Debug, Clone)]
/// 在克隆的 HTTP `Parts` 之间共享一次性 Codex 指纹上下文，避免重规划生成新身份。
pub(crate) struct CodexFingerprintContextSlot(
    /// 首个实际读取请求信号的调用负责初始化，后续克隆只复用同一值。
    Arc<OnceLock<CodexFingerprintConvergenceContext>>,
);

impl Default for CodexFingerprintContextSlot {
    /// 创建尚未解析请求身份信号的共享槽位。
    fn default() -> Self {
        Self(Arc::new(OnceLock::new()))
    }
}

impl CodexFingerprintContextSlot {
    /// 首次从原始头和正文构建上下文，之后无论传入何种重试正文都返回首值。
    fn resolve(
        &self,
        headers: &HeaderMap,
        body_json: &Value,
    ) -> CodexFingerprintConvergenceContext {
        self.0
            .get_or_init(|| {
                build_codex_fingerprint_context(headers, body_json, Uuid::now_v7().to_string())
            })
            .clone()
    }
}

/// 按“显式上下文、共享槽位、即时构建”的优先级解析当前逻辑 turn 的 Codex 身份。
pub(crate) fn resolve_codex_fingerprint_context(
    parts: &Parts,
    body_json: &Value,
) -> CodexFingerprintConvergenceContext {
    if let Some(context) = parts
        .extensions
        .get::<CodexFingerprintConvergenceContext>()
        .cloned()
    {
        return context;
    }
    if let Some(slot) = parts.extensions.get::<CodexFingerprintContextSlot>() {
        return slot.resolve(&parts.headers, body_json);
    }
    build_codex_fingerprint_context(&parts.headers, body_json, Uuid::now_v7().to_string())
}

/// 在读取正文前安装延迟槽位；若调用方已携带上下文或槽位则保持原值。
pub(crate) fn install_codex_fingerprint_context_slot(parts: &mut Parts) {
    if parts
        .extensions
        .get::<CodexFingerprintConvergenceContext>()
        .is_none()
        && parts
            .extensions
            .get::<CodexFingerprintContextSlot>()
            .is_none()
    {
        parts
            .extensions
            .insert(CodexFingerprintContextSlot::default());
    }
}

/// 确保 HTTP 重规划路径把解析结果固化进扩展，并移除不再需要的延迟槽位。
pub(crate) fn ensure_codex_fingerprint_context(
    parts: &mut Parts,
    body_json: &Value,
) -> CodexFingerprintConvergenceContext {
    let context = resolve_codex_fingerprint_context(parts, body_json);
    if parts
        .extensions
        .get::<CodexFingerprintConvergenceContext>()
        .is_none()
    {
        parts.extensions.remove::<CodexFingerprintContextSlot>();
        parts.extensions.insert(context.clone());
    }
    context
}

/// 为新 WebSocket 逻辑 turn 建立并附加上下文；`logical_turn_id` 在该 turn 全程稳定。
pub(crate) fn attach_codex_logical_turn_context(
    parts: &mut Parts,
    body_json: &Value,
    logical_turn_id: &str,
) -> CodexFingerprintConvergenceContext {
    let context =
        build_codex_fingerprint_context(&parts.headers, body_json, logical_turn_id.to_string());
    parts.extensions.remove::<CodexFingerprintContextSlot>();
    parts.extensions.insert(context.clone());
    context
}

/// 在重试、重绑或重新规划前恢复原逻辑 turn 上下文，禁止重新读取变化后的信号。
pub(crate) fn restore_codex_logical_turn_context(
    parts: &mut Parts,
    context: &CodexFingerprintConvergenceContext,
) {
    parts.extensions.remove::<CodexFingerprintContextSlot>();
    parts.extensions.insert(context.clone());
}

/// 将请求中的 turn、会话和缓存信号投影为传输层可复用的不可变上下文。
fn build_codex_fingerprint_context(
    headers: &HeaderMap,
    body_json: &Value,
    logical_turn_id: String,
) -> CodexFingerprintConvergenceContext {
    let signals = codex_request_signals_from_request(headers, Some(body_json));
    let mut context =
        CodexFingerprintConvergenceContext::new(logical_turn_id, current_unix_millis());

    if let Some(turn_id) = signals.turn_id {
        context = context.with_original_turn_id(turn_id);
    }
    if let Some(session_id) = signals.thread_id.or(signals.session_id) {
        context = context.with_original_client_session_id(session_id);
    }
    if let Some(prompt_cache_key) = signals.prompt_cache_key {
        context = context.with_original_prompt_cache_key(prompt_cache_key);
    }

    context
}

/// 返回当前 Unix 毫秒；系统时间早于纪元时按零处理，溢出时钳制到 `u64::MAX`。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;
    use serde_json::json;

    use super::*;

    /// 验证同一逻辑 turn 只采集一次请求信号，并把头部 thread 置于正文 thread 之前。
    #[test]
    fn request_signals_are_captured_once_for_the_logical_turn() {
        let request = http::Request::builder()
            .header("thread-id", "header-thread")
            .body(())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();
        let body = json!({
            "prompt_cache_key": "client-cache",
            "client_metadata": {
                "turn_id": "client-turn",
                "thread_id": "body-thread"
            }
        });

        let context = attach_codex_logical_turn_context(&mut parts, &body, "logical-turn");

        assert_eq!(context.logical_turn_id(), "logical-turn");
        assert_eq!(context.original_turn_id(), Some("client-turn"));
        assert_eq!(context.original_client_session_id(), Some("header-thread"));
        assert_eq!(context.original_prompt_cache_key(), Some("client-cache"));
        assert_eq!(
            parts.extensions.get::<CodexFingerprintConvergenceContext>(),
            Some(&context)
        );
    }

    /// 验证恢复的上下文优先于重试请求中新出现的会话、turn 与缓存字段。
    #[test]
    fn restored_context_wins_over_retry_request_signals() {
        let original = CodexFingerprintConvergenceContext::new("logical-turn", 1234)
            .with_original_turn_id("original-turn")
            .with_original_client_session_id("original-thread")
            .with_original_prompt_cache_key("original-cache");
        let request = http::Request::builder()
            .body(())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();
        parts
            .headers
            .insert("thread-id", HeaderValue::from_static("retry-thread"));
        restore_codex_logical_turn_context(&mut parts, &original);

        let resolved = resolve_codex_fingerprint_context(
            &parts,
            &json!({
                "prompt_cache_key": "retry-cache",
                "client_metadata": {"turn_id": "retry-turn"}
            }),
        );

        assert_eq!(resolved, original);
        assert_eq!(resolved.turn_started_at_unix_ms(), 1234);
    }

    /// 验证 HTTP 首次生成的上下文会持久到后续重新规划，而不会重新采样正文。
    #[test]
    fn generated_context_is_persisted_for_http_replanning() {
        let request = http::Request::builder()
            .header("session-id", "client-session")
            .body(())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();
        let body = json!({
            "prompt_cache_key": "client-cache",
            "client_metadata": {"turn_id": "client-turn"}
        });

        let first = ensure_codex_fingerprint_context(&mut parts, &body);
        let second = resolve_codex_fingerprint_context(
            &parts,
            &json!({
                "prompt_cache_key": "retry-cache",
                "client_metadata": {"turn_id": "retry-turn"}
            }),
        );

        assert_eq!(second, first);
        assert_eq!(second.original_turn_id(), Some("client-turn"));
        assert_eq!(second.original_prompt_cache_key(), Some("client-cache"));
    }

    /// 验证克隆的 `Parts` 共享同一个 `OnceLock`，首个解析结果决定整个请求身份。
    #[test]
    fn installed_slot_reuses_context_across_cloned_parts() {
        let request = http::Request::builder()
            .body(())
            .expect("request should build");
        let (mut parts, _) = request.into_parts();
        install_codex_fingerprint_context_slot(&mut parts);
        let cloned_parts = parts.clone();

        let first = resolve_codex_fingerprint_context(
            &parts,
            &json!({
                "prompt_cache_key": "first-cache",
                "client_metadata": {"turn_id": "first-turn"}
            }),
        );
        let second = resolve_codex_fingerprint_context(
            &cloned_parts,
            &json!({
                "prompt_cache_key": "second-cache",
                "client_metadata": {"turn_id": "second-turn"}
            }),
        );

        assert_eq!(second, first);
        assert_eq!(second.original_turn_id(), Some("first-turn"));
        assert_eq!(second.original_prompt_cache_key(), Some("first-cache"));
    }
}
