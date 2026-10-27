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

The current Antigravity admin refresh writes model entries under
`upstream_metadata.antigravity.quota_by_model`, with upstream reset values in
`reset_time`. Scheduling selects the first object in `quota_by_model`, `models`,
`model_quotas` order; even an empty object is authoritative. Explicit `windows`
and `additional_quota_windows` remain independent inputs. Do not merge stale
legacy model maps into the current result or migrate the persisted API shape.

Antigravity account badges read `upstream_metadata.antigravity.subscription_tier`
(`free | pro | ultra | null`). This is display-only: keep
`status_snapshot.quota.plan_type` null because `derive_plan_tier` consumes it for
scheduling. Existing accounts acquire the badge on the next quota refresh.

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
- Model quota is resolved against each serving account's actual upstream model,
  not a client alias or another account's model list. Candidate snapshots use
  `BTreeMap<key_id, BTreeMap<selected_provider_model_name, bool>>` so one account's
  multiple candidate models cannot overwrite each other's decisions.
- Antigravity model identities use trimmed, full `eq_ignore_ascii_case` matching
  across explicit scalar/array model aliases. Versions, thinking suffixes and
  Pro/Flash names are independent; no token, punctuation or family inference.
  Other providers retain their existing matching rules.
- Multiple exact-model windows retain `all` exhaustion semantics. An unknown
  applicable window prevents a known exhausted sibling window from proving
  exhaustion; all-unknown windows alone remain unresolved. Missing, empty and
  malformed model quota is unknown, not a default exhausted state.
- Antigravity display-only `quota_group` windows do not infer model relationships
  or override exact model windows. Structural generic account windows and the
  adapter's account snapshot fallback remain available.
- Antigravity `quota_hard_blocked` derives from the same resettable
  `quota_exhausted` decision. Ordinary and real Pool/sticky Keys reject known
  exhausted models for either `ignore_exhausted_accounts` value, using
  `account_quota_exhausted` / `pool_account_exhausted`. Never persist this as the
  independent manual-recovery `scheduling.quota_exhausted` state. Exhausted
  accounts remain eligible for background quota probes and self-checks.
- Raw metadata and materialized snapshots preserve source-time precedence and
  reevaluate reset deadlines at selection. Parse `reset_at`, `next_reset_at`,
  `reset_time`, `next_reset_time`, then existing relative seconds, trying later
  aliases after invalid values. Accept Unix/RFC3339; elapsed resets release only
  the resettable guard, and invalid deadlines never fabricate recovery.
- After `persist_provider_quota_refresh_state` returns true and the model refresh
  status is `success`, call `invalidate_provider_quota_candidate_caches` before
  returning the result. Preserve affinity mappings; sticky Keys still pass the
  same real-Key guards. Failed fetch, `no_metadata`, account-only observations,
  CAS conflicts and persistence errors do not claim quota recovery. Manual
  scheduling bans, disablement, OAuth, health and cooldown remain independent.
- Antigravity 额度刷新仅保存额度与上游模型发现元数据，不调用模型目录导入，不创建或修改 GlobalModel、ProviderModel 和 Endpoint 绑定。该边界适用于 OAuth 更新后、后台探测与手动刷新，避免已删除模型被自动恢复。
- 模型目录由管理员显式导入；导入仍使用发现结果中的准确 `endpoint_ids`，不以额度刷新代替用户确认。
- Antigravity OAuth exchange resolves Google userinfo through the same selected
  network context as token exchange and persists the returned email in both the
  normalized auth configuration and raw payload.
- Antigravity quota refresh probes `loadCodeAssist` once for project and tier,
  including accounts with an existing project. `paidTier` precedes `currentTier`;
  nonempty ID precedes name. Match whole identifier words: ultra/helium,
  free/starter, pro/premium/advanced. `standard-tier`, missing evidence and
  available upgrade options remain unknown; never infer membership from models.
- Discovery failure preserves the old tier in storage and refresh-response
  metadata, and a known project permits quota refresh to continue. Successful
  unknown evidence writes null to clear stale badges. Discovery uses canonical
  `antigravity` origin policy, selected proxy, quota timeouts and disabled
  redirects even when the provider has a custom display name.
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
| Antigravity target exhausted, sibling version/thinking/tiered model available | Block only the exact target for both ignore-switch values. |
| Authoritative `quota_by_model = {}` plus legacy exhausted `models` | Do not resurrect the legacy model map. |
| Exact model has exhausted and unknown independent windows | Preserve fail-open `all` semantics; do not discard the unknown window. |
| Raw `reset_time` / `next_reset_time` elapsed | Same recovery as the materialized snapshot without waiting for cache TTL. |
| Successful persisted model refresh, exhausted to available | Recompute candidate and resolved pages on the next selection; preserve affinity. |
| `fetchAvailableModels` fails but `loadCodeAssist` succeeds | Preserve old quota and never restore an exhausted model from the account observation. |
| Positive refresh with manual ban or disabled Key | Keep the Key ineligible. |
| All Antigravity target-model accounts exhausted | HTTP 503 and zero generation requests; background probes remain eligible. |
| Antigravity 额度刷新发现模型 | 保留发现元数据，不写模型目录。 |
| 用户删除全局模型后再次刷新额度 | 不恢复该模型及关联记录。 |
| 用户显式导入模型 | 使用准确的发现 Endpoint 证据，沿用现有导入校验。 |
| Codex reset consumption wins its reservation/generation fence | Decrement the projected count once. |
| Codex detail refresh fails | Preserve the previous count/items and mark detail failure. |
| Antigravity paid Pro with current Free | Display Pro; paid evidence wins. |
| Antigravity discovery fails with cached Ultra/project | Continue quota refresh and retain Ultra in response and storage. |
| Antigravity discovery succeeds with unknown/missing tier | Display unknown and clear the old tier; never default Free. |

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
- Good: paid Ultra and current Free renders Ultra in provider/Pool lists and
  quota detail without changing Pool ranking.
