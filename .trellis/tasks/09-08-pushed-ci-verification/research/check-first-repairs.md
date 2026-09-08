# First pushed-CI repairs review

- Status: READY, 2026-09-08. Reviewer has returned the eight-file repair write set to Root and stopped writing.
- Reviewed baseline: `948c1c16f2f9927b37a8570b86767128abd6b7c8`; first Rust CI run `34210248162`. Live run collection remains the separate reader's responsibility.
- Loaded the complete saved hook context, task PRD/design/implement/check context, all six current failure receipts, scoped AGENTS/specs, and `trellis-check`/`ponytail` instructions. No agent dispatch, approval gate, Git mutation, CI mutation, database/provider call, dependency install, or full build/test was performed.

## Findings (fixed)

No additional product defect was found; no repair source needed changing during review. The existing repairs were checked against current code and callers, not accepted solely from their receipts:

- `tests/gateway_build_watch_test.py`: the single real Cargo invocation fixes only parsed display color with native `--color never`. All 24 execution-count/version/Fresh/watch checks remain; inherited Cargo profile inputs are retained. No production `build.rs` change or assertion removal.
- `frontend/src/i18n/__tests__/i18n.spec.ts`: exact expectation matches the existing `legacyUiEnglishMessages` override, whose spread follows the base phrase. The actual model-mapping empty state still uses the same Chinese key. No dictionary or runtime priority change.
- `frontend/src/features/providers/components/__tests__/provider-key-batch-import-layout.spec.ts`: current button condition, click-to-open action, and dialog non-null/provider-type condition are asserted. The real button and component are present; the shared predicate still excludes OAuth account providers, not all providers except custom. The other import/update assertions remain.
- `apps/aether-gateway/src/handlers/public/support/auth_cookie_policy.rs`: `next_back()` selects the same final header row as `last()`, then the unchanged reverse split selects its final comma item. The real caller invokes this helper only after proxy trust resolution; HTTPS precedence, overrides, invalid-value behavior, and cookie rewriting remain unchanged.
- `crates/aether-data/contracts/src/repository/provider_catalog/types.rs`: scheduling CAS Debug intentionally omits sensitive fields. Exact safe output equality still rejects all four retained canaries and any added field; the generic redaction helper and three other CAS/fence checks are unchanged. No production Debug or CAS/serialization change.
- `crates/aether-data/runtime/schema/drivers/postgres/baseline/001_types_and_tables.sql`: only the prematurely backported historical revision-column line is removed. The existing separate incremental migration and current logical/generated/bootstrap schema retain the column. Executable history and the full SQL equality test are unchanged.
- `.github/workflows/rust-ci.yml` and `tests/ci_contract_test.py`: only Data/Workspace Rest gain `--no-fail-fast`, retaining failure exit status and all original targets/exclusions. The added regression checks exact commands and forbids ignored failures. This addresses the observed fail-fast collection limitation, not a seventh failing component.

## Findings (not fixed)

- No remaining code finding within the assigned paths. Root must still combine/push the repairs and certify the final exact SHA on Linux; this review does not claim the first run or final CI is successful.
- At review handoff, Root reported the first Gateway run completed with 35 behavior-test failures and the reader was extracting the full matrix. Those newly reported failures are outside this eight-file review; they were not investigated or claimed fixed here and require Root's next bounded assignments.
- Root-owned spec follow-up: extend the existing shared CI quality scenario with explicit no-color input for parsed Cargo output and Data/Workspace Rest all-failure collection. Existing schema source guidance already describes source/generated/bootstrap ownership; these repairs introduce no new product contract.

## Verification

Fresh reviewer checks, all PASS:

- Lint: read-only ESLint on the two changed frontend tests; `rustfmt --edition 2021 --check` on the two changed Rust files; scoped `git diff --check` on all eight repair files. No autofix or lint allowance. Full Linux Clippy remains the remote gate, not a claimed local pass.
- TypeCheck: existing TypeScript compiler API loaded `tsconfig.app.json` and checked only the two changed test roots, their actual imports, and existing ambient declarations with `noEmit`; PASS in 4.61 s. No full frontend build or project-wide type-check. The real copied Rust build script was compiled/type-checked by the fixture below; unchanged native Rust owner results remain valid.
- `python tests/ci_contract_test.py`: PASS for actual dispatcher invocation with mocked Cargo, command/env ownership, dry-run, failure propagation, workflow targets/gates/triggers.
- `python tests/gateway_build_watch_test.py` with `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_TERM_COLOR=always`: PASS, 24 checks, 9 unchanged with zero executions and 15 initial/changed with exactly one execution, 17.10 s. Session `39816` exited 0.
- Full current/baseline workflow parsed with installed PyYAML BaseLoader. After appending only the two intended flags to the baseline command values, the complete objects are equal. All 17 job definitions, matrix configuration, gates, dependencies, permissions, env, action pins, feature/adapter/integration checks, and mandatory PostgreSQL remain unchanged.
- Historical manifest byte composition: PASS, 137,639 bytes; SHA-256 `D964C8F630B162F751FD6CD1D48E2EB9EC8981D950267643E5F967B234090915`, equal to the immutable baseline and the pre-edit receipt. `git diff --quiet HEAD` confirms executable migrations, current bootstrap/logical/generated schema, and the original migration test are unchanged.

Valid owner runtime results reused; no related source/input changed afterward:

- Frontend: two targeted files, 11/11 tests PASS after both original assertions reproduced as failures. Local Vitest is 4.0.10 versus locked 4.1.11; final remote locked-dependency verification remains required.
- Auth cookie: all 9 existing inline native tests PASS using real current helper/test code and native dependency artifacts; gateway/environment composition was outside that harness.
- Data contracts: `cargo test -p aether-data-contracts --lib provider_catalog_cas_debug_output_redacts_credential_fences --locked`, original filter FAIL before repair and 1 PASS after.
- Data migration: `cargo test -p aether-data --lib lifecycle::migrate::tests::split_baseline_sources_match_executable_migrations -- --exact`, original strict test 1 PASS, including current build-generated bootstrap composition.

## Handoff and cleanup

- Reviewer changed only this receipt; all repair sources and other owners' work are preserved. No staging/commit/push, tag/release/deployment, task state edit, or generated schema mutation.
- No main Cargo target build or new dependency cache was started. All review subprocesses exited; final `aether-build-watch-*` temp inventory is empty. No reviewer-owned persistent process, standalone diagnostic file, emitted TypeScript file, or temporary fixture remains.
- Root owns spec/task integration, commit/push, remaining first-run failures, and final exact-SHA evidence. No full gateway/Data/workspace/frontend/real-DB test result or CI-speed improvement is claimed here.
