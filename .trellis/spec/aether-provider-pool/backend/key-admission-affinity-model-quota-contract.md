# Key Admission, Affinity, and Model Quota Contract

## 1. Scope / Trigger

Use this contract when changing Provider Key `concurrent_limit`, runtime
semaphores, sync/stream/WebSocket admission, Pool cache affinity, model quota
windows, Antigravity quota projection, Codex reset-credit state, or quota-driven
model discovery. It keeps every transport on the same atomic Key capacity and
prevents one model's exhausted window from blocking a different model.

## 2. Signatures

```rust
pub fn RuntimeState::keyed_semaphore(
    &self,
    gate: &'static str,
    resource_key: &str,
    limit: usize,
    config: RuntimeSemaphoreConfig,
) -> Result<RuntimeSemaphore, RuntimeSemaphoreError>;

pub(crate) async fn acquire_provider_pool_execution_guard(
    state: &AppState,
    plan: &ExecutionPlan,
) -> Result<ProviderPoolInFlightAdmission, GatewayError>;

pub(crate) enum ProviderPoolInFlightAdmission {
    Acquired(Option<ProviderPoolInFlightGuard>),
    Saturated { limit: usize },
}
```

Relevant persisted/admin JSON shapes are:

```json
{"concurrent_limit":4}
{"preset":"cache_affinity","enabled":true,"mode":"single_account"}
{"preset":"cache_affinity","enabled":true,"mode":"lru"}
{"status_snapshot":{"quota":{"windows":[{"model":"gemini-2.5-pro","exhausted":true,"reset_at":1710000000}]}}}
```

The current Antigravity admin snapshot stores model entries under
`upstream_metadata.antigravity.models`. Readers may accept the older
`quota_by_model` key as a compatibility input, but new local writes and tests
must use `models`.

## 3. Contracts

- Before every provider execution, strongly read the selected Key and confirm
  both Key ID and Provider ID. A read error, missing Key, or mismatch fails
  closed; never assume an unknown `concurrent_limit` is unlimited.
- Positive `concurrent_limit` values use one keyed semaphore resource per Key.
  Memory mode isolates capacity inside one process; Redis mode provides the
  same resource isolation across gateway instances. Missing, zero, or negative
  limits mean no Key capacity limit.
- Sync, stream, Responses WebSocket turns, and Codex Live turns all acquire the
  same admission guard. The RAII guard owns both Provider in-flight accounting
  and the optional Key permit and releases them on success, retry, error,
  cancellation, and WebSocket turn completion.
- Saturation is `provider_key_concurrency_limit_reached`. A final response is
  HTTP 429 only when every skipped candidate reason is capacity-related;
  mixed capacity/non-capacity exhaustion remains HTTP 503.
- Pool selection still applies balance, resettable quota, confirmed runtime
  quota, OAuth, health, active-probe, and exact Endpoint filters. Concurrency
  admission is not a replacement for those facts.
- Cache-affinity sticky hits remain first. On a miss, mode `single_account`
  concentrates work using reverse-LRU behavior; mode `lru` rotates to the
  least-recently-used Key. Unknown/missing mode uses `single_account`.
- A model-scoped quota window is evaluated only when its normalized `model`
  matches the requested provider model. Exhaustion for model A must not block
  model B on the same Key. Provider-wide windows retain their existing scope.
- Antigravity refresh must preserve refresh credentials and project each model
  window with reset aliases (`reset_at`, `next_reset_at`, `reset_time`,
  `next_reset_time`), accepting Unix and RFC3339 sources.
- A successful Antigravity admin quota refresh extracts routable model IDs from
  `antigravity.models` (or the legacy `quota_by_model` fallback), applies the
  shared case-insensitive internal-model exclusion predicate, and imports them
  through the normal admin model catalog path. Every imported Provider Model
  carries the exact source `endpoint.id`; never re-infer an Endpoint from the
  Provider after discovery.
- Catalog synchronization is a best-effort side effect of a successful quota
  refresh. Item or repository failures are warnings and must not turn valid
  quota data into a failed refresh.
- Antigravity OAuth exchange resolves Google userinfo through the same selected
  network context as token exchange and persists the returned email in both the
  normalized auth configuration and raw payload.
