# Codex Logical Identity Convergence Contract

## 1. Scope / Trigger

Use this contract when changing Codex OAuth identity, request signal extraction,
HTTP replanning, Responses WebSocket turn bootstrap/rebind/quota retry, Codex
Live planning, or provider request headers/body. It ensures one downstream
logical turn presents one stable upstream identity even when Aether executes
several candidate attempts.

## 2. Signatures

```rust
pub struct CodexFingerprintConvergenceContext {
    // private stable logical-turn fields
}

pub(crate) fn install_codex_fingerprint_context_slot(parts: &mut Parts);
pub(crate) fn ensure_codex_fingerprint_context(
    parts: &mut Parts,
    body_json: &Value,
) -> CodexFingerprintConvergenceContext;
pub(crate) fn attach_codex_logical_turn_context(
    parts: &mut Parts,
    body_json: &Value,
    logical_turn_id: &str,
) -> CodexFingerprintConvergenceContext;
pub(crate) fn restore_codex_logical_turn_context(
    parts: &mut Parts,
    context: &CodexFingerprintConvergenceContext,
);

pub fn apply_codex_fingerprint_convergence_with_context(
    transport: &GatewayProviderTransportSnapshot,
    provider_api_format: &str,
    context: &CodexFingerprintConvergenceContext,
    provider_request_headers: &mut BTreeMap<String, String>,
    provider_request_body: &mut Value,
) -> bool;
```

Provider configuration is opt-in:

```json
{"codex":{"fingerprint_convergence_enabled":true}}
```

OAuth `auth_config` persists `codex_identity_fingerprint`, derived from stable
member claims rather than the rotating access token.

## 3. Contracts

- Capture request turn/session/thread/prompt-cache signals once when a logical
  turn begins. HTTP installs a shared lazy slot before body parsing; cloned
  `Parts` and every replan reuse the initialized context.
- Responses WebSocket and Codex Live create one context per client turn and
  explicitly restore it before retry, replan, provider rebind, and quota retry.
  A new client turn receives a new logical-turn context.
- Provider egress derives installation, session, thread, turn, window, and
  optional prompt-cache identities deterministically from the stable account
  member fingerprint plus the immutable turn context.
- The same logical turn must produce the same identities across HTTP sync/SSE,
  Responses WebSocket, Live, candidate failover, and same-Key retry. The
  transport auth channel (OAuth, API Key, or ordinary Bearer) does not change
  this rule.
- Only provider type `codex` with the explicit flag participates. Codex Agent
  Identity uses its separate signed protocol and must remain unchanged.
- Responses Compact is excluded. For `codex:live`, converge the WebSocket
  identity headers without mutating the Live payload.
- Namespace a `prompt_cache_key` only when that field survived final body
  conversion/routing rules. A value captured from the original request is a
  retry signal, not permission to resurrect a deliberately removed field.
- OAuth import and refresh preserve the existing member fingerprint when fresh
  token payloads omit stable claims. Access-token rotation must not change the
  identity seed; missing stable claims may fall back to the Key ID.
- Responses WebSocket create-turn transport is supported. Retrieve, delete,
  cancel, input-items, input-tokens, Aether Response persistence, and
  `previous_response_id` affinity remain separate product contracts.
