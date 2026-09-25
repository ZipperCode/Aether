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
- A PoolGroup is an expansion entry, not its representative Key. Preselection
  must omit that Key's concurrency, zero-health and RPM facts while retaining
  Provider quota/concurrency and request authorization gates. Boolean admission
  and diagnostic admission share `current_candidate_runtime_skip_reason`.
- After strong catalog reads, `schedule_pool_page_candidates` applies the
  existing scheduler-core runtime checks to actual Keys, using current time,
  the existing recent-candidate window and Key RPM reset watermark. Run Pool
  quota/auth/balance/cooldown checks first to preserve their diagnostic priority.
  Filter actual-Key runtime denials before hot-pool fallback and window
  truncation, including sticky singleton candidates. Required counter-read
  failures retain `pool_key_state_unavailable`; do not invent quota exhaustion.
- An empty materialized score page must fall through to the catalog cursor.
  Inactive-Key score cleanup does not replace this fallback: prior stale scores
  or failed cleanup must not hide active unscored Keys. Keep bounded scan and
  ordering policies intact.
- Cache-affinity sticky hits remain first. On a miss, mode `single_account`
  concentrates work using reverse-LRU behavior; mode `lru` rotates to the
  least-recently-used Key. Unknown/missing mode uses `single_account`.
- A model-scoped quota window is evaluated only when its normalized `model`
  matches the requested provider model. Exhaustion for model A must not block
  model B on the same Key. Provider-wide windows retain their existing scope.
- Antigravity refresh must preserve refresh credentials and project each model
  window with reset aliases (`reset_at`, `next_reset_at`, `reset_time`,
  `next_reset_time`), accepting Unix and RFC3339 sources.
- Antigravity 额度刷新仅保存额度与上游模型发现元数据，不调用模型目录导入，不创建或修改 GlobalModel、ProviderModel 和 Endpoint 绑定。该边界适用于 OAuth 更新后、后台探测与手动刷新，避免已删除模型被自动恢复。
- 模型目录由管理员显式导入；导入仍使用发现结果中的准确 `endpoint_ids`，不以额度刷新代替用户确认。
- Antigravity OAuth exchange resolves Google userinfo through the same selected
  network context as token exchange and persists the returned email in both the
  normalized auth configuration and raw payload.
- A successful Codex reset-credit consumption decrements the locally projected
  available count exactly once under the existing credential-generation,
  reservation, and compare-and-set fences. A failed detail refresh preserves
  the last known count and item list while recording the detail failure.
- Pool Management, Provider Detail, and the Antigravity quota dialog use the
  shared account-window projection keyed by upstream `quota_group`, including
  weekly and five-hour periods. Keep each window's label, remaining percentage
  and reset countdown; do not rebuild the retired Gemini/Claude+GPT family
  minimum/range summary from individual model windows.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Key strong read fails, is missing, or Provider mismatches | Fail closed with catalog-state error; do not execute. |
| `concurrent_limit = 4`, four permits held | Fifth attempt is saturated and may fail over. |
| Limit missing, zero, or negative | No Key semaphore; existing Provider accounting may still apply. |
| All candidates saturated | Final 429 with capacity diagnostics. |
| Saturation plus any non-capacity skip | Final 503. |
| Representative Key has zero health, saturated concurrency or exhausted RPM | Keep its PoolGroup available for actual-Key expansion. |
| Actual hot/sticky Key fails a runtime guard | Record the exact reason and continue eligible cold/catalog Keys. |
| Only score rows are stale/inactive | Continue catalog scan instead of reporting premature exhaustion. |
| Cache-affinity hit is eligible | Use the sticky Key before secondary ordering. |
| Cache-affinity miss, mode `single_account` | Concentrate on the most recently used eligible account. |
| Cache-affinity miss, mode `lru` | Select the least recently used eligible account. |
| Matching model quota exhausted | Skip only that requested provider model. |
| Different model window exhausted | Keep the requested model eligible. |
| Reset timestamp malformed | Keep the quota fact, omit the countdown; do not invent a timestamp. |
| Antigravity 额度刷新发现模型 | 保留发现元数据，不写模型目录。 |
| 用户删除全局模型后再次刷新额度 | 不恢复该模型及关联记录。 |
| 用户显式导入模型 | 使用准确的发现 Endpoint 证据，沿用现有导入校验。 |
| Codex reset consumption wins its reservation/generation fence | Decrement the projected count once. |
| Codex detail refresh fails | Preserve the previous count/items and mark detail failure. |

## 5. Good / Base / Bad Cases

- Good: four HTTP/WS turns share a Key limit of four; a fifth turn fails over,
  and cancelling one held turn immediately frees capacity.
- Base: a Key without a positive limit follows the existing Pool scheduling and
  Provider in-flight behavior.
- Good: Antigravity reports Gemini exhausted and Claude available; a Claude
  request remains schedulable while the UIs share the same account-window data.
- Bad: reading `concurrent_limit` from a stale local snapshot, using separate
  semaphores per protocol, treating every quota window as Provider-wide, or
  implementing three independent Antigravity account-window projections.

## 6. Tests Required

- Gateway `runtime_key_guards_do_not_block_pool_group_representative` checks
  ordinary-Key denial, representative isolation and retained Provider gates.
- `pool_key_cursor_filters_real_key_runtime_guards_before_window_truncation`
  checks normal/sticky paths with denied Keys before an eligible Key beyond
  the frozen window and RPM-reset recovery.
- `pool_runtime_guards_preserve_hot_pool_fallback_and_quota_reasons` checks
  hot-to-cold fallback, all-denied outcome and exact quota/infrastructure reasons.
- Retain #824 stale inactive score regressions and fixed-order stream tests.

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
- OAuth/transport/admin：保留 Antigravity 刷新凭据、Google userinfo email、`models` 投影、旧 `quota_by_model` 读取和 RFC3339 reset 解析覆盖；额度刷新不得修改模型目录，删除后刷新不得恢复模型，显式导入仍保留准确 Endpoint 绑定。
- Admin/Gateway: Codex reset-credit activation and completion-order races,
  credential-generation replacement rejection, one-time local decrement, and
  failed-detail preservation.
- Frontend: concurrent-limit input/save, scheduling-mode metadata, shared
  `quota_group` windows, weekly/five-hour labels, percentage and countdown.

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
