# Research: routing policy synchronization

- Query: Selectively synchronize upstream routing commits `415b2da81` and `7323d41fb` onto fork baseline `50c96d060`, including only true prerequisites; identify dependency order, conflicts, fork behavior to retain, excluded-commit dependencies, callers/contracts, and the smallest validation set.
- Scope: internal
- Date: 2026-09-02
- Repository snapshot: worktree `C:\Users\Zipper\AppData\Local\Temp\aether-sync-selected-upstream-20260902`; fork baseline/HEAD `50c96d060442fb1b612a27c587b91dec4f79a613`; inspected upstream tip `cae9aa4134b6bfd4b21dab0c535186232002ed34`; common ancestor `7892aa94853461c1e634f7a5babbb1280128720f`.

## Findings

### Conclusion

Select exactly the two topic commits, in this strict order:

1. `415b2da81bdd307a808cce231a89d9fd711d78a8` — makes the resolved routing profile authoritative for scheduler ordering, bootstraps the system-default routing group, and adds API-format-scoped Key priorities.
2. `7323d41fbec5a18fbdc102e50822dd8a6bc29641` — directly descends from `415b2da81` and moves same-Key retries to `sticky_key_attempts`, materialized lazily in the attempt loop.

No additional upstream commit is a true R1 prerequisite on this fork. In particular, do **not** cherry-pick merge commits `e3644c614` / `cae9aa413`, or ancestor merge `166236c2e`: their ancestry includes excluded VSCodex, nightly, entitlement-revocation, and generic Usage API work. The current fork already contains the routing repository, scheduler ordering, candidate-page, cache-affinity/concurrency, dynamic-attempt-source, and frontend routing-editor contracts that these two patches call. This is corroborated by patch-context checks: `415b2da81` applies everywhere except one fork-edited import/call-site file; the combined range `415b2da81^..7323d41fb` reports only three textual conflict files.

For the overall R1-R4 integration, apply lower-level selected Pool/protocol/gateway patches before these two, then apply `415b2da81`, and keep `7323d41fb` last among changes touching candidate materialization or attempt loops. If R2 commit `9631b229b` is selected by the Pool research, put it before `415b2da81` (matching upstream authorship); it is not an extra R1 prerequisite because equivalent required contracts are already present locally.

### Commit dependency and excluded history

| Commit/history | Decision | Evidence / reason |
|---|---|---|
| `415b2da81` | Select | Owns R1 policy authority, default-group bootstrap, per-format Key priority, backend/admin/frontend projections. |
| `7323d41fb` | Select after `415b2da81` | Parent is exactly `415b2da81`; consumes its `SchedulerOrderingConfig::from_routing_policy` and extends its routing DTOs with `sticky_key_attempts`. |
| `e3644c614`, `cae9aa413` | Exclude | Merge wrappers add no R1 content beyond the selected topic commits and would pull sibling history. |
| `30a75832f` | Exclude | VSCodex product module; no symbol is referenced by either selected patch. |
| `ef7caa40e`, `611c29f1f` | Exclude | Nightly release/build wiring; no runtime contract needed by R1. |
| `144a28f54` | Exclude | Plan entitlement revocation; unrelated repository/API/frontend domain. |
| `4dbf98163` | Exclude | Generic Provider Usage API template; no R1 dependency. |
| Other commits between `7892aa948` and `166236c2e` | Do not add merely for ancestry | The selected patches are ordinary non-merge diffs. Required local APIs exist at `50c96d060`; direct patch checks and caller inspection show no missing R1 contract. Other selected feature research may independently choose some of them. |

No fragment from an excluded commit is required. The only reason excluded commits appear in ancestry is upstream merge topology, not a source or build dependency.

### Resulting behavior contract

1. **Policy authority and migration fallback**
   - A request with a resolved routing policy derives `priority_mode`, `scheduling_mode`, `keep_priority_on_conversion`, and later `sticky_key_attempts` directly from that policy; it does not OR the legacy global conversion flag (`415b2da81:apps/aether-gateway/src/ai_serving/planner/candidate_ranking.rs:190-196`).
   - A request without a resolved policy reads the enabled system-default routing group's `default_policy`, then falls back to the three legacy system config keys only when no usable enabled system-default group exists (`415b2da81:apps/aether-gateway/src/scheduler/config.rs:145-223`).
   - Startup creates an enabled/published system-default group from those legacy values when routing storage is writable and none exists (`415b2da81:apps/aether-gateway/src/state/routing_profiles.rs:22-99`; wired at `415b2da81:apps/aether-gateway/src/main.rs:2099`). This uses the existing opaque `config_json` repository, so no schema migration is introduced.
   - Provider-local `keep_priority_on_conversion` remains an independent per-provider override and is still ORed with the policy-derived value; only the legacy **global** value stops leaking into a resolved policy (current `apps/aether-gateway/src/ai_serving/planner/candidate_transport_ranking_facts.rs:93-115`).