- A successful Codex reset-credit consumption decrements the locally projected
  available count exactly once under the existing credential-generation,
  reservation, and compare-and-set fences. A failed detail refresh preserves
  the last known count and item list while recording the detail failure.
- Pool Management, Provider Detail, and the Antigravity quota dialog use the
  shared quota summary. Gemini is one group; Claude and `gpt-*` share another.
  The group displays minimum remaining percent, a range when values differ,
  and the reset countdown belonging to the minimum window.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Key strong read fails, is missing, or Provider mismatches | Fail closed with catalog-state error; do not execute. |
| `concurrent_limit = 4`, four permits held | Fifth attempt is saturated and may fail over. |
| Limit missing, zero, or negative | No Key semaphore; existing Provider accounting may still apply. |
| All candidates saturated | Final 429 with capacity diagnostics. |
| Saturation plus any non-capacity skip | Final 503. |
| Cache-affinity hit is eligible | Use the sticky Key before secondary ordering. |
| Cache-affinity miss, mode `single_account` | Concentrate on the most recently used eligible account. |
| Cache-affinity miss, mode `lru` | Select the least recently used eligible account. |
| Matching model quota exhausted | Skip only that requested provider model. |
| Different model window exhausted | Keep the requested model eligible. |
| Reset timestamp malformed | Keep the quota fact, omit the countdown; do not invent a timestamp. |
| Antigravity quota refresh discovers a routable model | Import it with the exact refresh Endpoint ID. |
| Discovered model is internal or differs only by case from an excluded ID | Do not add it to the catalog. |
| Catalog synchronization fails after valid quota data arrives | Return quota success and emit a warning. |
| Codex reset consumption wins its reservation/generation fence | Decrement the projected count once. |
| Codex detail refresh fails | Preserve the previous count/items and mark detail failure. |

## 5. Good / Base / Bad Cases

- Good: four HTTP/WS turns share a Key limit of four; a fifth turn fails over,
  and cancelling one held turn immediately frees capacity.
- Base: a Key without a positive limit follows the existing Pool scheduling and
  Provider in-flight behavior.
- Good: Antigravity reports Gemini exhausted and Claude available; a Claude
  request remains schedulable and all three UIs show the same grouped summary.
- Bad: reading `concurrent_limit` from a stale local snapshot, using separate
  semaphores per protocol, treating every quota window as Provider-wide, or
  implementing three independent Antigravity family summaries.

## 6. Tests Required

- Runtime-state: keyed resources isolate capacity and permits release correctly.
- Gateway: sync, stream, Responses WS, and Live admission/release; saturation
  skip persistence; all-capacity 429 versus mixed-reason 503.
- Gateway fixtures must install the final `GatewayDataState` before registering
  local tunnel proxies because `with_data_state_for_tests` rebuilds
  `EmbeddedTunnelState` and discards earlier hub registrations.
- Provider Pool/Gateway: both cache-affinity secondary modes, existing balance
  and runtime-quota fallbacks, and strong-read Pool behavior.
- Provider Pool: model A exhaustion does not block model B for Antigravity and
  Codex model-scoped windows.
- OAuth/transport/admin: Antigravity legacy refresh token, refreshed credential
  persistence, Google userinfo email, local `models` projection, legacy
  `quota_by_model` reading, exact-Endpoint catalog import, and RFC3339 reset
  parsing.
- Admin/Gateway: Codex reset-credit activation and completion-order races,
  credential-generation replacement rejection, one-time local decrement, and
  failed-detail preservation.
- Frontend: concurrent-limit input/save, scheduling-mode metadata, shared quota
  summary, percentage/range, and countdown rendering.

## 7. Wrong vs Correct

### Wrong

```rust
// Separate protocol-local counters race and ignore other transports.
if HTTP_IN_FLIGHT.load(Ordering::Relaxed) >= key.concurrent_limit {
    return Err(capacity_error());
}
```

### Correct

```rust
let admission = acquire_provider_pool_execution_guard(state, plan).await?;
let ProviderPoolInFlightAdmission::Acquired(guard) = admission else {
    return retry_capacity_candidate();
};
// Keep `guard` alive for the complete execution/turn lifetime.
```
