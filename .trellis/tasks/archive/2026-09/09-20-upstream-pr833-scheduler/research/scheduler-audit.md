# Research: Pool candidate false negatives during upstream integration

- Query: Trace upstream #824 and the fork's eligibility gates for the report "available Keys are skipped; no candidates". Identify a demonstrated residual defect and the smallest regression boundary.
- Scope: internal, with exact upstream commit diffs supplied by the coordinator
- Date: 2026-09-20
- Workspace: `C:/Users/Zipper/.codex/worktrees/sync-upstream-pr833/Aether`
- Parent-supplied baseline: `e34c05e8970d46444c3e1480ef94384521fb1fad`; merge target: `ba7c9f8b270cce63b0515299076b30129d7d64b4`.

## Findings

### Result

There is a concrete residual **PoolGroup representative-Key false negative** in the current merged source. Pool discovery deliberately returns one representative Key per Provider/Endpoint/Model. Runtime selection correctly suppresses that representative's quota, balance, OAuth and circuit facts, but still supplies its Key runtime record to shared concurrency, health and RPM gates. A single saturated, zero-health or RPM-exhausted representative therefore suppresses the whole Pool before healthy sibling Keys can be expanded.

This defect is present in both the original worktree source and the current integration source. It is separate from #824. This is source-derived causality, not a diagnosis of a particular production request.

### Files found

| Path | Purpose |
| --- | --- |
| `research/source-provenance.md` | Exact upstream commits and commands used for the coordinator-supplied diffs; temporary diff copies were removed after review. |
| `apps/aether-gateway/src/dispatch/pool_scheduler.rs` | Score/catalog cursor, sticky expansion, real-Key Pool filters, bounded windows and regression fixtures. |
| `apps/aether-gateway/src/state/catalog.rs` | Key updates and removal of inactive-Key score rows. |
| `apps/aether-gateway/src/scheduler/candidate/runtime.rs` | Runtime snapshot loading and ordinary/representative selectability inputs. |
| `apps/aether-gateway/src/scheduler/candidate/selection.rs` | Selection pipeline and mode-specific affinity behavior. |
| `apps/aether-gateway/src/scheduler/candidate/resolution.rs` | Applies runtime rejection and records exact skip reasons. |
| `apps/aether-gateway/src/scheduler/candidate/ranking.rs` | Correct example: representative Key health facts are omitted from group ranking. |
| `crates/aether-scheduler-core/src/candidate/selectability.rs` | Shared provider and Key runtime hard gates. |
| `crates/aether-scheduler-core/src/health.rs` | Real Key concurrency counts, health and RPM interpretation. |
| `crates/aether-data/adapters/postgres/src/candidate_selection.rs` | SQL collapses a Pool into one representative. |
| `crates/aether-pool-core/src/scheduler.rs` | Real-Key Pool quota/auth/balance/cooldown/cost filters and ranking. |
| `apps/aether-gateway/src/provider_pool_demand.rs` | Strong-read execution admission and atomic Key concurrency permit. |
| `crates/aether-provider/transport/src/conversion.rs` | Real-Key activation, API format and model-policy filters. |
| `crates/aether-ai/serving/src/runtime_miss.rs` | Distinguishes empty candidate discovery from all candidates skipped. |
| `apps/aether-gateway/src/handlers/proxy/mod.rs` | Runtime-miss logging and 429/503 classification. |

### What #824 actually changes

1. `pool_scheduler.rs:649`: `next_page_candidates()` now falls through to catalog paging when the score phase yields an empty materialized candidate vector. The original worktree returns `Some(empty)` immediately at its line 656. The new check is at integration lines 655-658.
2. `state/catalog.rs:929`, `:941`, `:1000`: deactivating a Key removes its Pool score rows in single and batch update paths. Cleanup failures log a warning; catalog fallback remains necessary for previously stale or failed-cleanup rows.
3. `ai_serving/planner/standard/openai/chat/plans/stream.rs:207`, `:373`: fixed-order streaming Chat uses target-selection window 1, retaining the ranked order instead of reselecting from a pressure-based window.
4. `ai_serving/planner/candidate_resolution.rs:137`, `:203` and `scheduler/candidate/resolution.rs:24`: skipped candidates gain structured logging, including reason and non-secret identity fields.

