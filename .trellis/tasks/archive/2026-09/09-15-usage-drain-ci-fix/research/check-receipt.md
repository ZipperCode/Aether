# Independent review receipt

## Scope

- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-release-v0.7.34-ci-20260915`.
- Branch/base: `codex/release-v0.7.34-ci`, `238c09382a44aebd4c3c4c83f5a3ff094b83cf6c`.
- Read the task check context and PRD, usage-runtime backend quality guidelines, all four original drain-helper callers, lifecycle submission workers, ordered completion guards, terminal barrier handoff, and existing blocking test fixtures.
- The source diff is wholly within `#[cfg(test)] mod tests`; production code, the gateway SSE fix, main-worktree WIP, dependency manifests and deadlines are unchanged.

## Findings (fixed)

- File: `crates/aether-usage/runtime/src/runtime.rs`, new regression at the two drain-future polling sites.
- Issue: the coordinator's first native Cargo attempt reported `E0277`; directly passing the async helper future to `futures_util::poll!` requires `Unpin`, which this future does not implement.
- Fix: pin each helper future with standard-library `std::pin::pin!`, then poll the resulting `Pin<&mut _>`. This reuses an existing local pinning pattern and retains the deterministic assertions without heap allocation or new dependencies.

## Findings (not fixed)

None in the reviewed source. Executable verification after the pinning correction is still coordinator-owned; this receipt does not declare the pending Cargo checks passed.

## Root cause and caller review

- `run_lifecycle_submission_worker` decrements submission pending only after its spawned slot execution has returned. A terminal barrier can already deliver an `OrderedLifecycleCompletion` to the caller before that worker bookkeeping runs. Awaiting terminal calls or observing an empty enqueue retry queue therefore does not imply `lifecycle_submission_pending == 0`.
- Ordered lifecycle pending is separately released by `OrderedLifecycleCompletion::finish`. The helper now requires both lifecycle and ordered pending to be zero in addition to its unchanged retry-recovered target and retry-pending-zero conditions.
- All four existing callers release their work before entering the shared helper; none intentionally retains an ordered guard or blocked writer across the wait:
  - `first_byte_append_failure_is_retried_locally`: first-byte submission has returned; the helper awaits the actual local retry recovery before validating persisted event fields.
  - `permanent_redis_and_database_failure_keeps_terminal_retry_tasks_bounded`: queue failure is disabled and submissions have completed or been cleaned up before drain; existing bounded-task and persistence assertions remain intact.
  - `slow_database_fallback_backpressures_excess_at_hard_capacity_and_recovers`: the writer is released and terminal submissions are joined before drain; the new conditions wait for precisely the final zero counters asserted by the original failing test.
  - `terminal_enqueue_limit_bounds_primary_burst_and_recovers_all_events`: all burst submissions are joined before drain; recovered totals, exact event count, concurrency limits and zero in-flight assertions are unchanged.
- The shared five-second timeout and one-millisecond polling interval are unchanged. No assertion was removed, relaxed or moved to conceal the original failure.

## Regression review

- The first phase holds a real `TestLifecycleSubmissionItem` with the existing `Notify` fixture and verifies that a one-shot poll of the helper is pending while lifecycle pending is exactly one and retry/ordered pending are zero.
- The second phase holds a real ordered completion guard and first waits for lifecycle submission bookkeeping to finish. This independently proves that ordered pending also prevents early drain success.
- Each held resource is released before asserting the poll result, then the real helper is awaited to prove recovery. `notify_one` retains a permit if the worker has not yet reached its release await, avoiding a lost `notify_waiters` notification race.
- The original helper would immediately return ready in the first phase because its retry conditions are already satisfied. A partial fix adding only lifecycle pending would fail the second phase. Actual execution of those mutation cases is not claimed here.

## Verification

- Rust formatting: **pass**, `rustfmt --edition 2021 --check crates/aether-usage/runtime/src/runtime.rs` after pinning corrections.
- Whitespace/diff: **pass**, `git diff --check`.
- TypeCheck: **not verified by reviewer**. The coordinator observed the initial `E0277`; the corrected source is ready for the coordinator's native C-drive Cargo rerun.
- Tests: **not verified by reviewer**. The new deterministic regression and four affected caller tests remain coordinator-owned executable checks.
- No Cargo, Docker, release, merge or main-worktree operations were launched by this reviewer.

## Reviewed source hash

`runtime.rs` SHA-256: `00C67F54F6F01E6033D32874698F87B90213697E354A7206942A9A1E1A63052C`.
