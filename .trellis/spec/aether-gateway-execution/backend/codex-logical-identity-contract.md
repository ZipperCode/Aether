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
