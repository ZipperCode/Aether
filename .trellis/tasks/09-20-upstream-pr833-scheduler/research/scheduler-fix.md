# Scheduler representative-Key fix receipt

## Scope and changes

- Worktree: `C:/Users/Zipper/.codex/worktrees/sync-upstream-pr833/Aether`.
- Product files changed: `apps/aether-gateway/src/scheduler/candidate/runtime.rs` and `apps/aether-gateway/src/dispatch/pool_scheduler.rs`.
- PoolGroup preselection no longer passes its representative Key's concurrency, health or RPM record to scheduler-core. Provider quota/concurrency and request authentication capacity remain in their existing checks. The boolean helper delegates to the diagnostic helper so both paths apply the same boundary.
- The shared Pool page/sticky scheduler retains strongly read actual Key records and reuses `candidate_runtime_skip_reason_with_state` unchanged. It reads the same recent 128 candidates only when a configured concurrency/RPM limit needs them, with current Unix time and the existing Key RPM reset watermark.
- Existing Pool quota, account/auth, balance, catalog availability, cooldown and cost filters run first, retaining their diagnostic priority. Actual-Key concurrency/health/RPM filtering happens before active-probe fallback is decided and before the cursor truncates its candidate window. Thus an unavailable hot Key does not hide healthy cold Keys. Pool-specific circuit/cooldown policy is unchanged.
- Catalog read failures/missing requested records retain `pool_key_state_unavailable`. A failed required recent-counter read also fails closed with that infrastructure reason, never an administrator-recovery quota reason. Actual execution's strong Key identity read and atomic concurrency admission are untouched.
- No core policy/configuration/dependency, paging/scan ceiling, #824 score fallback/fixed-order behavior, SQL, frontend or quota-refresh change. No staging or commits were performed.

## Causal evidence

Before the fix, gateway `current_candidate_runtime_skip_reason` masked the representative's quota/auth/circuit flags but still passed `snapshot.provider_key_rpm_states`. Scheduler-core unconditionally applied representative Key concurrency, zero-health and RPM checks, preventing the group from reaching `PoolKeyCursor`. See `scheduler-audit.md` for the complete repository-to-execution trace.

The new regression was written before the production changes. A separate RED execution was not run: the root owns shared Docker Cargo compilation and requested one combined run after integration. The pre-fix failure is source-level causal evidence, not a claimed executed test result.

## Checks and regressions

Executed successfully by this implementer:

```text
rustfmt --edition 2021 --config skip_children=true apps/aether-gateway/src/scheduler/candidate/runtime.rs apps/aether-gateway/src/dispatch/pool_scheduler.rs
git diff --check -- apps/aether-gateway/src/scheduler/candidate/runtime.rs apps/aether-gateway/src/dispatch/pool_scheduler.rs
```

Cargo compilation/test execution is delegated to the root's shared Docker pipeline. New exact gateway test filters:

- `runtime_key_guards_do_not_block_pool_group_representative`: independent zero-health, Key concurrency and RPM cases; ordinary Key denied, representative allowed, boolean/diagnostic agreement, Provider quota/concurrency still denied.
- `pool_key_cursor_filters_real_key_runtime_guards_before_window_truncation`: PostgreSQL-style sole representative, 17 limited real Keys followed by healthy Key 18, normal and sticky paths, exact rejection evidence, no repeated denied candidate, and RPM reset recovery.
- `pool_runtime_guards_preserve_hot_pool_fallback_and_quota_reasons`: unavailable hot Key falls back to the cold Key without persistent eviction, all real Keys denied yields no candidates, manual quota and unavailable-state reasons retain precedence.

Keep existing focused coverage for `runtime_balance_fact_blocks_real_key_but_not_pool_group_representative`, `pool_key_cursor_skips_low_balance_sticky_key_and_falls_back_once`, #824 inactive-score/fixed-order regressions, runtime-quota/strong-read cases, and `provider_key_limit_rejects_concurrent_guard_until_release`.

## Limits

- This is a demonstrated local scheduling defect, not a confirmed diagnosis of a production request. No request ID or production replay was available.
- Existing recent-candidate accounting remains bounded to 128 records; this change does not replace the separate admission/accounting design. Atomic Key concurrency protection remains at execution admission.
- The first implementation handoff reports formatting/diff checks only. Root must record actual combined Rust test results before marking validation complete.