Current score accounting already counts materialized rows instead of nonexistent score members: `pool_scheduler.rs:986-1000`. Missing score rows increment `pool_score_member_missing` without consuming effective Key budget. This accounting was already present in the fork; do not attribute all of it to #824.

Existing regression coverage in the integrated source:

- `pool_key_cursor_does_not_spend_scan_budget_on_missing_score_rows` (`pool_scheduler.rs:4417`).
- `inactive_pool_key_with_stale_score_does_not_exhaust_pool` (`:4474`).
- `stale_inactive_score_only_does_not_exhaust_pool` (`:4530`).
- `fixed_order_disables_stream_target_selection` (`standard/openai/chat/plans/stream.rs:543`).
- Fixed-order affinity tests in `scheduler/candidate/tests/selection.rs:546`, `:607`.

### Residual defect: representative-Key runtime facts suppress healthy siblings

The current chain is:

1. PostgreSQL `candidate_selection.rs:203` selects only one Key when `pool_advanced` is present; `:299-313` also collapses to one row per Provider/Endpoint/Model. The selected representative is ordered by Key internal priority, then Key ID (`:202`, `:307-308`).
2. `scheduler/candidate/selection.rs:247` strongly loads the candidate runtime snapshot; `:297` applies `resolve_scheduler_candidate_selectability()`.
3. `scheduler/candidate/runtime.rs:216` and `:273` know this is a PoolGroup. They mask representative quota, balance, OAuth and circuit flags, but pass the complete `provider_key_rpm_states` map at `:228` and `:300`.
4. `crates/aether-scheduler-core/src/candidate/selectability.rs:92-137` unconditionally looks up that representative record and checks:
   - Key concurrent limit (`:94-109`), yielding `provider_key_concurrency_limit_reached`;
   - format health score at or below zero (`:121-126`), yielding `key_health_score_zero`;
   - fixed/adaptive RPM (`:127-137`), yielding `key_rpm_exhausted`.
5. If rejected, the candidate never becomes `LocalExecutionCandidateKind::PoolGroup` (`ai_serving/planner/candidate_resolution.rs:231`) and cannot reach `PoolKeyCursor` to find another Key.

The ranking code already models the intended boundary: `scheduler/candidate/ranking.rs:30-42` does not use representative-Key health for group ranking.

Minimal deterministic reproduction: a Pool has Key A (first representative) and healthy Key B; A has `health_by_format = {"openai:chat":{"health_score":0.0}}`, all provider-level gates are clear. With only the SQL-style representative candidate A passed to preselection, the current code rejects the Pool with `key_health_score_zero`, although B is eligible. Key saturation and RPM exhaustion are equivalent independent cases.

### Real-Key enforcement that a repair must preserve

- **Atomic concurrency already exists after expansion.** `provider_pool_demand.rs:479-513` strongly reads the selected Key, confirms Key/Provider identity and acquires a Key-specific permit. Sync (`execution_runtime/sync/execution.rs:2263`), stream (`execution_runtime/stream/execution.rs:4050`) and Responses WebSocket (`handlers/proxy/websocket/responses/admission.rs:46`) share it. Missing records, mismatched Provider IDs and read errors fail closed.
- **Real Pool health/RPM filtering is currently absent.** The shared `schedule_pool_page_candidates()` (`pool_scheduler.rs:112`) strongly loads catalog Keys (`:1509`) then runs Pool-core filters. `crates/aether-pool-core/src/scheduler.rs:234-291` applies catalog availability, auth/account block, manual quota block, subscription/model quota, balance, cooldown and cost. Health only contributes to ordering. There is no call to shared scheduler health/RPM checks here.
- `candidate_common_transport_skip_reason()` (`conversion.rs:262-294`) only checks Provider/Endpoint/Key activation, API format and allowed model. It does not fill the missing health/RPM enforcement.
- Sticky hits already reuse the shared Pool scheduler (`pool_scheduler.rs:1056`). Normal pages also use it (`:1087`), making this the shared place to check actual-Key runtime facts before candidate-window truncation.

