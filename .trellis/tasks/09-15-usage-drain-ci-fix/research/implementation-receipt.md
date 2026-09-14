# Usage drain CI synchronization fix

- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-release-v0.7.34-ci-20260915`
- Branch/base: `codex/release-v0.7.34-ci` at `238c09382a44aebd4c3c4c83f5a3ff094b83cf6c`.
- Scope: test-only changes in `crates/aether-usage/runtime/src/runtime.rs`; no production behavior or dependency changes.

## Root cause and caller audit

`record_terminal_event` obtains an ordered barrier, persists the terminal event, and completes the ordered turn. Independently, `run_lifecycle_submission_worker` awaits the spawned slot execution before calling `LifecycleSubmissionState::record_processed`. A terminal caller can therefore finish while submission bookkeeping still reports one pending slot. Retry recovery metrics do not synchronize either lifecycle dispatcher.

All four existing callers of `wait_for_enqueue_dispatcher_to_drain` were read before editing:

- `first_byte_append_failure_is_retried_locally`: waits after submitting the first-byte persistence work; the expected recovered retry ensures fallback has run.
- `permanent_redis_and_database_failure_keeps_terminal_retry_tasks_bounded`: restores queue availability and joins/aborts submitted calls before draining.
- `slow_database_fallback_backpressures_excess_at_hard_capacity_and_recovers`: releases blocked writes and joins submitted calls, then asserts both lifecycle gauges are zero. This is the reported CI failure.
- `terminal_enqueue_limit_bounds_primary_burst_and_recovers_all_events`: joins every terminal submission before waiting for retry recovery and accounting checks.

The existing one-second and multi-second waits elsewhere in the test module already combine lifecycle submission and ordered lifecycle gauges. No existing shared helper provided this complete condition.

## Change

The shared drain helper now also requires `lifecycle_submission_pending == 0` and `ordered_lifecycle_pending == 0`. Its five-second deadline, one-millisecond polling interval, retry recovery condition, and all original caller assertions remain unchanged.

One regression, `enqueue_dispatcher_drain_waits_for_lifecycle_and_ordered_completion`, reuses `TestLifecycleSubmissionItem` and the real ordered barrier. It separately holds a submission worker and an ordered completion while the retry queue is empty, polls the actual helper once, and requires `Pending`. Releasing each held operation lets the helper finish. The second phase waits for submission bookkeeping to finish first, so removing either new predicate is independently detectable. No arbitrary delay or new fixture is needed.

## Validation

- PASS: `rustfmt --edition 2021 --check crates/aether-usage/runtime/src/runtime.rs`.
- PASS: `git diff --check`.
- NOT YET VERIFIED by implementer: compilation and runtime tests. The coordinator owns Cargo execution with a C-drive target because D-drive space is exhausted.

Run the new regression and the four caller tests listed above in package `aether-usage-runtime`, library target. Their full test names use the prefix `runtime::tests::`.

## Handoff

The coordinator owns independent review, final Cargo results, spec updates, commit/merge, exact-SHA CI, and release. The main Aether worktree and its Antigravity WIP were not modified by this implementation.