2. **API-format-scoped Key priority**
   - Persisted routing JSON gains `model_policies[*].key_priority_overrides_by_format[api_format][key_id]`; `serde(default)` keeps old configs readable (`415b2da81:crates/aether-routing-core/src/model.rs:35-58`).
   - Precedence is format-scoped Key override, then format-agnostic `key_priority_overrides`, then catalog fallback (`415b2da81:crates/aether-routing-core/src/ranking.rs:48-75`). Planner ranking uses alias-aware API-format matching; Pool allowed-Key sorting uses the candidate endpoint format (`415b2da81:apps/aether-gateway/src/ai_serving/planner/candidate_ranking.rs:200-235`; `415b2da81:apps/aether-gateway/src/dispatch/pool_scheduler.rs:749-779`).
   - The routing action `set_key_priority` gains optional `api_format`; an omitted/blank format preserves the old global-per-Key behavior (`415b2da81:crates/aether-routing-core/src/actions.rs:81-90`; `415b2da81:crates/aether-routing-core/src/policy.rs:228-251`).
   - Frontend normalization lowercases/trims format keys, edits only the selected format, and retains the old global map as display/runtime fallback (`415b2da81:frontend/src/features/routing/utils/routingPolicy.ts:299-363`; editor at `415b2da81:frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue:450-475,689-769`).

3. **Scheduler call chain**
   - The request-derived `SchedulerOrderingConfig` is threaded through preselection rather than re-read independently inside candidate selection (`415b2da81:apps/aether-gateway/src/ai_serving/planner/state/scheduler.rs:13-209`; `415b2da81:apps/aether-gateway/src/scheduler/candidate/selection.rs:113-260`).
   - Verified production callers cover the paged standard path, same-format provider path, Gemini Files, images, and video (`7323d41fb:apps/aether-gateway/src/ai_serving/planner/candidate_source.rs:164,1285`; `.../passthrough/provider/family/candidates.rs:134,244`; `.../specialized/files/support.rs:105,182`; `.../specialized/image/support.rs:122,200`; `.../specialized/video/support.rs:128,189`).
   - Admin model-routing preview and monitoring cache snapshot switch to the same effective ordering helper; Provider Management reads the enabled system-default group first (`415b2da81:apps/aether-gateway/src/handlers/admin/model/routing.rs:60-80`; `.../observability/monitoring/cache_store.rs:264-276`; `415b2da81:frontend/src/views/admin/ProviderManagement.vue:539-556`).

4. **Sticky retry semantics**
   - `RoutingDefaultPolicy.sticky_key_attempts` defaults to `2` for missing legacy JSON. `0` and `1` both mean one total attempt/no same-Key retry; a rule-level `set_scheduling` action may override it (`7323d41fb:crates/aether-routing-core/src/model.rs:25-57`; `.../policy.rs:204-224,475-541`).
   - Exactly one attempt per candidate is materialized. After a candidate-scoped failure, the attempt loop may derive the same Key again with a fresh candidate ID and incremented `retry_index`; no fallback candidate is pre-expanded (`7323d41fb:apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs:1598-1661,1894-1910`; `7323d41fb:crates/aether-ai/serving/src/attempt_loop.rs:105-170,225-305`).
   - Only `candidate_index == 0` may repeat. All later candidates receive one attempt. Within a Pool group only `pool_key_index == 0` may repeat, and encoded retry indices remain below stride `100` to avoid collision with the next Pool Key (`7323d41fb:apps/aether-gateway/src/orchestration/attempt.rs:153-204`).
   - The policy budget is resolved during candidate resolution, preserved through Pool expansion, serialized into report context, then read by both static and dynamic attempt loops (`7323d41fb:apps/aether-gateway/src/ai_serving/planner/candidate_resolution.rs:373-395`; `.../report_context.rs:52-65,120-136`; `.../dispatch/pool_scheduler.rs:1944-1961`; `.../executor/candidate_loop.rs:243-259,783-866,963-978`).
   - Existing provider/endpoint `max_retries` columns and import/export DTOs remain for compatibility, but they cease to generate local candidate attempt slots. Upstream deliberately removes only the Provider form input and the two retry-slot consumers; do not expand R1 into a storage/API migration (`7323d41fb:frontend/src/features/providers/components/ProviderFormDialog.vue:150-166`; current consumers to replace are `apps/aether-gateway/src/orchestration/attempt.rs:143-195` and `crates/aether-ai/serving/src/candidate_persistence.rs:12-62`).

