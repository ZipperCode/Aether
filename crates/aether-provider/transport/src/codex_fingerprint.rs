use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::snapshot::GatewayProviderTransportSnapshot;

pub const CODEX_FINGERPRINT_CONFIG_NAMESPACE: &str = "codex";
pub const CODEX_FINGERPRINT_ENABLED_CONFIG_KEY: &str = "fingerprint_convergence_enabled";

#[derive(Debug, Clone, PartialEq, Eq)]
/// 一个下游逻辑 turn 的原始身份信号；所有候选重试必须复用此值。
pub struct CodexFingerprintConvergenceContext {
    /// Aether 为当前逻辑 turn 分配的稳定标识，缺少客户端 turn 时参与派生。
    logical_turn_id: String,
    /// 客户端明确提供的 turn 标识；存在时优先于 Aether 标识。
    original_turn_id: Option<String>,
    /// 客户端 thread/session 标识，用于隔离同账号下不同会话。
    original_client_session_id: Option<String>,
    /// 客户端最初的缓存键；仅在最终正文仍保留缓存键时参与命名空间化。
    original_prompt_cache_key: Option<String>,
    /// 本 turn 首次建立上下文的 Unix 毫秒，用于生成稳定 UUIDv7 时间部分。
    turn_started_at_unix_ms: u64,
}

impl CodexFingerprintConvergenceContext {
    /// 创建逻辑 turn 上下文；调用方负责在所有重试路径复用同一实例。
    pub fn new(logical_turn_id: impl Into<String>, turn_started_at_unix_ms: u64) -> Self {
        Self {
            logical_turn_id: logical_turn_id.into().trim().to_string(),
            original_turn_id: None,
            original_client_session_id: None,
            original_prompt_cache_key: None,
            turn_started_at_unix_ms,
        }
    }

    /// 记录非空客户端 turn 标识；空白值按缺失处理。
    pub fn with_original_turn_id(mut self, original_turn_id: impl Into<String>) -> Self {
        self.original_turn_id = non_empty_owned(original_turn_id.into());
        self
    }

    /// 记录非空客户端 session/thread 标识，用于会话级身份隔离。
    pub fn with_original_client_session_id(
        mut self,
        original_client_session_id: impl Into<String>,
    ) -> Self {
        self.original_client_session_id = non_empty_owned(original_client_session_id.into());
        self
    }

    /// 记录非空原始缓存键，但不保证最终请求一定恢复该字段。
    pub fn with_original_prompt_cache_key(
        mut self,
        original_prompt_cache_key: impl Into<String>,
    ) -> Self {
        self.original_prompt_cache_key = non_empty_owned(original_prompt_cache_key.into());
        self
    }

    /// 返回 Aether 逻辑 turn 标识。
    pub fn logical_turn_id(&self) -> &str {
        self.logical_turn_id.as_str()
    }

    /// 返回客户端原始 turn 标识。
    pub fn original_turn_id(&self) -> Option<&str> {
        self.original_turn_id.as_deref()
    }

    /// 返回客户端原始会话标识。
    pub fn original_client_session_id(&self) -> Option<&str> {
        self.original_client_session_id.as_deref()
    }

    /// 返回客户端原始缓存键。
    pub fn original_prompt_cache_key(&self) -> Option<&str> {
        self.original_prompt_cache_key.as_deref()
    }