### Recommended minimum repair

1. In both gateway runtime selectability paths, omit the representative Key runtime record for PoolGroup evaluation. Keep provider quota, provider concurrency, auth-request capacity and error propagation intact. One bad Key must not be projected onto its whole group.
2. Reuse the existing Key runtime checks against **actual strongly read Keys** in the shared Pool page/singleton path, before window truncation. Extracting the existing Key-only portion of scheduler-core selectability into one reusable helper is preferable to reimplementing concurrency/health/RPM rules.
3. Supply the same recent-request counters, current Unix time and RPM reset watermark that ordinary-Key evaluation consumes. Preserve Pool's current circuit/cooldown semantics rather than silently adding a new circuit policy.
4. Keep Pool quota/auth/balance processing in its current layer so `pool_key_quota_exhausted`, `pool_account_exhausted`, `pool_account_blocked` and `pool_key_state_unavailable` retain their meanings. Do not route the fix through a second conflicting quota classifier or bypass catalog errors.
5. Do not change ranking modes, scan ceilings, refresh workers, manual recovery, SQL schema or unrelated protocols.

### Exact regression boundary

**Gateway runtime fixture:** extend the pattern in `scheduler/candidate/runtime.rs:701`, which currently tests only low-balance representative isolation. For each independent case (zero health, saturated Key limit, exhausted RPM), assert:

- ordinary Key selection returns its exact existing skip reason;
- the same snapshot with the provider in `pool_provider_ids` leaves its representative selectable;
- both boolean selectability and diagnostic selectability agree;
- provider-level quota and provider concurrency still suppress that same PoolGroup.

For saturation and RPM fixture shapes, reuse scheduler-core tests `provider_key_concurrency_limit_rejects_pending_active_with_exact_skip_reason` (`candidate/mod.rs:444`) and `provider_key_concurrency_limit_preserves_key_circuit_and_rpm_checks` (`:583`). Use a fixed real timestamp; do not substitute the ordering seed for `now_unix_secs`.

**Gateway Pool regression:** seed two real Keys but only A as the initial representative, as PostgreSQL does. A is zero-health or RPM-exhausted; B is healthy. Assert preselection retains the group, shared Pool expansion records A's rejection and returns B. Repeat with A as sticky binding to prove shared singleton filtering. Also exercise all actual Keys denied, so the fix cannot become unconditional admission.

**Fail-closed and independent restrictions:** retain coverage of manual quota blocks, model-scoped quota, low balance, invalid auth, and catalog read failure/missing record. An actual catalog failure must remain `pool_key_state_unavailable`, never a quota-recovery instruction. Keep `provider_key_limit_rejects_concurrent_guard_until_release` (`provider_pool_demand.rs:737`) as execution capacity evidence.

**Directly relevant existing checks to run after implementation:**

```text
cargo test -p aether-scheduler-core
cargo test -p aether-gateway runtime_balance_fact_blocks_real_key_but_not_pool_group_representative
cargo test -p aether-gateway inactive_pool_key_with_stale_score_does_not_exhaust_pool
cargo test -p aether-gateway stale_inactive_score_only_does_not_exhaust_pool
cargo test -p aether-gateway pool_key_cursor_does_not_spend_scan_budget_on_missing_score_rows
cargo test -p aether-gateway fixed_order_disables_stream_target_selection
cargo test -p aether-gateway provider_key_limit_rejects_concurrent_guard_until_release
```

Add the new regression test names to the focused run. These commands are recommendations; this researcher did not compile or execute them.

### Legitimate denials and diagnostic boundaries

