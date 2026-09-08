# Exclusive full-merge check

## Scope and ownership

- Review worktree only: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`.
- Uncommitted merge parents remain `b0ad8ff7f7bdb1888abac8c06f47f61e76761431` and `c7e403b410139d12a6189dda9c2bdf0c7c80782e`.
- Full upstream scope is authorized, including VSCodex, PostgreSQL-only, raw Header capture and root container execution. Older feature exclusions are superseded.
- Exclusive check did not spawn agents or change production behavior. It repaired three test fixtures described below. Root owns task lifecycle, specification reconciliation, final commit, master integration and cleanup.

## Review findings

No production correctness defect found in the targeted integration spotchecks:

- All 217 paths added by upstream since the merge base exist in the merged worktree. VSCodex has the public WebSocket route, user API and frontend route.
- AntiGravity auth still performs revision-only reads before the singleflight revision/value load; null and invalid outcomes are revision-cached. Gateway memory string CAS advances revision only on success; delete remains a tombstone.
- Candidate ranking remains transport-free and retains named time/seed semantics. The 2,048-candidate regression is present and included in the required execution below.
- Model-fetch uses compact target/reconciliation projections, accumulates successful Provider IDs and reconciles each once per batch. Automatic Endpoint binding replacement retains successful Endpoint authority.
- Default model-association UI loads aggregate discovery, guards pending Save, fences stale sessions and sends exact upstream model/Endpoint IDs. Export retains binding source/active state; import prevalidates and remaps Endpoint IDs for both creation and updates.
- Pool scheduling separates unavailable catalog state, persistent runtime quota block, subscription exhaustion and balance. Known subscription exhaustion is unconditional. Manual quota recovery uses the projection lock, strong re-read and score reconciliation without clearing unrelated hard states.
- HTTP cloned request parts share one immutable Codex context; WS/Live retry paths restore it. Provider transport retains the old type name as an alias of the new shared outbound context.
- Unified stream cancellation Guard takes terminal ownership once; admission errors retain Failed/429. Watchdog begins after admission, marks abandonment before dropping execution and does not interrupt started terminalization. Deferred response accounting consumes one extension carrying the actual response-producing candidate.
- Native Responses first-body error classification remains before 2xx commit. Same-format preservation, opaque/private IDs and visible Gemini thought contracts remain covered by the retained implementation and earlier format regressions.
- Dockerfiles use `USER 0:0`; default images remain `ghcr.io/zippercode/aether`, packaging targets fork `master`, and Tunnel is `0.3.17`. No obsolete MySQL/SQLite driver modules or enum branches remain in product source. Optional sqlx packages in Cargo.lock are not active application backends.

### Fixed test integration defect

- Files: `apps/aether-gateway/src/execution_runtime/stream/execution.rs` and `apps/aether-gateway/src/execution_runtime/sync/execution.rs`, only the two `records_admission_timeout_once_as_429` tests.
- Initial run compiled successfully in 18m46s, then both tests failed: persisted candidate and usage already passed Failed/429/void assertions, but `client_response_body /error/type` was absent.
- Root cause: the merged `GatewayDataState::body_capture_policy` defaults missing `request_record_level` to `Basic`, and `UsageRuntime::record_terminal_event_direct` applies it before persistence. Both Guards construct the correct error body; the old fixtures implicitly assumed full capture. The default has its own existing regression.
- Fix: explicitly seed `request_record_level = "full"` using the existing `with_system_config_values_for_tests` helper in both fixtures. Preserve every original assertion, including `gateway_admission_timeout` and no rejected second terminal submission; add substantive Chinese explanations. Production Basic default and raw Header/body semantics are unchanged.
- Scoped rustfmt `--check` and `git diff --check`: PASS immediately after the edit. Both failures passed on retry and again against the final binary.

### Fixed read-only candidate fixture

- File: `apps/aether-gateway/src/ai_serving/planner/candidate_materialization.rs`, only its test imports and the 2,048-candidate regression.
- Initial assertion saw `candidate_count = 0`. A retained failure-only diagnostic exposed the deferred error: `stored provider catalog credentials require migration but the catalog writer is unavailable`.
- The old fixture cloned one bare Fernet credential across all Keys. The merged strong-read path correctly requires a credential migration for that legacy envelope, but this intentionally read-only fixture has no catalog writer.
- Fix: use the existing `AppState::seal_provider_catalog_key_api_key` with one explicit development credential state and each actual Provider/Key ID. This matches the sibling candidate-source/ranking fixtures and needs no writer, migration path or production change.
- Preserve the full 2,048 candidates, zero hydration reads while ranking, first-Key invalidation, two reads only after fallback, and empty static drain assertions. The count failure message now exposes a deferred error without evaluating `next_attempt` on the passing path.
- Scoped rustfmt and diff whitespace checks PASS. The changed single filter passed in session `91242` (exit 0): incremental test build 4m02s; 1 passed in 3.20s. Log: `target/full-merge-check-gateway-candidate-retry.log`.

## Specification follow-up owned by Root

Three exact remaining old-contract descriptions were reported to Root, who accepted them for the final documentation-only pass:

1. `aether-gateway/backend/quality-guidelines.md`: lightweight projection checks still name MySQL/SQLite; narrow to memory/PostgreSQL.
2. `aether-routing-core/backend/routing-ordering-sticky-retry-contract.md`: retire legacy scheduler-key fallback and legacy-derived bootstrap descriptions to match current mandatory resolved policy/default-group behavior.
3. `aether-provider-pool/backend/key-admission-affinity-model-quota-contract.md`: replace old model-family min/range UI aggregation with upstream account `quota_group` weekly/five-hour windows; retain atomic admission, model-scoped quota and exact Endpoint contracts.

These findings do not authorize restoring retired product semantics. No unrelated historical spec audit was expanded.

## Reused valid verification

Only three gateway test fixtures were modified during this check. The final (not intermediate) evidence in `integration.md` and `frontend.md` remains valid for unchanged production, packages, frontend and configuration; the gateway lib-test target is rebuilt and rerun below:

- `cargo check --workspace --all-targets --locked`: PASS, 7m43s; gateway lib/all-targets and locked metadata also PASS.
- Rust format, API coverage generation check, workflow YAML parsing, shell syntax, supply-chain fixture and four read-only Compose fixture configurations: PASS.
- Frontend type-check: PASS. Scoped non-mutating ESLint: 0 errors, retained style warnings. 423 unique selected logic/state tests: PASS. Node 24.18.0 differs from CI Node 22.
- Ten smaller-crate Rust filters: 17 passed, zero failed/ignored.
- This is a local integration gate, not a claim of full CI, whole-gateway/frontend suite, Clippy, browser/visual, live database/provider or deployed-runtime validation.

## Required gateway behavior execution

The original five-filter sequence used `cargo test -p aether-gateway --lib <filter> --locked` serially. After two evidenced fixture repairs and the repeated worktree build-script issue, Root authorized equivalent direct execution of the same compiled test binary as detailed below. Existing MSVC/NASM/CMake were reused with `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_BUILD_JOBS=2`, unset `CMAKE_GENERATOR`, and `RUST_MIN_STACK=16777216`.

- Start: 2026-09-08 14:40:22 Asia/Shanghai; tool session `32967`, Cargo PID `38136`.
- Log: ignored worktree `target/full-merge-check-gateway.log` (temporary; final result is retained here).
- Fixture-only scope; the worktree has no `.env`. No service, real database or provider request was started.
- First chain finished with exit 101: 0 passed, 2 failed in the admission filter, so its `&&` sequence did not execute the remaining four filters.
- Retry started at 15:02:03 with unchanged build environment/cache, tool session `86270`, Cargo PID `46876`, gateway rustc PID `41344`. Log: `target/full-merge-check-gateway-retry.log`; the gateway lib-test target was rebuilt after the fixture-only edits.
- Retry produced 2 admission passes (4m18s incremental build) and 14 watchdog passes (1m41s repeated build). `build.rs` watches `../../.git/HEAD`, which is absent because this worktree's `.git` is a file; that pre-existing unchanged script causes unnecessary Cargo rebuilds.
- Root authorized stopping the next task-owned redundant Cargo build and directly running the unchanged binary. Session `86270` was interrupted with exit 1; no matching Cargo/rustc remained. The binary at that point was `target/debug/deps/aether_gateway-34eee8ab4f33e82f.exe`, SHA-256 `DFF9BA29E6C7499DF0CED5C0E08E22EED18CBC0703E42A11BB56218E752FB875`.
- Direct `memory_system_config_` passed 3 tests; direct native Responses filter passed 1. Candidate regression failed as described above; its diagnostic build finished in 4m00s and exposed the exact deferred credential-migration error.
- Final candidate retry built the complete gateway lib-test target and passed. The other four filters then ran serially as `<final-exe> <filter> --nocapture` against that exact unchanged binary, with `RUST_MIN_STACK=16777216`; all passed, final process exit 0. No build-system workaround or configuration edit was introduced.
- Final binary: `target/debug/deps/aether_gateway-34eee8ab4f33e82f.exe`; SHA-256 `59FC2BF253D7F30A5C90844F713483E9C9CDD6A74727E259CCFA09AD53A364C2`.
- Final evidence logs: `target/full-merge-check-gateway-candidate-retry.log` and `target/full-merge-check-gateway-final.log`. Counts below are unique tests, not accumulated successful retries.

| Filter | Result |
| --- | --- |
| `records_admission_timeout_once_as_429` | PASS: 2, 1.37s |
| `stream_candidate_watchdog_` | PASS: 14, 0.51s |
| `memory_system_config_` | PASS: 3, 0.00s |
| `routed_ranking_hydrates_only_selected_candidates_in_fallback_order` | PASS: 1, 3.20s |
| `same_format_responses_prefetch_retries_bare_error_before_committing_success` | PASS: 1, 2.46s |

**Check: 21 unique gateway tests passed, zero final failures/ignored. Implement: 17 separate smaller-crate tests passed. Combined: 38 unique Rust tests, plus 423 selected frontend logic/state tests.** The initial admission failures and candidate failure/diagnostic rerun are not counted again.

## Final handoff

READY for Root's documentation reconciliation and final merge integration. No unresolved product/test failure remains. The three exact Root-owned specification updates above are still required before final task completion.

- Changed/staged check paths: the stream and sync admission test files, candidate materialization test file, and this report.
- Final scoped rustfmt `--check` and normal/staged diff whitespace: PASS. Unresolved index entries: 0; unstaged paths: 0; HEAD and MERGE_HEAD remain the original two parents. No whole workspace/frontend rerun was necessary after fixture-only edits.
- No commit/push/archive/master change was performed. No existing `.env`, service, database, provider or deployment was touched.
- Every check subprocess has exited or the explicitly authorized redundant task-owned build was interrupted. Ignored logs, build cache and previously installed worktree-only dependencies remain as review evidence for Root's worktree cleanup; no debug switch or temporary production diagnostic was added.