### Patch-context conflicts and fork behavior to retain

`git apply --check` against `50c96d060` gives this bounded textual merge surface:

| File / symbol | Required integration | Fork behavior that must survive |
|---|---|---|
| `apps/aether-gateway/src/ai_serving/planner/specialized/files/support.rs` | Manual merge of the added `SchedulerOrderingConfig` import and policy-derived argument at both selection calls. | Keep `LocalGeminiFilesSpec`, `set_endpoint_capability_context`, dynamic `require_streaming` / `request_operation`, and both endpoint-capability quarantine calls added by local `60377958` (current lines 31,48,82,115-126,190-201,257,285). Never revert these calls to upstream's older hard-coded `false`/missing-operation shape. |
| `crates/aether-ai/serving/src/attempt_loop.rs::run_ai_attempt_loop` and `AiAttemptLoopOutcome` | Add `with_same_key_retry`, `next_same_key_retry`, and `pending_same_key_retry`, but merge them into the fork loop. | Keep local `Deferred { response, exhaustion }` and preserve the exact fallback attempt's plan/context when building exhaustion (current lines 21-25,109-166). Do not accept upstream context that regresses this to tuple `Deferred(Response)` or uses the last later failure for the fallback diagnostic. |
| `apps/aether-gateway/src/executor/candidate_loop.rs::run_dynamic_attempt_loop` | Add a pending same-Key attempt that executes before asking the dynamic source for its next candidate. | Keep local `fallback` tuple with plan/context, `drain_execution_attempts` + `mark_unused_attempts` on early return/exhaustion, sync/stream admission handling, endpoint-level failover, and local execution effects (current lines 785-887; admission/effects at 1365-1805). A quota-blocked derived retry must still pass `should_skip_attempt` before execution, allowing the existing runtime-quota filter to discard it. |

The following files are patch-clean but semantically overlap local work and require review rather than wholesale replacement:

- `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs`: retain endpoint-capability metadata/quarantine and exact Endpoint binding from `60377958`; change only retry-slot expansion to one candidate attempt plus report metadata.
- `apps/aether-gateway/src/ai_serving/planner/candidate_source.rs`, `decision_input.rs`, and specialized/passthrough payload files: preserve endpoint signature, required streaming, operation, and model-directive context while adding ordering/budget fields.
- `apps/aether-gateway/src/dispatch/pool_scheduler.rs`: retain local balance and manual runtime-quota eligibility (`runtime_quota_hard_blocked` / `balance_below_minimum` currently lines 325-328, strong runtime block read at 1284, shared projection at 1582-1583). Layer per-format priority sorting and sticky-attempt metadata on top; do not bypass the shared Pool scheduler or strong Key reads.
- `apps/aether-gateway/src/handlers/admin/model/routing.rs`: retain local exact model↔Endpoint binding/fallback behavior; replace only the three independent legacy scheduler reads with the shared effective config.
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue` and `frontend/src/views/admin/RoutingProfiles.vue`: retain local `a0aa5296` per-model Provider eligibility filter and hidden-override preservation (`providerRows` and `updateProviderOverrides`, current editor lines 408-410,451-467,625-630; parent binding at current RoutingProfiles line 670). Add per-format Key maps without resetting hidden Provider overrides.
- `frontend/src/mocks/handler.ts` and `ProviderManagement.vue`: expect overlap with local quota/status mocks and balance UI. Add only missing routing fields/loading; retain existing quota payloads.
- `apps/aether-gateway/src/executor/orchestration.rs` and usage/failover tests: preserve local Codex quota-race and Endpoint-failure assertions; update expected call counts only where default sticky attempt `2` intentionally adds one first-candidate call.

### Files found

Core contracts:

- `crates/aether-routing-core/src/actions.rs` — JSON action fields for scheduling and per-format Key priority.
- `crates/aether-routing-core/src/model.rs` — persisted routing config, default policy, and sticky-attempt default.
- `crates/aether-routing-core/src/policy.rs` — policy resolution and rule-action application.
- `crates/aether-routing-core/src/ranking.rs` — per-format priority precedence and trace rank vectors.
- `crates/aether-routing-core/src/{lib.rs,validation.rs}` — exports and phase validation fixtures.
- `crates/aether-ai/serving/src/attempt_loop.rs` — shared fixed-vector attempt loop and retry extension points.
- `crates/aether-ai/serving/src/candidate_persistence.rs` — changes materialization from N slots to one slot per candidate.
- `crates/aether-ai/serving/src/lib.rs` — exports sticky report-field contract.

Gateway policy/scheduler/runtime:

- `apps/aether-gateway/src/scheduler/config.rs` — one effective ordering config plus legacy bootstrap conversion.
- `apps/aether-gateway/src/state/routing_profiles.rs` and `main.rs` — idempotent startup creation of the system-default routing group.
- `apps/aether-gateway/src/routing/resolver.rs` — static-default policy parsing, including sticky attempts.
- `apps/aether-gateway/src/scheduler/candidate/{mod.rs,selection.rs}` and tests — explicit request ordering threaded through all selection entry points.
- `apps/aether-gateway/src/ai_serving/planner/state/scheduler.rs` — planner facade for the updated scheduler signatures.
- `apps/aether-gateway/src/ai_serving/planner/{candidate_ranking.rs,candidate_source.rs,candidate_resolution.rs}` — policy authority, preselection propagation, and sticky-budget attachment.
- `apps/aether-gateway/src/ai_serving/planner/{candidate_materialization.rs,decision_input.rs,report_context.rs}` — one-attempt materialization, trace fields, and retry-budget transport.
- `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/{candidates.rs,payload.rs}` — same-format provider candidate selection/report propagation.
- `apps/aether-gateway/src/ai_serving/planner/specialized/{files,image,video}/{support.rs,decision.rs}` — specialized endpoint candidate selection/report propagation.
- `apps/aether-gateway/src/ai_serving/planner/standard/family/payload.rs` and `standard/openai/{chat,responses}/decision/payload.rs` — standard request report propagation.
- `apps/aether-gateway/src/dispatch/{pool_scheduler.rs,refs.rs}` — Pool per-format Key ordering and orchestration metadata.
- `apps/aether-gateway/src/orchestration/{attempt.rs,mod.rs}` — retry-index policy and fresh same-Key attempt creation.
- `apps/aether-gateway/src/executor/{candidate_loop.rs,orchestration.rs}` — fixed/dynamic loop integration and gateway retry regression.
- `apps/aether-gateway/src/handlers/admin/model/routing.rs` — model routing preview uses effective policy.
- `apps/aether-gateway/src/handlers/admin/observability/monitoring/cache_store.rs` — monitoring snapshot uses effective policy.
- `apps/aether-gateway/src/tests/{ai_execute,usage.rs,usage/local.rs,architecture/ai_serving.rs}` — sync/stream/search/usage and ownership regressions.

Frontend:

- `frontend/src/features/routing/utils/routingPolicy.ts` — routing DTO normalization, per-format maps, sticky default/clamp.
- `frontend/src/features/routing/components/RoutingPriorityPolicyEditor.vue` — selected-format Key editing; local per-model Provider filter must remain.
- `frontend/src/views/admin/RoutingProfiles.vue` — global conversion-priority switch and sticky-attempt input.
- `frontend/src/views/admin/ProviderManagement.vue` — system-default policy badge/source.
- `frontend/src/features/providers/components/ProviderFormDialog.vue` — removes the obsolete retry-count control only.
- `frontend/src/features/routing/__tests__/routingPolicy.spec.ts`, `frontend/src/views/admin/__tests__/RoutingProfiles.allowed-models.spec.ts`, and existing `RoutingPriorityPolicyEditor.spec.ts` — normalization/UI/local eligibility regressions.
- `frontend/src/mocks/handler.ts` — mock routing JSON defaults.

### Smallest validation set

Run after integration, in this order; stop at the first failure and fix that scope:

```powershell
cargo fmt --all --check
cargo test -p aether-routing-core
cargo test -p aether-ai-serving attempt_loop
cargo test -p aether-ai-serving candidate_persistence
cargo test -p aether-gateway scheduler::config::tests
cargo test -p aether-gateway orchestration::attempt::tests
cargo test -p aether-gateway openai_image_sync_heartbeat_retries_sticky_key_lazily_before_failover
cargo test -p aether-gateway gateway_retries_next_local_openai_chat_sync_candidate_after_auth_failure
cargo test -p aether-gateway gateway_retries_next_local_openai_chat_stream_candidate_after_retryable_failure
cargo test -p aether-gateway pool_scheduler
cargo check -p aether-gateway
Set-Location frontend
npm run test:run -- src/features/routing/__tests__/routingPolicy.spec.ts src/features/routing/components/__tests__/RoutingPriorityPolicyEditor.spec.ts src/views/admin/__tests__/RoutingProfiles.allowed-models.spec.ts
npm run type-check
```

Why these are minimal: routing-core proves serialization/default/rule and per-format precedence; ai-serving proves lazy loop/materialization; focused gateway filters cover bootstrap, retry-index rules, one concrete execution path, sync/stream failover, and the risky Pool merge; `cargo check` catches every changed Rust struct literal/caller; the three frontend specs plus type-check cover normalization, local per-model filtering, the new controls, and API/config shape. Do not run `npm run lint` as a check because this project config runs ESLint with `--fix`.

### Integration requests

1. Cherry-pick or replay `415b2da81` first; resolve `specialized/files/support.rs` by preserving local Endpoint capability arguments and adding only the new ordering argument.
2. Apply `7323d41fb` last around execution-loop work. Hand-merge the two loops; do not choose either side wholesale.
3. Keep `max_retries` persistence/import/export fields; remove their role in candidate attempt materialization and remove only the Provider form control, matching upstream's bounded change.
4. After resolving, grep every `ResolvedRoutingPolicy {`, `RoutingDefaultPolicy {`, `RoutingAction::SetScheduling {`, `RoutingAction::SetKeyPriority {`, `RoutingCandidateFacts {`, and `LocalExecutionCandidateMetadata {` literal. `cargo check` is the final exhaustive guard, but the grep makes omissions cheap to locate.
5. Review the final diff for forbidden ancestry/product files; R1 should not introduce `vscodex`, nightly workflow/install scripts, plan-entitlement revocation, or generic Usage API paths.

## External references

- None. No web documentation was needed; repository Git objects and current source are the authoritative version-specific evidence.
- Rust workspace package names verified from current manifests: `aether-routing-core`, `aether-ai-serving`, and `aether-gateway`. Frontend commands verified from current `frontend/package.json`.

## Related specs

- `.trellis/workflow.md` — Phase 1 research persistence and task boundaries.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` — trace config from persisted JSON through policy, scheduler, report context, executor, and UI.
- `.trellis/spec/guides/code-reuse-thinking-guide.md` — keep one ordering source and shared policy helpers.
- `.trellis/spec/aether-routing-core/backend/index.md` — routing-core package entrypoint; package-specific documents are otherwise placeholders.
- `.trellis/spec/aether-gateway-execution/backend/index.md` — gateway execution package entrypoint; package-specific documents are otherwise placeholders.
- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md` — local balance eligibility that Pool changes must preserve.
- `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md` — local strong runtime quota blocking and candidate/Pool behavior that retry-loop changes must preserve.
- `.trellis/spec/aether-gateway/backend/quality-guidelines.md` — existing bounded-response behavior remains outside this change but constrains executor conflict resolution.

## Caveats / Not Found

- `git apply --check` is textual evidence, not compile proof. The combined range has only three textual conflicts, but broad struct-field additions and semantic overlap still require the listed checks.
- The UI normalizer clamps `sticky_key_attempts` to `99`, while backend routing JSON accepts `u32` and lazily derives attempts without a general upper bound. Pool retries remain capped by the index stride (`<100`). This is upstream's intentional current contract; do not “harmonize” it during sync unless scope changes.
- System-default bootstrap is best-effort: read-only/missing routing storage or a creation error leaves the legacy fallback active. Therefore “sole source” means routing policy is authoritative once resolved/bootstrapped, not that legacy keys are deleted in this task.
- No standalone frontend API DTO change is needed because routing group `config_json` is an opaque JSON contract normalized in `routingPolicy.ts`.
- No database migration, VSCodex fragment, entitlement fragment, generic Usage API fragment, or nightly/release fragment was found to be necessary for R1.