- 头部 `x-codex-turn-metadata` 经身份收敛重写后必须为 ASCII-safe JSON；中文与非 BMP 字符使用 UTF-16 `\uXXXX` 转义，未知键/嵌套字段的 JSON 语义保留。已有 ASCII 输出直接复用序列化缓冲；正文元数据不做头部编码转换。
- `UpstreamBindingIdentity::from_decision` 对合法头值使用原始字节，不得用 `HeaderValue::to_str()` 附加 ASCII 限制。先认证分类，再排除 turn-scoped 字段，仅对需要参与身份的值复制字节；显式认证头即使与 turn metadata 同名也必须进入凭据指纹。
- 字节身份沿用现有摘要域与长度编码，ASCII 指纹保持逐位兼容；稳定头变化改变连接身份，turn metadata 变化不重绑。非法头名与 CR/LF/NUL 仍由握手构造验证拒绝。

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Codex provider flag disabled/missing | Leave request identity unchanged. |
| Non-Codex provider | Leave request unchanged even if a similarly named config exists. |
| Codex Agent Identity transport | Do not apply convergence. |
| Responses Compact request | Do not apply convergence. |
| HTTP request replans after body parsing | Reuse the shared initialized context. |
| Responses WS retry/rebind/quota retry | Restore the original turn context before planning. |
| Live retry | Reuse identity headers; do not add Responses body metadata. |
| Final body removed `prompt_cache_key` | Keep it absent. |
| OAuth refresh omits member claims | Preserve the stored fingerprint. |
| Stable member claims unavailable | Use the stable Key fallback, never the rotating token. |
| Unicode turn metadata 经收敛重写 | 输出 ASCII-safe JSON，解析语义与未知字段不丢失。 |
| 合法 UTF-8 稳定握手头 | 原始字节参与身份与摘要，值改变必须被识别。 |
| 仅 turn metadata 变化 | 复用绑定；若该名称是显式认证头则不得排除。 |
| 非法头名或控制字符 | 保留 InvalidHandshakeHeaders 错误，不删头或吞错。 |

## 5. Good / Base / Bad Cases

- Good: a Responses WebSocket turn retries three candidates and each egress
  request has the same turn/thread/cache identity; the next client turn differs.
- Base: a Codex provider without the opt-in flag keeps existing headers/body.
- Good: OAuth access and refresh tokens rotate while the persisted member
  fingerprint and derived installation ID stay stable.
- Bad: rebuilding context from each retry body, seeding identity from an access
  token, reintroducing a removed cache key, or applying convergence to Agent
  Identity or Compact.

## 6. Tests Required

- `codex_fingerprint::tests`: deterministic account/session/thread/turn output,
  auth-channel independence, non-Codex/disabled/Agent Identity exclusions,
  Live header behavior, and no cache-key resurrection.
- `ai_serving::codex_context::tests`: one-time signal capture, cloned HTTP parts,
  restored context precedence, and HTTP replan persistence.
- Gateway planner regression:
  `codex_fingerprint_convergence_runs_at_every_provider_routing_success_exit`.
- Client-session tests: header/body precedence for session, thread, turn, Live,
  and Responses session signals.
- OAuth tests: member-scoped fingerprint survives token rotation and refresh.
- Responses WS/Live tests must cover bootstrap, rebind, and retry with the same
  context, while separate turns produce separate turn identities.
- `header_turn_metadata_rewrite_outputs_ascii_safe_json`：中文/非 BMP、已有转义输入和畸形 JSON 边界。
- binding 回归覆盖 UTF-8 turn/stable/auth 头、ASCII 摘要兼容、非法头拒绝及 token refresh generation 不变。
- `codex_unicode_turn_metadata_binds_and_continuations_reuse_the_socket`：真实收敛→本机 WS 握手，上游收到 ASCII metadata 和原始 UTF-8 稳定头；先消费第一轮回复，再验证第二轮回复及握手计数仍为 1。

## 7. Wrong vs Correct

### Wrong

```rust
for candidate in candidates {
    let context = CodexFingerprintConvergenceContext::new(Uuid::now_v7(), now());
    send(candidate, context).await?;
}
```

### Correct

```rust
let context = ensure_codex_fingerprint_context(parts, original_body);
for candidate in candidates {
    restore_codex_logical_turn_context(parts, &context);
    send(candidate, context.clone()).await?;
}
```

错误：将 JSON 转义后的 Unicode 重序列化为原始中文，再通过 ASCII-only 访问器计算连接身份；或者简单删掉元数据“修复”握手。

正确：头部 JSON 使用语义等价的 ASCII 编码，连接身份按合法原始头字节比较，turn-scoped 与认证优先级明确分离。