| Evidence | Interpretation |
| --- | --- |
| Key merely active in the UI | Insufficient evidence that it matches this model/Endpoint/auth channel or has available capacity. |
| `key_quota_exhausted` / `pool_key_quota_exhausted` | Persisted runtime manual-recovery block; balance refresh must not clear it. |
| `pool_key_state_unavailable` | Strong catalog failure/missing Key; fail closed and investigate data availability. |
| `account_quota_exhausted` / `pool_account_exhausted` | Account/model quota evidence; maintain model scope and reset/source rules. |
| `key_balance_below_minimum` / `pool_balance_below_minimum` | Shared fresh balance-source evidence, not Key activation. Stale/unknown balance inference remains fail-open. |
| `key_model_disabled`, `key_api_format_disabled`, provider/Endpoint activation or auth mismatch | Request-specific eligibility denial, not score paging. |
| `provider_key_concurrency_limit_reached`, `key_rpm_exhausted`, `key_health_score_zero` on only the representative | Investigate the demonstrated group projection defect before concluding every Key is unavailable. |

`crates/aether-ai/serving/src/runtime_miss.rs:65-99` distinguishes `candidate_list_empty` from `all_candidates_skipped`; later execution exhaustion has its own reason (`:132`). `handlers/proxy/mod.rs:2370-2385` combines live diagnostic and persisted candidate evidence; `:2860` chooses 429 for capacity-only exhaustion and 503 otherwise. #833 exposes this evidence but does not itself repair eligibility.

### Related specs

- `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md`: strong reads, persistent administrator recovery and distinct infrastructure failure reasons.
- `.trellis/spec/aether-provider-pool/backend/key-admission-affinity-model-quota-contract.md`: atomic execution admission, model-specific quota and 429/503 boundary.
- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md`: shared balance evidence and Pool representative isolation; current source evidence supersedes older memory about the historical exhaustion switch.
- `.trellis/spec/aether-scheduler-core/backend/index.md`, `.trellis/spec/aether-pool-core/backend/index.md`, `.trellis/spec/aether-gateway-execution/backend/index.md`: ownership and referenced contracts.
- `.trellis/workflow.md`: research persisted within this task; no product or spec modifications by researcher.

### External references and versions

- [Upstream PR #824](https://github.com/fawney19/Aether/pull/824), exact reviewed diffs from commits `7daf355e65708a348d2a8f3db6fbc6e36023f07d` and `01acff077468dcde8271a2efd9b86d420a814ee1`; reproduction commands are recorded in `source-provenance.md`.
- [PR #833](https://github.com/fawney19/Aether/pull/833), integration target `0486435f16b7db31add34133aa819e8477c81ea0` from task PRD; observational scope is separate from scheduler repair.
- Workspace uses pinned Rust 1.95.0 per project instructions. No external library behavior is required to explain this defect.

## Caveats / Not Found

- No production request ID/time, server logs or request catalog snapshot was supplied. Do not claim the user's live incident is proven fixed from this static evidence.
- No Git operation, product edit, compile, test execution or external mutation was performed by this researcher. Root supplied exact upstream diffs; original baseline file content was read from `D:/Project/GitHub/Aether` without Git.
- Line numbers describe the integration files at audit time and will move when the implementation agent applies the fix.
- The Pool cursor intentionally reads pages of 64 and retains a ranked window of 16 (`pool_scheduler.rs:1070-1104`). Existing tests explicitly codify bounded discard: `score_candidates_continue_across_pool_windows` expects 32 returned from 128 scored rows (`:4630`), and the large LRU test bounds output windows (`:5013`, `:5041-5050`). This can limit later retry choices, but it is established policy; do not silently rewrite it during this merge. Runtime filtering should happen before truncation.
- Absolute and effective scan ceilings remain intentional, as does request-format candidate paging. More Keys existing outside the configured scan/eligibility scope does not by itself prove another defect.
- The historical memory's subscription-switch wording is older than current quota specs. It was used only to locate the representative/balance boundary, not as authority to weaken current quota gates.
