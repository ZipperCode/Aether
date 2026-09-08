# Second CI: usage terminal ordering

- Assigned failure: `runtime::tests::terminal_seed_waits_for_its_ordered_turn_before_admission`, `aether-usage-runtime`; CI `34217818968`, source `9026380d1957e7a57a746a409335f15eb1675515`.
- Status: READY; write ownership and exclusive Cargo slot returned to Root.
- Write ownership: `crates/aether-usage/runtime/src/runtime.rs` and this report only. Root granted the exclusive Cargo slot after all three other small-crate writers finished.

## Observed path

- `record_pending` builds a pending event in the usage background runtime and blocks in `BlockingPolicyQueueConfiguredUsageStore::body_capture_policy` before ordered dispatch.
- `record_sync_terminal` hands off its seed without entering terminal admission until the earlier pending phase completes. The original test correctly asserts zero terminal pending/in-flight before release.
- Pending uses `PendingPersistenceItemImpl` and the fixture's successful no-op usage writer. Only terminal queues an event, so the expected single successful append remains correct.
- Body policy loading holds the runtime-local cache mutex, then caches the successful policy for 30 seconds; the two-second test does not require another policy release. Upstream Basic capture and cost reconciliation changes do not alter this schedule.
- The fixture signals `policy_started.notify_one()` before constructing `release_policy.notified()`. The test runtime can respond with `notify_waiters()` in that gap; no future yet owns that notification generation. Tokio 1.50.0's `Notify::notified` snapshots the broadcast generation at creation, so a later future misses that release.
- All three fixture callers were inspected: the original ordering case, terminal-seed backlog, and terminal submission admission recovery. No production scheduler race or append-count mismatch is established.

## Root cause and minimal fix

- Added `blocking_policy_store_keeps_release_during_start_notification`: a standard-library `Wake` callback releases policy synchronously during the start notification. This fixes the exact interleaving without sleep, increased timeout, retry, or probabilistic stress.
- RED: with the original helper, the witness failed `policy release during start notification must not be lost`; the policy future remained Pending after the only release broadcast. This is a deterministically reproduced fixture race, not evidence of production scheduler deadlock or merely an unproven flaky-test label. The exact CI thread interleaving itself was not logged.
- Fix: construct the release future before publishing the start signal, then await that future. The stored broadcast generation now observes release even before the first poll. One shared helper fixes all three callers.
- Original ordering, admission, queue append and recovery counter assertions are unchanged. No sleep, timeout increase, retry, ignore, production scheduler/capture/cost change, or new dependency.

## Verification

Environment was local to each validation process: `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_BUILD_JOBS=2`, `RUST_MIN_STACK=16777216`; existing main target cache, pinned Rust toolchain, no global environment changes.

1. RED: `cargo test -p aether-usage-runtime --lib runtime::tests::blocking_policy_store_keeps_release_during_start_notification --locked -- --exact --nocapture`; session `35785` exited 1, build **2m09s**, execution **0.00s**, **0 passed / 1 failed / 253 filtered**.
2. GREEN after the shared-fixture change: same command; session `54804` exited 0, rebuild **24.08s**, execution **0.00s**, **1 passed / 0 failed / 253 filtered**.
3. Same final test binary, separate exact-filter invocations with `--exact --nocapture`:

   | Test | Result | Runtime |
   | --- | --- | --- |
   | `runtime::tests::terminal_seed_waits_for_its_ordered_turn_before_admission` | 1/1 PASS | 0.01s |
   | `runtime::tests::terminal_seed_backlog_does_not_create_per_request_admission_waiters` | 1/1 PASS | 0.01s |
   | `runtime::tests::terminal_submission_admission_backpressures_and_recovers_all_events` | 1/1 PASS | 0.02s |

4. Final single-file `rustfmt --edition 2021 --check` and scoped `git diff --check`: PASS. Diff review confirms both hunks are inside the test module, all original three tests are unchanged, and no temporary debugging remains.

Final result: **4 distinct targeted tests PASS**. Artifact: `target/debug/deps/aether_usage_runtime-41bec7712d3918ad.exe`; SHA-256 `C5374CB4D29150CDE6248EFC7BAA9EFB6263F9F9E4577759531F75BC83731896`.

No whole-usage/workspace/Gateway suite, standalone Clippy, real database/provider, or final-SHA Linux CI validation was run by this owner. Root owns the remaining batch checks and actual CI certification; this native result is not a remote-CI success claim.

## Scope and cleanup

- No production edit, dependency/configuration change, Git write, CI operation, database/provider access, persistent process, or temporary diagnostic file. Both Cargo sessions and all test processes exited; ordinary Cargo cache/test artifacts remain.
- Root owns final verification, task/spec updates and commit/push.
- Suggested spec lesson for Root: cross-runtime blocking fixtures must establish the release `Notified` before publishing the start signal when release uses `notify_waiters`; deterministic wake callbacks can validate this ordering without timing-based retries.