- Bad: `allowedTiers` contains Pro, or a Claude model exists, so classifying the
  account as Pro; neither proves an active subscription.

## 6. Tests Required

- Gateway `runtime_key_guards_do_not_block_pool_group_representative` checks
  ordinary-Key denial, representative isolation and retained Provider gates.
- `pool_key_cursor_filters_real_key_runtime_guards_before_window_truncation`
  checks normal/sticky paths with denied Keys before an eligible Key beyond
  the frozen window and RPM-reset recovery.
- `pool_runtime_guards_preserve_hot_pool_fallback_and_quota_reasons` checks
  hot-to-cold fallback, all-denied outcome and exact quota/infrastructure reasons.
- Retain #824 stale inactive score regressions and fixed-order stream tests.
- `antigravity_sibling_model_does_not_mask_exhausted_target_model` checks both
  exhaustion directions for versions, thinking and tiered Pro/Flash siblings,
  scalar/array aliases, and hard-block parity.
- Antigravity authoritative-map, malformed-window, reset-alias and freshness
  regressions cover empty-map precedence, unknown-window `all` semantics,
  metadata/snapshot parity, malformed reset retention and `0 <-> 0.5` updates.
- `antigravity_same_key_multi_model_quota_preserves_each_model_across_orders_and_switches`
  checks per-account actual-model isolation, both candidate orders/ignore flags,
  PoolGroup representative exemption and manual-ban priority. The non-Antigravity
  counterpart uses disjoint model tokens to preserve legacy switch semantics.
- `pool_key_cursor_antigravity_quota_refresh_recovers` uses actual catalog quota
  with normal/sticky cursors, both ignore flags, denied Keys beyond the frozen
  window, immediate recovery and no forced first member when all are exhausted.
- `antigravity_exhausted_keys_remain_eligible_for_quota_probe_while_manual_ban_is_excluded`
  checks Base/Burst selection and active-probe eviction of only the manual ban.
- `gateway_antigravity_quota_refresh_restores_serving` runs a real localhost
  gateway and execution endpoint in ordinary and Pool/cache-affinity modes.
  Assert native Gemini final text and separate per-account probe/generation
  counts for warm A, exhausted A -> B, refreshed A/B exhausted -> A, failed quota
  fetch, manual ban, disable/re-enable and all-exhausted HTTP 503. No restart,
  TTL wait, production credentials or external Google/database service.

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
- OAuth/transport/admin: preserve Antigravity refresh credentials, Google userinfo
  email, current `quota_by_model` projection, legacy `models` input and RFC3339
  reset parsing. Quota refresh never changes the model catalog or resurrects
  deleted models; explicit import retains exact Endpoint evidence.
- Admin/Gateway: Codex reset-credit activation and completion-order races,
  credential-generation replacement rejection, one-time local decrement, and
  failed-detail preservation.
- Frontend: concurrent-limit input/save, scheduling-mode metadata, shared
  `quota_group` windows, weekly/five-hour labels, percentage and countdown.
- Antigravity parser: paid/current precedence, blank identifiers, aliases,
  unrecognized evidence and project variants. Gateway HTTP refresh: Pro to Free
  to Ultra, failed discovery retention, successful unknown clearing, unchanged
  scheduling plan and model catalog. UI: all three badges plus unknown in
  provider/Pool desktop/mobile and quota detail.

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

For account tier display, wrong: `quota.plan_type = subscription_tier` (changes
Pool scoring). Correct: persist `antigravity.subscription_tier` and render it
through the shared frontend account-tier resolver.

For Antigravity model quota, wrong: match shared `claude` tokens, merge current
and historical model maps, and cache the result under only `key_id`. An available
sibling then hides the requested exhausted model. Correct: select the authoritative
map, match `candidate.trim().eq_ignore_ascii_case(requested.trim())`, preserve
unknown exact windows in the `all` decision, and index the actual key/model pair.
After a successful persisted model refresh, invalidate candidate pages, not
affinity or independent manual-recovery scheduling state.
