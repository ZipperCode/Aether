# 修复发布 CI 的用量队列等待竞态

## Goal

Fix the asynchronous test drain race before v0.7.34 exact-SHA CI and release.

## Requirements

- Fix the proven test synchronization race without changing production queue behavior or weakening/removing assertions.
- Read all four callers of wait_for_enqueue_dispatcher_to_drain and the real lifecycle/ordered completion path before choosing the narrow shared wait condition.
- Keep all existing deadlines/capacity and final-zero assertions; wait on the state being asserted rather than adding arbitrary sleeps or rerunning CI until green.
- Preserve all unrelated main-worktree Antigravity WIP. Work only in this C-drive isolated worktree and merge back to master.

## Acceptance Criteria

- [x] The original failing test and directly affected caller tests pass locally; deterministic coverage demonstrates the missing wait if feasible with existing fixtures.
- [x] Scoped Rust formatting and independent review pass; no new dependencies or production behavior changes.
- [x] Commit the scoped fix and merge into master with WIP intact; coordinator resumes exact-SHA CI before creating v0.7.34.

## Notes

- CI run 34864784095 for 238c09382a44aebd4c3c4c83f5a3ff094b83cf6c: Test (Workspace Rest) ran 3821 tests, 3820 passed / 1 failed / 30 skipped. Failure is runtime::tests::slow_database_fallback_backpressures_excess_at_hard_capacity_and_recovers at runtime.rs:13117, lifecycle_submission_pending left 1 / right 0.
- Current helper waits only for retry recovered/pending, not lifecycle/ordered completion. This crate was unchanged relative to the previous green origin/master.
- D drive has only about 40 MiB free. Use host-native Rust and task-local C-drive target if possible; do not expand the shared Docker volume or clean user files. Coordinator owns Cargo execution.