    /// 返回上下文建立时的 Unix 毫秒。
    pub fn turn_started_at_unix_ms(&self) -> u64 {
        self.turn_started_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// 最终写入上游头部和 Responses 正文的一组收敛身份。
struct CodexConvergedFingerprint {
    /// 账号成员稳定安装标识。
    installation_id: String,
    /// 账号范围稳定会话标识。
    session_id: String,
    /// 下游会话范围稳定 thread 标识。
    thread_id: String,
    /// 当前逻辑 turn 的稳定 UUIDv7。
    turn_id: String,
    /// Codex 窗口标识；当前固定使用 thread 的第零窗口。
    window_id: String,
    /// 原逻辑 turn 的开始时间，供嵌套元数据复用。
    turn_started_at_unix_ms: u64,
    /// 在账号成员命名空间内派生的可选缓存键。
    prompt_cache_key: Option<String>,
}

/// 判断 Provider 是否显式启用 Codex 指纹收敛；仅 `codex` 类型接受该开关。
pub fn codex_fingerprint_convergence_enabled(
    provider_type: &str,
    provider_config: Option<&Value>,
) -> bool {
    provider_type.trim().eq_ignore_ascii_case("codex")
        && provider_config
            .and_then(Value::as_object)
            .and_then(|config| config.get(CODEX_FINGERPRINT_CONFIG_NAMESPACE))
            .and_then(Value::as_object)
            .and_then(|config| config.get(CODEX_FINGERPRINT_ENABLED_CONFIG_KEY))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// 兼容旧调用方：为单次调用建立新 turn 上下文后执行指纹收敛。
/// 需要跨重试稳定身份的路径必须改用带上下文版本。
pub fn apply_codex_fingerprint_convergence(
    transport: &GatewayProviderTransportSnapshot,
    provider_api_format: &str,
    original_client_session_id: Option<&str>,
    provider_request_headers: &mut BTreeMap<String, String>,
    provider_request_body: &mut Value,
) -> bool {
    let mut context =
        CodexFingerprintConvergenceContext::new(Uuid::now_v7().to_string(), current_unix_millis());
    if let Some(original_client_session_id) = original_client_session_id {
        context = context.with_original_client_session_id(original_client_session_id);
    }
    apply_codex_fingerprint_convergence_with_context(
        transport,
        provider_api_format,
        &context,
        provider_request_headers,
        provider_request_body,
    )
}

/// 使用既有逻辑 turn 上下文收敛 Codex 请求头和 Responses 正文身份。
/// Compact、非 Codex、Agent Identity 或未启用场景保持原请求不变并返回 `false`。
pub fn apply_codex_fingerprint_convergence_with_context(
    transport: &GatewayProviderTransportSnapshot,
    provider_api_format: &str,
    context: &CodexFingerprintConvergenceContext,
    provider_request_headers: &mut BTreeMap<String, String>,
    provider_request_body: &mut Value,
) -> bool {
    let is_responses = aether_ai_formats::is_openai_responses_format(provider_api_format);
    let is_live = aether_ai_formats::api_format_alias_matches(provider_api_format, "codex:live");
    // Convergence is a Codex provider policy, independent of whether the key
    // uses OAuth, an API key, or another ordinary auth channel. Agent Identity
    // uses a separate signed-identity protocol and is excluded here.
    if !transport
        .provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case("codex")
        || crate::agent_identity::is_codex_agent_identity_transport(transport)
        || (!is_responses && !is_live)
        || is_responses
            && aether_ai_formats::openai_responses_request_operation(
                provider_api_format,
                provider_request_body,
            ) == Some(aether_ai_formats::OPENAI_RESPONSES_OPERATION_COMPACT)
        || !codex_fingerprint_convergence_enabled(
            transport.provider.provider_type.as_str(),
            transport.provider.config.as_ref(),
        )
        || !provider_request_body.is_object()
    {
        return false;
    }

    let auth_identity = aether_ai_formats::parse_codex_auth_identity(
        transport.key.decrypted_auth_config.as_deref(),
    );
    let account_seed = resolve_codex_account_seed(&auth_identity, transport.key.id.as_str());
    // Only namespace a cache key that survived all provider-body conversion and
    // routing rules. The client-side value in `context` is a retry signal, not
    // permission to resurrect a field that the terminal body deliberately
    // removed.
    let effective_prompt_cache_key = provider_request_body
        .as_object()
        .and_then(|body| body.get("prompt_cache_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let fingerprint = resolve_converged_fingerprint_with_prompt_cache(
        account_seed.as_str(),
        context,
        effective_prompt_cache_key,
    );

    apply_converged_headers(provider_request_headers, &fingerprint);
    // Live uses the converged identity on the WebSocket/call-control headers.
    // Its event/session payload is an independent opaque protocol and must not
    // receive Responses-only `client_metadata` fields.
    if is_responses {
        apply_converged_client_metadata(provider_request_body, &fingerprint);
    }
    true
}

#[cfg(test)]
/// 供测试直接解析收敛结果；生产路径通过请求改写消费结果。
fn resolve_converged_fingerprint(
    account_seed: &str,
    context: &CodexFingerprintConvergenceContext,
) -> CodexConvergedFingerprint {
    resolve_converged_fingerprint_with_prompt_cache(account_seed, context, None)
}

/// 从账号成员种子和 turn 上下文派生全部稳定 ID，并仅命名空间化仍有效的缓存键。
fn resolve_converged_fingerprint_with_prompt_cache(
    account_seed: &str,
    context: &CodexFingerprintConvergenceContext,
    effective_prompt_cache_key: Option<&str>,
) -> CodexConvergedFingerprint {
    let installation_id =
        derive_stable_uuid_v4(&format!("aether:codex-installation-id:v1:{account_seed}"));
    let session_id = derive_stable_uuid_v4(&format!("aether:codex-session-id:v1:{account_seed}"));
    let original_client_session_id = context
        .original_client_session_id()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let thread_id = original_client_session_id
        .map(|client_session_id| {
            derive_stable_uuid_v4(&format!(
                "aether:codex-thread-id:v1:{account_seed}:{client_session_id}"
            ))
        })
        .unwrap_or_else(|| session_id.clone());
    let window_id = format!("{thread_id}:0");
    let turn_identity = context
        .original_turn_id()
        .map(|turn_id| ("original", turn_id))
        .unwrap_or_else(|| ("logical", context.logical_turn_id()));
    let turn_id = derive_stable_uuid_v7(
        context.turn_started_at_unix_ms(),
        &format!(
            "aether:codex-turn-id:v1\0{account_seed}\0{}\0{}",
            turn_identity.0, turn_identity.1
        ),
    );
    let prompt_cache_key = context
        .original_prompt_cache_key()
        .and(effective_prompt_cache_key)
        .map(|effective| {
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("aether:codex-prompt-cache-key:v1\0{account_seed}\0{effective}").as_bytes(),
            )
            .to_string()
        });

    CodexConvergedFingerprint {
        installation_id,
        session_id,
        thread_id,
        turn_id,
        window_id,
        turn_started_at_unix_ms: context.turn_started_at_unix_ms(),
        prompt_cache_key,
    }
}

/// 规范化可选字符串，空白值统一视为缺失。
fn non_empty_owned(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// 返回当前 Unix 毫秒；时钟异常时回退为零。
fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

/// 规范化账号身份片段，使用小写保证同一身份不因大小写漂移。
fn normalized_identity_part(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

/// 按持久指纹、账号成员、成员、账号、Key 的顺序选择稳定且隔离的派生种子。
fn resolve_codex_account_seed(
    identity: &aether_ai_formats::CodexAuthIdentity,
    fallback_key_id: &str,
) -> String {
    let fingerprint = normalized_identity_part(identity.codex_identity_fingerprint.as_deref())
        .or_else(|| {
            aether_oauth::provider::providers::derive_codex_identity_fingerprint(
                identity.account_id.as_deref(),
                identity.account_user_id.as_deref(),
                identity.user_id.as_deref(),
                identity.email.as_deref(),
            )
        });
    if let Some(fingerprint) = fingerprint {
        return format!("persisted:v1:{fingerprint}");
    }

    let account = normalized_identity_part(identity.account_id.as_deref());
    let member = normalized_identity_part(identity.account_user_id.as_deref())
        .or_else(|| normalized_identity_part(identity.user_id.as_deref()))
        .or_else(|| normalized_identity_part(identity.email.as_deref()));
    match (account, member) {
        (Some(account), Some(member)) => format!("account-member:v1:{account}\0{member}"),
        (None, Some(member)) => format!("member:v1:{member}"),
        (Some(account), None) => format!("account:v1:{account}"),
        (None, None) => format!("key:v1:{}", fallback_key_id.trim()),
    }
}

/// 从确定性种子构造符合版本/variant 位约束的 UUIDv4 字符串。
fn derive_stable_uuid_v4(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

/// 用固定时间与种子构造确定性 UUIDv7，使同一逻辑 turn 的重试保持一致。
fn derive_stable_uuid_v7(timestamp_ms: u64, seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    let timestamp_bytes = timestamp_ms.min(0x0000_ffff_ffff_ffff).to_be_bytes();
    bytes[..6].copy_from_slice(&timestamp_bytes[2..]);
    bytes[6..].copy_from_slice(&digest[..10]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

/// 将收敛身份写入 Codex HTTP/Live 头部，并同步嵌套 turn 元数据。
fn apply_converged_headers(
    headers: &mut BTreeMap<String, String>,
    fingerprint: &CodexConvergedFingerprint,
) {
    set_header(
        headers,
        "x-codex-installation-id",
        fingerprint.installation_id.clone(),
    );
    set_header(headers, "x-codex-window-id", fingerprint.window_id.clone());
    set_header(
        headers,
        "x-client-request-id",
        fingerprint.thread_id.clone(),
    );
    set_header(headers, "session-id", fingerprint.session_id.clone());
    set_header(headers, "session_id", fingerprint.session_id.clone());
    set_header(headers, "thread-id", fingerprint.thread_id.clone());
    // Codex Live/Realtime uses `x-session-id` for the thread-scoped session
    // identity on the WebSocket upgrade request. Keep it aligned with the
    // converged thread identity instead of the account-scoped session value.
    set_header(headers, "x-session-id", fingerprint.thread_id.clone());
    rewrite_header_turn_metadata(headers, fingerprint);
}

/// 将收敛身份写入 Responses `client_metadata`；非对象正文保持不变。
fn apply_converged_client_metadata(body: &mut Value, fingerprint: &CodexConvergedFingerprint) {
    let Some(body) = body.as_object_mut() else {
        return;
    };
    if let Some(prompt_cache_key) = fingerprint.prompt_cache_key.as_ref() {
        body.insert(
            "prompt_cache_key".to_string(),
            Value::String(prompt_cache_key.clone()),
        );
    }
    let metadata = body
        .entry("client_metadata".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let Some(metadata) = metadata.as_object_mut() else {
        return;
    };

    metadata.insert(
        "x-codex-installation-id".to_string(),
        Value::String(fingerprint.installation_id.clone()),
    );
    metadata.insert(
        "session_id".to_string(),
        Value::String(fingerprint.session_id.clone()),
    );
    metadata.insert(
        "thread_id".to_string(),
        Value::String(fingerprint.thread_id.clone()),
    );
    metadata.insert(
        "turn_id".to_string(),
        Value::String(fingerprint.turn_id.clone()),
    );
    metadata.insert(
        "x-codex-window-id".to_string(),
        Value::String(fingerprint.window_id.clone()),
    );
    rewrite_embedded_turn_metadata(metadata, fingerprint);
}

/// 若头部携带合法 JSON turn 元数据，则原位改写身份字段；畸形值保持原样。
fn rewrite_header_turn_metadata(
    headers: &mut BTreeMap<String, String>,
    fingerprint: &CodexConvergedFingerprint,
) {
    let Some((name, raw)) = find_header(headers, "x-codex-turn-metadata") else {
        return;
    };
    let Ok(mut metadata) = serde_json::from_str::<Map<String, Value>>(&raw) else {
        return;
    };
    apply_turn_metadata_fields(&mut metadata, fingerprint);
    let Ok(rebuilt) = serde_json::to_string(&metadata) else {
        return;
    };
    headers.remove(&name);
    headers.insert("x-codex-turn-metadata".to_string(), rebuilt);
}

/// 同时支持对象和 JSON 字符串形态的嵌套 turn 元数据，其他类型不改写。
fn rewrite_embedded_turn_metadata(
    metadata: &mut Map<String, Value>,
    fingerprint: &CodexConvergedFingerprint,
) {
    let Some(turn_metadata) = metadata.get_mut("x-codex-turn-metadata") else {
        return;
    };
    match turn_metadata {
        Value::Object(turn_metadata) => apply_turn_metadata_fields(turn_metadata, fingerprint),
        Value::String(raw) => {
            let Ok(mut parsed) = serde_json::from_str::<Map<String, Value>>(raw) else {
                return;
            };
            apply_turn_metadata_fields(&mut parsed, fingerprint);
            let Ok(rebuilt) = serde_json::to_string(&parsed) else {
                return;
            };
            *raw = rebuilt;
        }
        _ => {}
    }
}

/// 在一个 turn 元数据对象中统一写入安装、会话、thread、turn、窗口和缓存身份。
fn apply_turn_metadata_fields(
    metadata: &mut Map<String, Value>,
    fingerprint: &CodexConvergedFingerprint,
) {
    metadata.insert(
        "installation_id".to_string(),
        Value::String(fingerprint.installation_id.clone()),
    );
    metadata.insert(
        "session_id".to_string(),
        Value::String(fingerprint.session_id.clone()),
    );
    metadata.insert(
        "thread_id".to_string(),
        Value::String(fingerprint.thread_id.clone()),
    );
    metadata.insert(
        "turn_id".to_string(),
        Value::String(fingerprint.turn_id.clone()),
    );
    metadata.insert(
        "window_id".to_string(),
        Value::String(fingerprint.window_id.clone()),
    );
    metadata.insert(
        "turn_started_at_unix_ms".to_string(),
        Value::from(fingerprint.turn_started_at_unix_ms),
    );
    if let Some(prompt_cache_key) = fingerprint.prompt_cache_key.as_ref() {
        metadata.insert(
            "prompt_cache_key".to_string(),
            Value::String(prompt_cache_key.clone()),
        );
    }
}

/// 不区分大小写替换一个头部，避免同名不同大小写同时发往上游。
fn set_header(headers: &mut BTreeMap<String, String>, name: &str, value: String) {
    let matching_names = headers
        .keys()
        .filter(|candidate| candidate.eq_ignore_ascii_case(name))
        .cloned()
        .collect::<Vec<_>>();
    for matching_name in matching_names {
        headers.remove(&matching_name);
    }
    headers.insert(name.to_string(), value);
}

/// 不区分大小写查找头部，并返回真实键名和值以便安全替换。
fn find_header(headers: &BTreeMap<String, String>, name: &str) -> Option<(String, String)> {
    headers
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(name, value)| (name.clone(), value.clone()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider,
    };

    fn sample_transport() -> GatewayProviderTransportSnapshot {
        GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: "provider-1".to_string(),
                name: "Codex".to_string(),
                provider_type: "codex".to_string(),
                website: None,
                is_active: true,
                keep_priority_on_conversion: false,
                enable_format_conversion: true,
                concurrent_limit: None,
                max_retries: None,
                proxy: None,
                request_timeout_secs: None,
                stream_first_byte_timeout_secs: None,
                config: Some(json!({
                    "codex": {"fingerprint_convergence_enabled": true},
                    "unrelated": {"kept": true}
                })),
            },
            endpoint: GatewayProviderTransportEndpoint {
                id: "endpoint-1".to_string(),
                provider_id: "provider-1".to_string(),
                api_format: "openai:responses".to_string(),
                api_family: None,
                endpoint_kind: None,
                is_active: true,
                base_url: "https://chatgpt.com/backend-api/codex".to_string(),
                header_rules: None,
                body_rules: None,
                max_retries: None,
                custom_path: None,
                config: None,
                format_acceptance_config: None,
                proxy: None,
            },
            key: GatewayProviderTransportKey {
                id: "key-1".to_string(),
                provider_id: "provider-1".to_string(),
                name: "OAuth".to_string(),
                auth_type: "oauth".to_string(),
                is_active: true,
                api_formats: None,
                auth_type_by_format: None,
                allow_auth_channel_mismatch_formats: None,
                allowed_models: None,
                capabilities: None,
                rate_multipliers: None,
                global_priority_by_format: None,
                expires_at_unix_secs: None,
                proxy: None,
                fingerprint: None,
                upstream_metadata: None,
                decrypted_api_key: "access-token".to_string(),
                decrypted_auth_config: Some(json!({"account_id": "account-1"}).to_string()),
            },
        }
    }

    /// 验证指纹收敛开关仅对显式启用的 Codex 提供商生效。
    #[test]
    fn provider_config_switch_is_opt_in_and_codex_only() {
        assert!(!codex_fingerprint_convergence_enabled("codex", None));
        assert!(!codex_fingerprint_convergence_enabled(
            "codex",
            Some(&json!({"codex": {"fingerprint_convergence_enabled": false}}))
        ));
        assert!(codex_fingerprint_convergence_enabled(
            "CODEX",
            Some(&json!({"codex": {"fingerprint_convergence_enabled": true}}))
        ));
        assert!(!codex_fingerprint_convergence_enabled(
            "openai",
            Some(&json!({"codex": {"fingerprint_convergence_enabled": true}}))
        ));
    }

    /// 验证一次收敛把相同身份集合同时写入头部、正文和嵌套元数据。
    #[test]
    fn convergence_rewrites_headers_and_body_with_one_identity_set() {
        let transport = sample_transport();
        let mut headers = BTreeMap::from([
            ("Session-Id".to_string(), "client-session".to_string()),
            (
                "X-Session-Id".to_string(),
                "client-live-session".to_string(),
            ),
            (
                "x-codex-turn-metadata".to_string(),
                json!({
                    "installation_id": "client-installation",
                    "session_id": "client-session",
                    "thread_source": "cli"
                })
                .to_string(),
            ),
        ]);
        let mut body = json!({
            "model": "gpt-5.4",
            "client_metadata": {
                "session_id": "client-session",
                "x-codex-turn-metadata": json!({
                    "installation_id": "client-installation",
                    "sandbox": "workspace-write"
                }).to_string()
            }
        });

        assert!(apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client-session"),
            &mut headers,
            &mut body,
        ));

        let session_id = headers.get("session-id").expect("session header");
        let thread_id = headers.get("thread-id").expect("thread header");
        let installation_id = headers
            .get("x-codex-installation-id")
            .expect("installation header");
        assert_eq!(
            Uuid::parse_str(session_id)
                .expect("session UUID")
                .get_version_num(),
            4
        );
        assert_eq!(
            Uuid::parse_str(thread_id)
                .expect("thread UUID")
                .get_version_num(),
            4
        );
        assert_eq!(
            Uuid::parse_str(installation_id)
                .expect("installation UUID")
                .get_version_num(),
            4
        );
        assert_eq!(headers["session_id"], *session_id);
        assert_eq!(headers["x-client-request-id"], *thread_id);
        assert_eq!(headers["x-session-id"], *thread_id);
        assert_eq!(headers["x-codex-window-id"], format!("{thread_id}:0"));
        assert_eq!(
            headers
                .keys()
                .filter(|name| name.eq_ignore_ascii_case("x-session-id"))
                .count(),
            1
        );
        assert_eq!(body["client_metadata"]["session_id"], *session_id);
        assert_eq!(body["client_metadata"]["thread_id"], *thread_id);
        assert_eq!(
            body["client_metadata"]["x-codex-installation-id"],
            *installation_id
        );

        let header_metadata: Value =
            serde_json::from_str(&headers["x-codex-turn-metadata"]).expect("header metadata");
        let body_metadata: Value = serde_json::from_str(
            body["client_metadata"]["x-codex-turn-metadata"]
                .as_str()
                .expect("embedded metadata"),
        )
        .expect("body metadata");
        assert_eq!(
            header_metadata["turn_id"],
            body["client_metadata"]["turn_id"]
        );
        assert_eq!(body_metadata["turn_id"], body["client_metadata"]["turn_id"]);
        assert_eq!(header_metadata["thread_source"], "cli");
        assert_eq!(body_metadata["sandbox"], "workspace-write");
        assert_eq!(
            Uuid::parse_str(
                body["client_metadata"]["turn_id"]
                    .as_str()
                    .expect("turn id")
            )
            .expect("turn UUID")
            .get_version_num(),
            7
        );
    }

    /// 验证对象形态的嵌套 turn 元数据与外层字段使用同一套收敛身份。
    #[test]
    fn object_form_embedded_turn_metadata_is_rewritten_with_the_same_identity() {
        let transport = sample_transport();
        let context = CodexFingerprintConvergenceContext::new("logical-turn-1", 1_700_000_000_123)
            .with_original_turn_id("client-turn-1")
            .with_original_client_session_id("client-session-1")
            .with_original_prompt_cache_key("client-cache-1");
        let mut headers = BTreeMap::new();
        let mut body = json!({
            "model": "gpt-5.4",
            "client_metadata": {
                "x-codex-turn-metadata": {
                    "installation_id": "old-installation",
                    "session_id": "old-session",
                    "thread_id": "old-thread",
                    "turn_id": "old-turn",
                    "window_id": "old-window",
                    "custom": "preserved"
                }
            }
        });

        assert!(apply_codex_fingerprint_convergence_with_context(
            &transport,
            "openai:responses",
            &context,
            &mut headers,
            &mut body,
        ));

        let embedded = &body["client_metadata"]["x-codex-turn-metadata"];
        assert!(embedded.is_object());
        assert_eq!(embedded["custom"], "preserved");
        assert_eq!(
            embedded["installation_id"],
            body["client_metadata"]["x-codex-installation-id"]
        );
        assert_eq!(
            embedded["session_id"],
            body["client_metadata"]["session_id"]
        );
        assert_eq!(embedded["thread_id"], body["client_metadata"]["thread_id"]);
        assert_eq!(embedded["turn_id"], body["client_metadata"]["turn_id"]);
        assert_eq!(
            embedded["window_id"],
            body["client_metadata"]["x-codex-window-id"]
        );
        assert_eq!(embedded["prompt_cache_key"], body["prompt_cache_key"]);
        assert_eq!(
            embedded["turn_started_at_unix_ms"],
            context.turn_started_at_unix_ms()
        );
    }

    /// 验证账号身份、客户端 thread 与逻辑 turn 各自在正确范围内确定且相互隔离。
    #[test]
    fn stable_account_identity_client_thread_and_logical_turn_are_deterministic() {
        let context = CodexFingerprintConvergenceContext::new("logical-turn-1", 1_700_000_000_123)
            .with_original_client_session_id("client-a");
        let other_turn_context =
            CodexFingerprintConvergenceContext::new("logical-turn-2", 1_700_000_000_124)
                .with_original_client_session_id("client-a");
        let other_client_context = context.clone().with_original_client_session_id("client-b");
        let first = resolve_converged_fingerprint("account-1", &context);
        let second = resolve_converged_fingerprint("account-1", &context);
        let other_turn = resolve_converged_fingerprint("account-1", &other_turn_context);
        let other_client = resolve_converged_fingerprint("account-1", &other_client_context);
        let other_account = resolve_converged_fingerprint("account-2", &context);

        assert_eq!(first.installation_id, second.installation_id);
        assert_eq!(first.session_id, second.session_id);
        assert_eq!(first.thread_id, second.thread_id);
        assert_eq!(first.turn_id, second.turn_id);
        assert_eq!(first.turn_started_at_unix_ms, 1_700_000_000_123);
        assert_ne!(first.turn_id, other_turn.turn_id);
        assert_ne!(first.thread_id, other_client.thread_id);
        assert_eq!(first.session_id, other_client.session_id);
        assert_ne!(first.installation_id, other_account.installation_id);
        assert_ne!(first.turn_id, other_account.turn_id);
        assert_eq!(
            Uuid::parse_str(&first.turn_id)
                .expect("turn UUID")
                .get_version_num(),
            7
        );
        assert_eq!(
            first.turn_id.replace('-', "")[..12],
            format!("{:012x}", context.turn_started_at_unix_ms())
        );
    }

    /// 验证旧包装函数仍为每次独立调用建立新 turn，且不凭空创建缓存键。
    #[test]
    fn legacy_wrapper_keeps_a_fresh_turn_and_does_not_namespace_prompt_cache() {
        let transport = sample_transport();
        let mut first_headers = BTreeMap::new();
        let mut second_headers = BTreeMap::new();
        let mut first_body = json!({
            "model": "gpt-5.4",
            "prompt_cache_key": "existing-cache"
        });
        let mut second_body = first_body.clone();

        assert!(apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client-session"),
            &mut first_headers,
            &mut first_body,
        ));
        assert!(apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client-session"),
            &mut second_headers,
            &mut second_body,
        ));

        assert_ne!(
            first_body["client_metadata"]["turn_id"],
            second_body["client_metadata"]["turn_id"]
        );
        assert_eq!(first_body["prompt_cache_key"], "existing-cache");
        assert_eq!(second_body["prompt_cache_key"], "existing-cache");
    }

    /// 验证显式上下文在重试中复用原始时间和命名空间化后的缓存键。
    #[test]
    fn convergence_context_reuses_original_turn_time_and_namespaced_prompt_cache_key() {
        let mut transport = sample_transport();
        transport.key.decrypted_auth_config = Some(
            json!({
                "account_id": "workspace-1",
                "account_user_id": "member-1",
                "codex_identity_fingerprint": "codex-persisted-fingerprint:v1:member-1"
            })
            .to_string(),
        );
        let context =
            CodexFingerprintConvergenceContext::new("logical-attempt-1", 1_700_000_000_123)
                .with_original_turn_id("client-turn-1")
                .with_original_client_session_id("client-session-1")
                .with_original_prompt_cache_key("client-cache-1");
        let original_headers = BTreeMap::from([(
            "x-codex-turn-metadata".to_string(),
            json!({
                "turn_id": "client-turn-1",
                "prompt_cache_key": "client-cache-1"
            })
            .to_string(),
        )]);
        let original_body = json!({
            "model": "gpt-5.4",
            "prompt_cache_key": "already-adapted-cache",
            "client_metadata": {
                "x-codex-turn-metadata": json!({
                    "turn_id": "client-turn-1",
                    "prompt_cache_key": "client-cache-1"
                }).to_string()
            }
        });

        let mut first_headers = original_headers.clone();
        let mut first_body = original_body.clone();
        let mut retried_headers = original_headers;
        let mut retried_body = original_body;
        assert!(apply_codex_fingerprint_convergence_with_context(
            &transport,
            "openai:responses",
            &context,
            &mut first_headers,
            &mut first_body,
        ));
        assert!(apply_codex_fingerprint_convergence_with_context(
            &transport,
            "openai:responses",
            &context,
            &mut retried_headers,
            &mut retried_body,
        ));

        assert_eq!(first_headers, retried_headers);
        assert_eq!(first_body, retried_body);
        assert_eq!(context.logical_turn_id(), "logical-attempt-1");
        assert_eq!(context.original_turn_id(), Some("client-turn-1"));
        assert_eq!(
            context.original_client_session_id(),
            Some("client-session-1")
        );
        assert_eq!(context.original_prompt_cache_key(), Some("client-cache-1"));
        assert_eq!(context.turn_started_at_unix_ms(), 1_700_000_000_123);

        let prompt_cache_key = first_body["prompt_cache_key"]
            .as_str()
            .expect("prompt cache key");
        assert_ne!(prompt_cache_key, "client-cache-1");
        assert_ne!(prompt_cache_key, "already-adapted-cache");
        assert_eq!(
            Uuid::parse_str(prompt_cache_key)
                .expect("prompt cache UUID")
                .get_version_num(),
            5
        );
        let header_metadata: Value =
            serde_json::from_str(&first_headers["x-codex-turn-metadata"]).expect("header metadata");
        let body_metadata: Value = serde_json::from_str(
            first_body["client_metadata"]["x-codex-turn-metadata"]
                .as_str()
                .expect("body metadata"),
        )
        .expect("body metadata json");
        assert_eq!(header_metadata["prompt_cache_key"], prompt_cache_key);
        assert_eq!(body_metadata["prompt_cache_key"], prompt_cache_key);
        assert_eq!(
            header_metadata["turn_started_at_unix_ms"],
            1_700_000_000_123_u64
        );
        assert_eq!(
            body_metadata["turn_started_at_unix_ms"],
            1_700_000_000_123_u64
        );

        let same_original_turn = context.clone().with_original_turn_id("client-turn-1");
        let changed_logical_turn =
            CodexFingerprintConvergenceContext::new("logical-attempt-2", 1_700_000_000_123)
                .with_original_turn_id("client-turn-1");
        assert_eq!(
            resolve_converged_fingerprint("account-1", &same_original_turn).turn_id,
            resolve_converged_fingerprint("account-1", &changed_logical_turn).turn_id
        );
    }

    /// 验证正文规则删除缓存键后，原始上下文不会把该字段复活。
    #[test]
    fn convergence_does_not_resurrect_a_removed_prompt_cache_key() {
        let transport = sample_transport();
        let context = CodexFingerprintConvergenceContext::new("logical-turn", 1_700_000_000_123)
            .with_original_prompt_cache_key("client-cache");
        let mut body = json!({
            "model": "gpt-5.4",
            "client_metadata": {}
        });
        let mut headers = BTreeMap::new();

        assert!(apply_codex_fingerprint_convergence_with_context(
            &transport,
            "openai:responses",
            &context,
            &mut headers,
            &mut body,
        ));

        assert!(body.get("prompt_cache_key").is_none());
        assert!(body["client_metadata"].get("prompt_cache_key").is_none());
    }

    /// 验证持久指纹优先于可变令牌声明，并在缺失时采用规范账号成员身份。
    #[test]
    fn persisted_or_canonical_member_identity_drives_the_account_seed() {
        let persisted_a = aether_ai_formats::parse_codex_auth_identity(Some(
            r#"{"account_id":"workspace-a","email":"old@example.com","codex_identity_fingerprint":"Stable-Member"}"#,
        ));
        let persisted_b = aether_ai_formats::parse_codex_auth_identity(Some(
            r#"{"account_id":"workspace-b","email":"new@example.com","codex_identity_fingerprint":"stable-member"}"#,
        ));
        assert_eq!(
            resolve_codex_account_seed(&persisted_a, "key-a"),
            resolve_codex_account_seed(&persisted_b, "key-b")
        );

        let canonical_a = aether_ai_formats::parse_codex_auth_identity(Some(
            r#"{"account_id":"Workspace-1","account_user_id":"Member-1","email":"old@example.com"}"#,
        ));
        let canonical_b = aether_ai_formats::parse_codex_auth_identity(Some(
            r#"{"account_id":"workspace-1","account_user_id":"member-1","email":"new@example.com"}"#,
        ));
        let other_member = aether_ai_formats::parse_codex_auth_identity(Some(
            r#"{"account_id":"workspace-1","account_user_id":"member-2"}"#,
        ));
        assert_eq!(
            resolve_codex_account_seed(&canonical_a, "key-a"),
            resolve_codex_account_seed(&canonical_b, "key-b")
        );
        assert_ne!(
            resolve_codex_account_seed(&canonical_a, "key-a"),
            resolve_codex_account_seed(&other_member, "key-a")
        );

        let derived_fingerprint =
            aether_oauth::provider::providers::derive_codex_identity_fingerprint(
                canonical_a.account_id.as_deref(),
                canonical_a.account_user_id.as_deref(),
                canonical_a.user_id.as_deref(),
                canonical_a.email.as_deref(),
            )
            .expect("canonical member fingerprint");
        let after_first_refresh = aether_ai_formats::parse_codex_auth_identity(Some(
            &json!({
                "account_id": "Workspace-1",
                "account_user_id": "Member-1",
                "email": "old@example.com",
                "codex_identity_fingerprint": derived_fingerprint
            })
            .to_string(),
        ));
        let legacy_seed = resolve_codex_account_seed(&canonical_a, "key-a");
        let refreshed_seed = resolve_codex_account_seed(&after_first_refresh, "key-a");
        assert_eq!(legacy_seed, refreshed_seed);

        let context = CodexFingerprintConvergenceContext::new("logical-turn-1", 1_700_000_000_123)
            .with_original_client_session_id("client-session-1")
            .with_original_prompt_cache_key("client-cache-1");
        assert_eq!(
            resolve_converged_fingerprint(&legacy_seed, &context),
            resolve_converged_fingerprint(&refreshed_seed, &context)
        );
    }

    /// 验证 Live 只改写 WebSocket 身份头，不向其 opaque 事件正文注入 Responses 字段。
    #[test]
    fn live_convergence_sets_the_websocket_identity_without_mutating_the_payload() {
        let transport = sample_transport();
        let original_body = json!({"model": "gpt-live", "future_live_field": true});
        let mut body = original_body.clone();
        let mut headers = BTreeMap::new();

        assert!(apply_codex_fingerprint_convergence(
            &transport,
            "codex:live",
            Some("client-live-session"),
            &mut headers,
            &mut body,
        ));

        assert_eq!(body, original_body);
        assert_eq!(headers.get("x-session-id"), headers.get("thread-id"));
        assert!(headers.contains_key("x-codex-installation-id"));
        assert!(headers.contains_key("x-codex-window-id"));
    }

    /// 验证关闭开关、错误格式与 Compact 等范围外请求均保持原样。
    #[test]
    fn disabled_or_out_of_scope_requests_are_unchanged() {
        let mut transport = sample_transport();
        let original_headers = BTreeMap::from([("session-id".to_string(), "client".to_string())]);
        let original_body = json!({"model": "gpt-5.4"});

        transport.provider.config = None;
        let mut headers = original_headers.clone();
        let mut body = original_body.clone();
        assert!(!apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client"),
            &mut headers,
            &mut body,
        ));
        assert_eq!(headers, original_headers);
        assert_eq!(body, original_body);
        transport.provider.config = Some(json!({
            "codex": {"fingerprint_convergence_enabled": true}
        }));

        for api_format in [
            "openai:responses:compact",
            "openai:chat",
            "openai:search",
            "openai:image",
        ] {
            let mut headers = original_headers.clone();
            let mut body = original_body.clone();
            assert!(!apply_codex_fingerprint_convergence(
                &transport,
                api_format,
                Some("client"),
                &mut headers,
                &mut body,
            ));
            assert_eq!(headers, original_headers);
            assert_eq!(body, original_body);
        }

        let mut compact_v2_headers = original_headers.clone();
        let mut compact_v2_body = json!({
            "model": "gpt-5.4",
            "input": [{"type": "compaction_trigger"}]
        });
        let original_compact_v2_body = compact_v2_body.clone();
        assert!(!apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client"),
            &mut compact_v2_headers,
            &mut compact_v2_body,
        ));
        assert_eq!(compact_v2_headers, original_headers);
        assert_eq!(compact_v2_body, original_compact_v2_body);
    }

    /// 验证普通 Codex API Key/OAuth 通道均执行收敛，不再限定于 OAuth。
    #[test]
    fn ordinary_codex_auth_channels_apply_convergence() {
        for auth_type in ["api_key", "bearer"] {
            let mut transport = sample_transport();
            transport.key.auth_type = auth_type.to_string();
            let original_headers =
                BTreeMap::from([("x-custom-header".to_string(), "preserve-me".to_string())]);
            let original_body = json!({"model": "gpt-5.4"});
            let mut headers = original_headers.clone();
            let mut body = original_body.clone();

            assert!(apply_codex_fingerprint_convergence(
                &transport,
                "openai:responses",
                Some("client"),
                &mut headers,
                &mut body,
            ));
            assert_ne!(headers, original_headers, "auth_type={auth_type}");
            assert_ne!(body, original_body, "auth_type={auth_type}");
            assert!(headers.contains_key("x-codex-installation-id"));
            assert_eq!(body["client_metadata"]["session_id"], headers["session-id"]);
        }
    }

    /// 验证非 Codex Provider 即使配置同名开关也不修改请求。
    #[test]
    fn non_codex_providers_are_unchanged_even_with_codex_convergence_enabled() {
        let context = CodexFingerprintConvergenceContext::new("logical-turn", 1_700_000_000_123)
            .with_original_turn_id("client-turn")
            .with_original_client_session_id("client-session")
            .with_original_prompt_cache_key("client-cache");

        for provider_type in ["openai", "anthropic", "custom"] {
            let mut transport = sample_transport();
            transport.provider.provider_type = provider_type.to_string();
            let original_headers = BTreeMap::from([
                ("session-id".to_string(), "client-session".to_string()),
                (
                    "x-codex-turn-metadata".to_string(),
                    json!({"turn_id": "client-turn"}).to_string(),
                ),
                ("x-custom-header".to_string(), "preserve-me".to_string()),
            ]);
            let original_body = json!({
                "model": "gpt-5.4",
                "prompt_cache_key": "client-cache",
                "client_metadata": {"session_id": "client-session"}
            });
            let mut headers = original_headers.clone();
            let mut body = original_body.clone();

            assert!(!apply_codex_fingerprint_convergence_with_context(
                &transport,
                "openai:responses",
                &context,
                &mut headers,
                &mut body,
            ));
            assert_eq!(headers, original_headers, "provider={provider_type}");
            assert_eq!(body, original_body, "provider={provider_type}");
        }
    }

    /// 验证普通 Codex 通道在同一 Key 内稳定，并在不同 Key 之间隔离。
    #[test]
    fn ordinary_codex_auth_channels_are_stable_and_key_scoped() {
        let context = CodexFingerprintConvergenceContext::new("logical-turn", 1_700_000_000_123)
            .with_original_client_session_id("client-session");
        for auth_type in ["api_key", "bearer"] {
            let mut transport = sample_transport();
            transport.key.auth_type = auth_type.to_string();
            transport.key.decrypted_auth_config = None;

            let mut first_headers = BTreeMap::new();
            let mut first_body = json!({"model": "gpt-5.4"});
            assert!(apply_codex_fingerprint_convergence_with_context(
                &transport,
                "openai:responses",
                &context,
                &mut first_headers,
                &mut first_body,
            ));

            let mut retry_headers = BTreeMap::new();
            let mut retry_body = json!({"model": "gpt-5.4"});
            assert!(apply_codex_fingerprint_convergence_with_context(
                &transport,
                "openai:responses",
                &context,
                &mut retry_headers,
                &mut retry_body,
            ));
            assert_eq!(first_headers, retry_headers, "auth_type={auth_type}");
            assert_eq!(first_body, retry_body, "auth_type={auth_type}");

            transport.key.id = "key-2".to_string();
            let mut other_key_headers = BTreeMap::new();
            let mut other_key_body = json!({"model": "gpt-5.4"});
            assert!(apply_codex_fingerprint_convergence_with_context(
                &transport,
                "openai:responses",
                &context,
                &mut other_key_headers,
                &mut other_key_body,
            ));
            assert_ne!(
                first_headers["x-codex-installation-id"],
                other_key_headers["x-codex-installation-id"],
                "auth_type={auth_type}"
            );
            assert_ne!(
                first_body["client_metadata"], other_key_body["client_metadata"],
                "auth_type={auth_type}"
            );
        }
    }

    /// 验证 Agent Identity 的签名协议不经过普通 Codex 指纹改写。
    #[test]
    fn agent_identity_transport_is_unchanged() {
        let mut transport = sample_transport();
        transport.key.decrypted_auth_config = Some(
            json!({
                "auth_mode": "agentIdentity",
                "agent_identity": {
                    "agent_runtime_id": "runtime-1",
                    "agent_private_key": "private-key"
                }
            })
            .to_string(),
        );
        let original_headers = BTreeMap::from([("session-id".to_string(), "client".to_string())]);
        let original_body = json!({"model": "gpt-5.4"});
        let mut headers = original_headers.clone();
        let mut body = original_body.clone();

        assert!(!apply_codex_fingerprint_convergence(
            &transport,
            "openai:responses",
            Some("client"),
            &mut headers,
            &mut body,
        ));
        assert_eq!(headers, original_headers);
        assert_eq!(body, original_body);
    }
}
