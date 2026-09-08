# Build-watch CI color failure receipt

- Status: READY, 2026-09-08. Owner changed only `tests/gateway_build_watch_test.py` and this receipt. No commit, push, workflow mutation, real database, service, release, or deployment operation.
- Remote baseline: `948c1c16f2f9927b37a8570b86767128abd6b7c8`; Rust CI run `34210248162`, Format job `102009266027`.

## Confirmed cause

The completed job's raw log was read with `gh api repos/ZipperCode/Aether/actions/jobs/102009266027/logs`; only matching environment/error lines were emitted, with ESC represented as `\x1b`.

```text
2026-09-08T09:29:41.2477261Z   CARGO_TERM_COLOR: always
2026-09-08T09:29:41.6951548Z     assert "Fresh aether-build-watch-fixture" in output, output
2026-09-08T09:29:41.6952852Z AssertionError: \x1b[1m\x1b[92m       Fresh\x1b[0m aether-build-watch-fixture v0.0.0 (.../checkout/apps/gateway)
```

Cargo's ANSI reset is between `Fresh` and the package name, so rendered text looks correct but the exact substring is absent. The unchanged check already reported zero build-script executions. This is a test/tool-output boundary failure, not a Rust formatting failure or evidence of an incorrect build-script watch. The same failure reproduces on Windows with the CI environment; it is not a Linux-only limitation.

## Minimal change

`check()` is the fixture's single Cargo invocation owner. Add Cargo's native `--color never` argument there, with a Chinese comment explaining the parsed-output contract. Caller environment/profile values remain inherited, and only this Cargo command's display color is fixed. No ANSI parser, environment reset, retry, assertion removal, new dependency, production build-script change, or workflow change.

All 24 real Cargo checks remain: normal and linked checkouts, unchanged and advanced branches, packed-only and packed-to-loose refs, detached HEAD, source archive, version precedence/tunnel-tag exclusion, and explicit build type. Existing execution-count, Fresh, saved-version/type, and exact-file watch assertions are unchanged.

## Commands and results

Native host: Windows; `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`. Commands use a fresh `pwsh -NoProfile` process from `D:\Project\GitHub\Aether`; the assignments below do not change the parent environment.

Before and after the change, run the existing fixture with exactly the workflow's four global Cargo environment values:

```powershell
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:CARGO_TERM_COLOR = 'always'
python tests/gateway_build_watch_test.py
```

- Before: FAIL, exit 1 on the second Cargo check at the original line 51. Initial build executed once; unchanged build executed zero times; local assertion showed the same `Fresh\x1b[0m aether-build-watch-fixture` fragment as Actions.
- After, CI environment: PASS, exit 0, 24 checks, 9 with `executions=0, expected=0`, 15 with `executions=1, expected=1`; fixture reported 30.65 s.
- After, ordinary unmodified local environment (`python tests/gateway_build_watch_test.py` in a separate fresh shell): PASS, exit 0, the same 24/9/15 counts; fixture reported 32.49 s.
- The two after-change fixtures ran concurrently in distinct system-temp packages. Timings are actual local elapsed values, not comparable CI speed measurements.
- `python tests/ci_contract_test.py`: PASS for dispatcher commands, inherited/owned environment, dry-run, failure propagation, workflow gates and triggers.
- `git diff --check -- tests/gateway_build_watch_test.py`: PASS. Git emitted only the repository's LF-to-CRLF checkout advisory, not a whitespace error.

## Cleanup and limits

- Pre-run and final `aether-build-watch-*` system-temp directory inventories were empty. `TemporaryDirectory` cleaned up the failed and successful fixtures; both after-change subprocess sessions finished with exit 0. No task-owned persistent process or standalone diagnostic file was created.
- No full gateway/workspace/frontend/real-database compilation or tests were run. The copied real `build.rs` was compiled/type-checked by the dependency-free fixture.
- Linux/Actions validation of the final exact SHA remains Root's follow-up after collecting all first-run failures and combining the fixes. This receipt does not claim the overall CI run is successful.
