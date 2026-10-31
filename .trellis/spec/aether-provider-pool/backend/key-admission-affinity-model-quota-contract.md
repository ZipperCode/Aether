# Key Admission, Affinity, and Model Quota Contract

## 1. Scope / Trigger

Use this contract when changing Provider Key `concurrent_limit`, runtime
semaphores, sync/stream/WebSocket admission, Pool cache affinity, model quota
windows, Antigravity discovered-model admission and dynamic thinking variants,
runtime quota feedback and quota projection, Codex reset-credit state, or
quota-driven model discovery.
It keeps every transport on the same atomic Key capacity and separates model availability from quota.

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

pub fn provider_pool_key_supports_discovered_model(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: &str,
) -> bool;

pub enum AntigravityThinkingLevel { Minimal, Low, Medium, High }
pub fn antigravity_request_thinking_level(
    api_format: &str,
    body: &serde_json::Value,
    requested_model: &str,
) -> Option<AntigravityThinkingLevel>;
pub fn antigravity_model_thinking_variant(
    model: &str,
) -> Option<(&str, AntigravityThinkingLevel)>;
pub fn resolve_provider_pool_key_thinking_model<'a>(
    key: &'a StoredProviderCatalogKey,
    provider_type: &str,
    requested_model: &str,
    selected_provider_model: &'a str,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> Option<&'a str>;

