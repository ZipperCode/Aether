# Gateway A: planner, Pool, and architecture CI repair

- Status: READY / stop writes, 2026-09-08.
- Baseline evidence: immutable SHA `948c1c16f2f9927b37a8570b86767128abd6b7c8`, run `34210248162`, Gateway inventory rows 1, 2, 3, 15, 16.
- Exclusive product write set: `candidate_materialization.rs`, `pool_scheduler.rs`, and `tests/architecture/ai_serving.rs` under `apps/aether-gateway/src`; this research receipt is the only additional write. All changes are tests or test comments; production code is unchanged.

## Failure-to-fix mapping

| CI row / original test | Actual root cause | Fix and preserved meaning |
| --- | --- | --- |
| 1 / `resolved_candidate_page_cache_requires_fixed_order_or_explicit_affinity` | The final fixture correctly has a fixed-order routing policy, but its positive cache assertion conflicts with `should_cache_resolved_candidate_page`: routed aggregate pages deliberately bypass resolved-page caching because they contain request-specific global ordering and fallback chains. The underlying `LocalCandidatePreselectionPageCursor` independently permits fixed-order priority pages. | Rename to `resolved_candidate_page_cache_preserves_affinity_and_routed_page_boundaries`; assert the inner priority-page condition is true and the outer routed-page decision is false. Retain the existing non-routed explicit-affinity positive case and no-affinity/sticky/routed negative cases. Update the old full-transport comment because resolved snapshots are now transport-free. No change to routing policy, global ordering, hydration, cache implementation, time, or distribution seeds. |
| 2 / `active_probe_subscription_exhaustion_respects_existing_switch` | Upstream and the current balance scheduling spec intentionally make known subscription exhaustion unconditional. The test still expected the old switch to bypass preflight eviction. | Rename to `active_probe_subscription_exhaustion_ignores_historical_switch`; exercise explicit `false` and `true`, asserting the exact evicted Key set and removal from active membership in both cases. Production exhaustion, runtime quota, balance, and manual-recovery behavior remain unchanged. |
| 3 / `pool_scheduler_evicts_low_balance_active_probe_member` | Its nonempty catalog snapshot contains only the hot Key, so the omitted cold Key hits the existing missing-entry hard-block path in `run_local_execution_pool_scheduler_with_runtime_map`. After seal fallback both Keys are filtered and indexing candidates panics. | Supply the cold Key's default available `PoolMemberSignals`, as a complete page snapshot would. Assert the entire scheduled Key list, exact hot-Key balance skip pair, and exact eviction set rather than indexing the first item. Missing-state filtering and real low-balance eviction remain unchanged. |
| 15 / `ai_serving_crate_owns_attempt_loop_without_gateway_runtime_deps` | Ownership did not move: `candidate_ranking.rs` still implements the serving port, but lazy hydration generalized it to `impl<Candidate> ... GatewayLocalCandidateRankingPort<'_, Candidate>`. The source assertion required the retired nongeneric spelling. | Require the exact generic implementation and `LocalCandidateRankingTarget` bound plus both `EligibleLocalExecutionCandidate` and `RankedLocalExecutionCandidate` target implementations. Keep all serving delegation and forbidden-policy assertions. |
| 16 / `retired_api_format_occurrences_are_whitelisted` | `lifecycle/migrate/tests/policy_nulls.rs` deliberately includes `claude:chat` in `preserved_values`, inserts it into both JSON/JSONB policy columns, executes the null repair twice, and checks exact value preservation. The file is a genuine legacy migration regression, not a runtime acceptance path. | Add only that exact migration-test path to the existing explicit exceptions, with its preservation rationale. No wildcard/directory exception, removed alias, changed parser, or migration edit. |

## Actual verification

- `rustfmt --edition 2021 --check` on the three owned Rust files: PASS after applying the one formatting adjustment it requested. No global formatter ran.
- `git diff --check -- <three owned Rust paths>`: PASS. Reviewed complete scoped diff; only the five test functions and their comments changed.
- Read-only PowerShell check extracted the ranking required/forbidden arrays from the current architecture test and checked the real ranking source: PASS, **9 required and 8 forbidden patterns**.
- Read-only PowerShell replay of the alias scan used the current test's exact allowed paths and alias strings with matching Rust/TypeScript/Vue file scope: PASS, **2,738 files, 13 exact exception paths, 6 retired aliases, 0 violations**.
- These checks are not a Rust runtime-test or full architecture-test PASS. No Gateway/workspace compilation, runtime suite, database, provider, CI operation, commit, push, tag, or deployment was performed by this writer.

## Integrator runtime filters

Run after all writers stop, using the shared CI environment/profile contract and one unified Gateway build. The five repaired test filters are:

```text
test(ai_serving::planner::candidate_materialization::tests::resolved_candidate_page_cache_preserves_affinity_and_routed_page_boundaries)
test(dispatch::pool_scheduler::tests::active_probe_subscription_exhaustion_ignores_historical_switch)
test(dispatch::pool_scheduler::tests::pool_scheduler_evicts_low_balance_active_probe_member)
test(tests::architecture::ai_serving::ai_serving_crate_owns_attempt_loop_without_gateway_runtime_deps)
test(tests::architecture::ai_serving::retired_api_format_occurrences_are_whitelisted)
```

The existing relevant preservation checks are `priority_page_cache_requires_fixed_order_or_explicit_affinity`, `routed_ranking_hydrates_only_selected_candidates_in_fallback_order`, `pool_scheduler_skips_quota_exhausted_key_when_flag_is_false`, `pool_scheduler_falls_back_when_active_probe_members_are_unschedulable`, `pool_low_balance_skip_releases_cursor_scan_budget`, and `pool_key_cursor_skips_low_balance_sticky_key_and_falls_back_once`. They were not rerun by this writer.

## Handoff and cleanup

- No cross-scope lock request, unresolved implementation decision, new dependency, helper framework, temporary file, or persistent process.
- Other agents' and preexisting dirty files were preserved. Root owns the task state, independent review, unified runtime validation, Git, and exact-SHA CI follow-up.
- Historical memory suggested the old subscription switch semantics; the current source, dispatch, and updated spec explicitly supersede that memory. No memory was edited.