pub(crate) async fn antigravity_model_quota_blocked_many_with_auth_configs(
    runtime: &RuntimeState,
    candidates: &[AntigravityModelQuotaCandidate<'_>],
    auth_configs: &[Option<&str>],
    now_unix_secs: u64,
) -> Result<Vec<bool>, GatewayError>;

pub(crate) async fn capture_antigravity_model_quota_refresh(
    state: &AppState,
    serving_transport: &GatewayProviderTransportSnapshot,
) -> Result<Option<AntigravityModelQuotaRefreshCapture>, GatewayError>;

pub(crate) async fn reconcile_antigravity_model_quota_refresh(
    state: &AppState,
    capture: &AntigravityModelQuotaRefreshCapture,
    upstream_metadata: Option<&serde_json::Value>,
) -> Result<(), GatewayError>;
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
- Antigravity discovered-model admission supplements, never replaces, existing
  account allowlists and scoped Provider mappings. Strongly read the actual Key
  and require the selected upstream model to match a key in its authoritative
  `upstream_metadata.antigravity` model map. Use the same first-object precedence
  as quota projection (`quota_by_model`, `models`, `model_quotas`), including empty
  maps; ignore stale lower-priority maps and explicitly disabled entries.
  Missing/malformed model maps preserve pre-discovery behavior; other Provider
  types are unchanged. A null allowlist or disabled automatic fetching cannot
  override a discovered catalog excluding the target. Do not infer subscription
  privileges or another account's model availability. Dynamic thinking selection
  below is the only recognized-suffix family operation; discovery/quota admission
  still compares the resulting complete identity exactly.
- Ordinary candidates and actual Pool/sticky members share the borrowed
  `provider_pool_key_supports_discovered_model` decision and emit
  `key_model_disabled` after existing higher-priority runtime denials. PoolGroup
  representative discovery never removes the logical group; filter real members
  before hot-pool fallback/window truncation. Model absence is not exhausted
  quota and never writes a persistent scheduling ban.
- Model quota is resolved against each serving account's actual upstream model,
  not a client alias or another account's model list. Candidate snapshots use
  `BTreeMap<key_id, BTreeMap<selected_provider_model_name, bool>>` so one account's
  multiple candidate models cannot overwrite each other's decisions.
- Antigravity model identities use trimmed, full `eq_ignore_ascii_case` matching
  across explicit scalar/array model aliases. Versions, thinking suffixes and
  Pro/Flash names are independent; no token, punctuation or family inference.
  Other providers retain their existing matching rules.
- Antigravity dynamic thinking is resolved from each serving Key's first
  authoritative map before discovery/quota snapshots and Pool windows. Decode
  native Gemini `generationConfig.thinkingConfig.thinkingLevel`, Chat
  `reasoning_effort`, or Responses `reasoning.effort` after client routing
  mutations. `xhigh/max/extra` normalize to high; `none/minimal/extra-low` prefer
  a discovered minimal variant, using low only when no minimal variant exists.
  Never bypass a disabled minimal variant or approximate missing medium/high.
- Native Gemini without a recognized `thinkingLevel` also decodes integer
  `thinkingBudget` through the shared budget ladder: `0..=1664` -> low,
  `1665..=3072` -> medium, `>=3073` -> high; dynamic `-1` -> medium.
  Accept snake_case aliases and existing private envelopes. Explicit level and
  client model suffix retain precedence; other negative budgets select no tier.
- Only `gemini-` terminal effort suffixes form variant families; preserve exact
  family/version identity. Explicit client suffix takes precedence over body
  effort; a mapped default suffix is not a client choice. Bare Gemini names
  without format-native thinking settings default to medium before preselection,
  strong-Key permission and quota admission, regardless of the mapped default.
  Explicit malformed settings are not replaced by this default. Return only
  discovered IDs; a missing/disabled medium never falls back to high/low.
- OpenAI null reasoning fields are unconfigured: Chat `reasoning_effort: null`,
  Responses `reasoning: null` and `reasoning.effort: null` use the same bare
  Gemini medium default; explicit non-null malformed values do not.
- Absent discovery and base-only catalogs retain the actual selected model
  identity. Base-only Gemini 3 private envelopes use canonical medium when
  settings are absent; legacy Gemini 2.5 budget-only wire requests remain
  unchanged. Claude and unrelated providers retain their existing behavior.
- Native base-only Gemini 3 also projects explicit client suffixes before
  endpoint body rules and directive patches. Defer those rules only for this
  narrow suffix case and execute them once; never enter canonical conversion or
  enable global model directives to recover an Antigravity suffix.
- Converted OpenAI requests retain the frozen Antigravity selection across
  conversion. Recover omitted compatible tiers and client suffix overrides on
  base-only Gemini 3 before endpoint body rules/directive patches; never restore
  them afterward. At the private-envelope boundary an unresolved explicit
  source setting with no effective target tier is `EnvelopeUnsupported`, not
  implicit medium. Presence of malformed target fields is not absence; do not
  repair those containers or alter the pure-conversion/OpenAI pass-through
  contract to enforce this provider-specific default.
- Converted response finalizers retain the client's recognized directive suffix
  as the display model. For a base-only catalog, assert that client alias
  separately from the exact bare upstream identity and its selected thinking
  level; do not change the established response-model restoration contract.
- Static mapping cannot pre-reject a low-only Key against a default-high alias.
  Defer row `key_allowed_models` only for effort-sensitive Antigravity Gemini
  rows whose possible mapping targets are all Gemini. Then the unavoidable
  strong-Key resolver authorizes the actual target or explicitly allowed client
  alias, including unchanged targets. Null is unrestricted; empty denies all.
  Ordinary missing-Key snapshots skip with `key_quota_state_unavailable`, not
  model-disabled/exhausted, so an eligible sibling can continue.
- Re-resolve on every account change, including sticky before reserve-minimum
  quota. Effort-bearing requests bypass resolved-page caches; source-row caches
  remain shared. Keep selected candidate, plan, usage and quota attribution on
  the same exact upstream model. The private-envelope boundary canonicalizes
  Gemini thinking level from that model in fresh/pre-enveloped bodies while
  retaining numeric budgets and unrelated fields; native format still bypasses
  conversion.
- Multiple exact-model windows retain `all` exhaustion semantics. An unknown
  applicable window prevents a known exhausted sibling window from proving
  exhaustion; all-unknown windows alone remain unresolved. Missing, empty and
  malformed model quota is unknown, not a default exhausted state.
- Antigravity model `quotaInfo` is normalized by
  `aether_provider_transport::antigravity::antigravity_quota_info_remaining_fraction(&serde_json::Map<String, Value>) -> Option<f64>`.
  Both admin quota refresh and model discovery use this function. Explicit
  `remainingFraction` numbers or trimmed numeric strings must be finite and are
  clamped to `[0, 1]`; explicit invalid/null values remain unknown, even with a
  valid reset. Only an absent fraction plus a trimmed RFC3339 string `resetTime`
  with nonnegative Unix seconds means observed zero remaining (upstream omits
  zero scalar values). Empty/missing quotaInfo and malformed/non-string resets
  remain unknown. Do not compare current time in this parser: preserve zero and
  reset, and let the existing selection deadline release elapsed observations.
  No group matching, plan inference, cross-bucket fallback or account-wide ban
  is introduced. Historical reset-only records converge after a successful
  normal quota refresh/model discovery, not by replacing the binary alone.
- Antigravity `quota_group` windows do not infer model relationships. An
  administrator can explicitly bind each account's exact discovered upstream
  model to one saved quota group with a binding-only
  `PUT /api/admin/endpoints/keys/{id}` body:
  `{"antigravity_quota_bindings":{"exact-model":"opaque-group-key"}}`.
  Empty object clears bindings; mixed configuration writes, unknown/disabled
  account models, unknown groups, and duplicate normalized model identities are
  rejected. Store the mapping in its own `upstream_metadata.antigravity_quota_bindings`
  namespace using CAS, so normal quota/model refresh cannot overwrite it.
  System configuration export/import preserves the optional per-Key
  `antigravity_quota_bindings` map, including an explicit empty map. Import
  validates its structure before any Key write; refreshed runtime quota remains
  independent and is not replaced by restoring administrator bindings.
- `antigravity_quota_group_key(&Map<String, Value>) -> Option<String>` uses
  `id:<group_id>` when available, otherwise `buckets:<JSON sorted unique bucket IDs>`.
  Group ordering/display names do not change identity. No model/tier/prefix
  inference or cross-pool fallback: Pro/Ultra and Argon associations are account
  configuration, not static routing policy. A missing binding/group preserves
  existing exact-model admission rather than guessing a replacement pool.
- Within an explicitly bound group, enabled buckets are cumulative constraints:
  any known exhausted bucket whose reset has not elapsed blocks that model.
  An unknown sibling never hides a known exhausted window. `disabled:true`
  buckets are ignored and project `is_included:false`; UI shows not applicable,
  never a consumable percentage. Partial reset cannot release a still-exhausted
  weekly/short sibling. This group guard is ORed with existing exact-model
  exhaustion, so a positive shared group never restores an exhausted exact model.
  Positive refresh or all applicable exhausted deadlines elapsed restores group
  eligibility; independent authentication/manual guards still apply. Other
  providers' independent exact-model `all` semantics are unchanged.
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
- Antigravity refresh failures preserve `oauth_invalid_at` and
  `oauth_invalid_reason`; missing project, transport errors and `no_metadata`
  never run success-state clearing. Only a successfully parsed and persisted
  model quota response clears recoverable credential errors through the existing
  success policy. Preserve `[ACCOUNT_BLOCK]` even after successful quota probes.
- Admin refresh and runtime generation share
  `aether_admin::provider::quota::antigravity_oauth_invalid_reason(u16, Option<&Value>) -> Option<String>`.
  HTTP 401 and explicit `UNAUTHENTICATED` use `[OAUTH_EXPIRED] oauth_token_invalid`
  so existing ordinary/Pool hard-invalid guards reject the actual credential;
  an ambiguous expired-token label alone is recoverable and does not guarantee
  exclusion. HTTP 400/403 with `VALIDATION_REQUIRED`, `ACCOUNT_DISABLED`,
  `ACCOUNT_SUSPENDED` ErrorInfo reasons or explicit account verification/disabled/
  suspended messages use sanitized `[ACCOUNT_BLOCK]` reasons. Generic parameter,
  model-disabled and project errors do not create runtime account blocks.
  Keep existing runtime OAuth taxonomy detection and refresh-only generic 403
  forbidden handling. Credential-fenced persistence and bearer matching remain
  mandatory; no database migration or production account mutation is implied.
- Runtime Antigravity quota observations use shared RuntimeState namespace
  `aether:antigravity:model-quota:v1`, indexed by hashed Provider/actual Key,
  credential generation and exact upstream model. The internal record contains
  `version`, `provider_id`, `key_id`, `credential_generation`, `model`,
  `deadline_unix_secs`, unique `observation_id` and `source`; no API/schema change.
  OAuth refresh-token identity survives access-token rotation; credential
  replacement fences out old observations. Access-token-only credentials use
  the existing full credential fence.
- Only Antigravity HTTP 429 with structured `QUOTA_EXHAUSTED`/`QUOTA_EXCEEDED`
  detail reasons or quota-reset metadata enters this model-only path. Generic
  `RESOURCE_EXHAUSTED`, explicit rate/RPM evidence, account/auth failures and
  other Providers retain existing behavior. Administrator quota rules take
  precedence. An explicitly named model must match the selected upstream model
  after trimming and ASCII case-insensitive comparison; a mismatch records no
  scoped fact. Missing model metadata uses the actual selected model, never a
  family, client alias or another account's catalog.
- Deadlines prefer parsed upstream reset metadata, then numeric `Retry-After`,
  then configured quota cooldown (300 seconds when zero), bounded to 1 second
  through 31 days. Ordinary/Pool/sticky admission reads the shared fact even
  with cached candidates, before fallback/window truncation. Use existing
  `account_quota_exhausted` / `pool_account_exhausted` reasons for the affected
  pair; backend errors use `key_quota_state_unavailable` /
  `pool_key_state_unavailable`. One credential decryption failure affects only
  its descriptors. PoolGroup representatives do not consume real-Key facts.
- Scoped errors retain failed-attempt diagnostics and release Pool leases, but
  never write account cooldown, Pool `QuotaExhausted`, health/circuit, adaptive
  RPM, OAuth invalidation or manual-recovery scheduling bans. Recording
  invalidates local candidate pages and affinity cache; positive refresh keeps
  affinity mappings and independent manual restrictions intact.
- Capture scoped observations before a quota fetch. After successful persisted
  refresh, strongly re-read the credential and positive exact-model metadata
  under the Provider/Key quota projection lock; clear only captured records
  still byte-identical and positive in both incoming and durable metadata.
  Unique observation IDs preserve newer same-second failures. Failed fetch,
  observation-only patches, CAS conflicts, stale refresh and unrelated model
  success do not clear an active fact. Elapsed deadlines release only this
  resettable guard; replacement credentials cannot match the old generation.
- Credential-fenced quota persistence sends the already-merged provider
  namespace, not the raw observation-only patch: memory/PostgreSQL perform a
  top-level namespace merge. A failed-fetch account observation must preserve
  existing `quota_by_model`, project and unrelated namespaces; retain namespace
  CAS conflict detection and credential fencing.
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
| Antigravity absent fraction with valid nonnegative RFC3339 reset | Persist remaining 0 / used 100; block only that exact model until reset. |
| Antigravity explicit invalid fraction with valid reset | Keep remaining/used unknown; never apply the omitted-zero fallback. |
| Raw `reset_time` / `next_reset_time` elapsed | Same recovery as the materialized snapshot without waiting for cache TTL. |
| Successful persisted model refresh, exhausted to available | Recompute candidate and resolved pages on the next selection; preserve affinity. |
| Antigravity structured `QUOTA_EXHAUSTED`/`QUOTA_EXCEEDED` or quota-reset 429 | Record only the actual Key and selected upstream model in shared RuntimeState; skip that exact pair on the next ordinary/Pool/sticky selection. |
| Antigravity generic `RATE_LIMIT_EXCEEDED`, per-minute or RPM 429 | Preserve account-level cooldown, health, adaptive RPM and existing Pool feedback; do not create a model-only fact. |
| Scoped model-quota fact active with a sibling model on the same Key | Keep the sibling model eligible immediately; do not reuse account-wide cooldown or Pool `QuotaExhausted`. |
| Scoped model-quota fact active with the same model on another Key | Continue with the eligible Key/account. |
| Runtime quota state read fails or returns a malformed batch | Mark the affected Key/model state unavailable; never report it as exhausted or generate against a known blocked pair. |
| Credential generation changes before effect persistence | Reject the stale observation; do not block the replacement credential. |
| Failed or unrelated quota refresh | Retain the active exact-model fact and continue unrelated model serving. |
| Matching persisted refresh succeeds or the scoped deadline elapses | Release only the exact Key/model guard; preserve independent manual bans/cooldowns. Positive refresh preserves affinity. |
| All eligible accounts have the requested exact-model fact active | Return HTTP 503 with zero generation attempts for that model; background quota probes remain eligible. |
| `fetchAvailableModels` fails but `loadCodeAssist` succeeds | Preserve old quota and never restore an exhausted model from the account observation. |
| Explicitly bound group has weekly zero / short positive, or the reverse | Block only models bound to that account/group; positive sibling does not mask the exhausted limit. |
| Short reset elapsed but weekly exhaustion reset is future | Keep the bound model excluded. |
| Disabled zero bucket with all enabled buckets available | Ignore disabled bucket; show not applicable, no progress percentage/bar. |
| No explicit binding or the saved group identity disappears | Retain exact-model behavior; require explicit reassociation rather than model-prefix guessing. |
| Antigravity quota HTTP 401 | Persist hard-invalid Token rejection; display abnormal account and skip ordinary/Pool/sticky serving. |
| Failed quota refresh after an authentication failure | Preserve the original reason/timestamp; do not restore serving. |
| Successful quota refresh after Token rejection | Clear recoverable credential failure and immediately restore eligible serving. |
| Runtime 400/403 with structured `VALIDATION_REQUIRED` | Persist account-verification block; subsequent sync/SSE skips that account even after successful quota probes. |
| Ordinary parameter 400 without account evidence | Do not invent an account or Token block. |
| Positive refresh with manual ban or disabled Key | Keep the Key ineligible. |
| All Antigravity target-model accounts exhausted | HTTP 503 and zero generation requests; background probes remain eligible. |
| Antigravity null allowlist and fetched map excludes target | Skip that actual Key with `key_model_disabled`; zero generation requests. |
| Map contains only `gemini-4-argon-disabled`, target is `gemini-4-argon-high` | No family inference; reject that account and continue eligible siblings. |
| Authoritative model map is empty or exact entry is disabled | Reject target without changing quota state or importing models. |
| Model maps are absent/malformed, or Provider is not Antigravity | Preserve existing pre-discovery/allowlist behavior. |
| Sticky account loses target after successful discovery refresh | Re-evaluate strong Key evidence, skip old binding and serve/rebind an eligible sibling. |
| All accounts exclude target in discovery | HTTP 503 and zero upstream generation requests. |
| Alias defaults to high, client requests low | Select that Key's discovered low before quota filtering; high exhaustion must not block low. |
| Gemini has only legacy numeric budget | Resolve its normalized tier before account admission; do not retain default-high solely because `thinkingLevel` is absent. |
| Bare Gemini name, no native thinking setting, default mapping bare/high | Resolve current account's discovered medium before admission; preserve explicit suffix/level/budget overrides. |
| OpenAI reasoning field absent or null on bare Gemini | Resolve discovered medium, including warm ordinary/Pool requests and converted completion. |
| Medium missing/disabled/exhausted on preferred account | Skip that account, serve eligible sibling medium; all ineligible returns 503 with no unsupported generation. |
| Requested tier missing/disabled on an account | `key_model_disabled`; continue another account, never silently choose another tier. |
| A allows only high, B only low; alias defaults high and effort is low | Reject A, defer static permission on B, authorize B's resolved low and serve/rebind B. |
| Client exact low model conflicts with body high | Keep discovered low identity and canonical low thinking level. |
| Strong ordinary Key is absent during effort selection | `key_quota_state_unavailable`; continue eligible siblings without cached-default execution. |
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
- Good: a null-allowlist account exposes only a disabled Argon placeholder and is
  skipped, while a sibling exposing the exact mapped `-high` model serves native
  Gemini even when its automatic model-fetch toggle is disabled.
- Bad: treating unknown quota as proof of model entitlement, or allowing a null
  allowlist to erase an account's fetched model restriction.
- Good: a stable `gemini-3.8-flash` alias maps to high but no-effort requests
  select discovered medium; explicit low selects low and xhigh/max select high.
- Good: native Gemini `thinkingBudget=1280/2048/16384/-1` selects the same
  account-local low/medium/high/medium models as explicit thinking levels.
- Bad: rewrite only the envelope after default-high quota admission, or clear a row
  allowlist without enforcing the actual strong-Key target afterward.

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
- `gateway_antigravity_runtime_quota_isolation` runs real localhost ordinary and
  Pool/cache-affinity sync/native-SSE flows in both directions. Assert real
  upstream quota 429, zero repeated generation on the affected Key/model,
  sibling-model serving on the same Key, affected-model fallback to another
  account, positive-refresh recovery, failed-refresh retention, native SSE
  `STOP`/exact text/model identity and zero account-wide penalties.
- `gateway_antigravity_generic_rate_limit_remains_account_scoped` asserts
  explicit `RATE_LIMIT_EXCEEDED` retains cooldown, health, adaptive RPM and
  Pool feedback rather than becoming model-only quota evidence.
- Runtime regressions assert expiry, OAuth rotation/replacement fences,
  same-second refresh/failure ordering, positive exact-model intersection,
  per-account decryption-error isolation, batch-read failure diagnostics and
  ordinary/Pool/sticky filtering before truncation.
- `antigravity_discovered_model_admission_uses_exact_account_evidence` covers
  exact identities, disabled flags, authoritative empty-map precedence, missing
  discovery fallback, and unchanged non-Antigravity behavior.
- `runtime_antigravity_discovered_model_denies_real_key_but_not_pool_group_representative`
  and `pool_key_cursor_filters_antigravity_undiscovered_model_before_window_truncation`
  cover ordinary denial, representative isolation, normal/sticky member scans,
  eligible members beyond the frozen window, and existing skip-reason priority.
- `gateway_antigravity_account_scoped_argon_admission` exercises real localhost
  sync and public `streamGenerateContent?alt=sse` HTTP in ordinary/Pool modes.
  Assert zero unsupported generations, exact mapped model, native SSE `STOP`,
  refreshed discovery rebinding, and all-unsupported HTTP 503 with no generation.
- `gateway_antigravity_dynamic_effort_*` exercises ordinary/Pool HTTP for native
  Gemini sync/SSE and converted Chat/Responses: no-effort/null medium with
  bare/high mappings, warm alternating tiers, explicit suffix priority,
  canonical xhigh/max, missing/disabled/exhausted medium, scoped high quota
  feedback and high-only/low-only Key allowlists. Assert completion text,
  exact envelope model/level, preserved native custom fields and zero unsupported
  upstream calls; all-medium-ineligible requests return 503 without generation.
  Runtime unit coverage retains an eligible sibling when the preferred Key is
  absent from the strong snapshot.
- `gateway_antigravity_dynamic_effort_legacy_budget_alias_variants` asserts
  ordinary/Pool HTTP completion model and outbound level for numeric budgets
  and `-1`; the transport regression covers snake_case and level precedence.
- `gateway_antigravity_dynamic_effort_converted_base_only_invalid_effort_rejected`
  asserts zero upstream generation for unresolved `ultra`, accepted `extra-low`
  on the discovered bare identity, and client low suffix over conflicting valid
  or invalid body effort. Check both completion semantics and the exact outbound
  thinking level in ordinary and Pool modes.
- `gateway_antigravity_dynamic_effort_native_base_only_explicit_suffix_priority`
  disables conversion and exercises ordinary/Pool native sync/SSE. Assert bare
  default medium, explicit client low over body high, exact bare upstream model,
  restored client display alias, native STOP/text and preserved unknown fields.

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
- Transport `antigravity_quota_info_*`: finite numeric/string fractions, zero,
  clamp bounds, valid future/past/epoch resets, invalid/non-string/pre-epoch
  resets, and explicit null/empty/invalid/nonfinite fractions with valid reset.
  Admin `parses_antigravity_reset_only_quota_without_exhausting_unknown_models`
  and model-fetch `antigravity_models_reset_only_quota_without_exhausting_unknown_models`
  assert zero/100/reset preservation and keep no-quota/empty/invalid-reset
  models unknown. Preserve `antigravity_models_without_explicit_quota_are_not_marked_exhausted`.
- Gateway `gateway_antigravity_reset_only_quota_refresh_restores_serving` binds
  loopback HTTP and exercises real admin refresh plus native Gemini MEDIUM
  sync/SSE with format conversion disabled. For ordinary and Pool/sticky paths,
  assert exact account/model generation counts: preferred A warmup, reset-only
  A failover to B, available A Flash despite exhausted Argon and B sticky,
  all-Argon exhaustion 503 with no execution, immediate positive-refresh
  recovery, and elapsed-reset recovery while B retains a future deadline.
- Gateway `gateway_antigravity_auth_failure_refresh_blocks_serving`: real HTTP
  warmup, admin 401 refresh, abnormal status projection, sync/SSE failover with
  unchanged invalid-account generation counts, failed 400/502 preservation,
  all-invalid 503, positive-refresh recovery, ordinary 400 non-blocking and
  refresh 403 exclusion in both ordinary and Pool/sticky modes.
- Gateway `gateway_antigravity_runtime_account_verification_blocks_serving`:
  structured verification 400 through sync and SSE, persisted account-verification
  status, no further execution on the blocked account, and continued exclusion
  after successful quota refresh. Admin classifier tests distinguish explicit
  account restrictions from parameter/model/project errors without retaining
  upstream secrets.
- Provider Pool `antigravity_joint_quota_tests` asserts cumulative window
  blocking, unknown siblings, invalid reset retention, disabled exclusion,
  elapsed resets, stable group identity, no-binding fallback and exact-model
  exhaustion precedence.
- Gateway `gateway_antigravity_joint_group_quota_restores_serving` uses real
  loopback admin PUT/refresh and native Gemini sync/SSE ordinary/Pool/sticky
  requests. Assert validation errors, exact account/model execution deltas,
  both exhausted-window directions, partial resets, unrelated models, all-blocked
  503, disabled windows, elapsed deadlines and clear-binding recovery. Cover
  group-ID and sorted opaque bucket-set identities through safe API projection.
- Browser smoke must operate the real OAuth account editor to select an exact
  model/group, save only the binding field, clear it, and render disabled quota
  as not applicable without a progress bar. No new hardcoded model lists.
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

For runtime Antigravity quota, wrong: turn every `RESOURCE_EXHAUSTED` into
account-wide cooldown or Pool `QuotaExhausted`. Correct: require structured
model-quota evidence, store a finite Provider/Key/credential/model fact, and
consume it in ordinary/Pool/sticky admission. Clear only captured matching
positive refresh observations or let the deadline expire; keep generic rate
limits and administrator-recoverable account bans independent.

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

For reset-only parsing, wrong: require `remainingFraction` to observe exhaustion,
or `unwrap_or(0.0)` for every missing/invalid quota. Correct: normalize only
absent fraction plus valid RFC3339 reset through the shared transport parser;
preserve explicit invalid values as unknown and leave reset expiry to selection.

For discovered-model admission, wrong: `allowed_models == null` therefore any
Provider mapping is valid for that account. Correct: keep existing mapping and
allowlist rules, then apply the shared discovered-model guard to the actual
Key and selected upstream model. Preserve absent-discovery fallback; never
reinterpret missing quota as a model permission or persist model absence as a ban.

For dynamic effort, wrong: admit/quota-filter static high and later rewrite the
envelope to low. Correct: resolve a discovered model for the actual account,
authorize that target or client alias, then read exact-model quota and build
the envelope. Keep family matching out of discovery and quota guards.

For Antigravity authentication, wrong: clear invalid state before checking refresh
success, or label a rejected Token as merely expired and assume selection stops.
Correct: retain failures, normalize explicit Token rejection to `oauth_token_invalid`,
and reuse existing hard-invalid guards. Account verification is a separate
`[ACCOUNT_BLOCK]` and cannot be healed by a successful quota probe.

For cumulative group windows, wrong: reuse independent model-window `all`
exhaustion or match Argon to Gemini by prefix. Correct: explicitly bind the
account's exact model to an upstream group and reject when any enabled
nonelapsed group limit is known exhausted; preserve independent model guards.
