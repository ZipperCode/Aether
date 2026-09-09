# Upstream integration receipt

## Scope and state

- Status: `READY_FOR_CHECK`. All integrator validation processes have exited; product writes and Cargo cache ownership are returned to Root/check.
- Integrator worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-upstream-release-20260909`.
- Branch: `codex/upstream-release-20260909`.
- Baseline: `4f865ff53518885acf756250fb083ce6a86a50e5`.
- Source / retained `MERGE_HEAD`: `361952ada9e3d01ac180846cee497cd1f55ad1c3`.
- Merge base: `c7e403b410139d12a6189dda9c2bdf0c7c80782e`.
- Executed real `git merge --no-commit --no-ff 361952ada9e3d01ac180846cee497cd1f55ad1c3`.
- All 8 upstream commits are integrated; 117 source-touched paths yield 114 staged changed product paths. The other three already contain the source change: `auth_cookie_policy.rs` uses `next_back()`, architecture allowlist already includes `policy_nulls.rs`, and quota-test import formatting was already present.
- No merge commit, push, tag, release, service, database, real `.env`, credential, or main-checkout source modification by the integrator.
- Root authorized only the existing `D:/Project/GitHub/Aether/target` as an additional write scope for serial Cargo cache reuse.

## Conflict map (13 files)

| Path (under worktree) | Semantic resolution |
| --- | --- |
| `apps/aether-gateway/src/ai_serving/planner/standard/openai/responses/mod.rs` | Keep upstream test-module relocation once, retain local header-dependent fingerprint assertions and Chinese explanation. |
| `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs` | Adopt generic bounded `FirstSseSemanticEvent` and opening-event buffering, retain existing complete-record parser, local annotations and Gemini/Anthropic contracts. Update old Chat/Responses enum assertions and add split/multiline/control-record regression across LF/CRLF/CR. |
| `apps/aether-gateway/src/execution_runtime/stream/execution.rs` | Keep fork cancellation guard, response history/capture, first-Data owner and deferred ownership; adopt structured error before success regex, opening-event buffering, configured timeout, no image-success replay, new disconnect tests. Remove duplicate auto-merged first-Data recording and duplicate await of the same test JoinHandle. Keep unknown Responses event byte-preservation assertion. |
| `apps/aether-gateway/src/executor/orchestration.rs` | Retain exactly one task per heartbeat path and the fork deferred-exhaustion settlement. Add upstream conditional `tx.closed()` cancellation to that existing task; do not run the upstream and fork tasks twice. |
| `apps/aether-gateway/src/orchestration/classifier.rs` | Apply global routing rules while retaining persistent quota classification. A global HTTP 200 rule overlapping explicit quota exhaustion still selects credential-scoped `RetryQuotaExhausted`; regression added. |
| `apps/aether-gateway/src/orchestration/policy.rs` | Retain quota pattern diagnostics and add both global rule counts. |
| `apps/aether-gateway/src/routing/resolver.rs` | Adopt shared execution-policy deserialization/validation and lifecycle configuration; remove the replaced unused boolean parser. Sticky budget and static policy semantics remain. |
| `apps/aether-gateway/src/tests/ai_execute/lifecycle.rs` | Explicitly configure cancellation for old cancel tests, retain every attempted candidate's Failed/429/rate-limit assertion, adopt exhaustion response assertions. |
| `apps/aether-gateway/src/tests/ai_execute/stream_provider_gemini/local_chat.rs` | Use upstream nonempty successful SSE fixture but retain public no-`alt=sse` JSON-array output. |
| `apps/aether-gateway/src/tests/ai_execute/stream_provider_gemini/local_cli.rs` | Same resolution for three CLI/Vertex fixtures; private wrapper and OAuth assertions remain. |
| `apps/aether-gateway/src/tests/architecture/ai_serving.rs` | Retain existing historical-alias allowlist and explanation; final contents unchanged from baseline. |
| `frontend/src/features/routing/utils/routingPolicy.ts` | Extend the existing typed default policy with upstream failover policy, retain sticky and format-key priority behavior. |
| `frontend/src/views/admin/RoutingProfiles.vue` | Retain upstream global failover editor update function alongside existing execution/model policy flow. |

## Additional real compiler repair

The first cancelled-tests build found E0004 in `crates/aether-ai/formats/src/formats/context.rs`: upstream `FormatError::diagnostic()` omitted fork `ResponseBlocked` and `EmptyResponse`. Added exhaustive diagnostic mappings preserving code, format and available reason, plus one table-driven regression. This is the only additional manually changed product file outside the original conflicts.

## Preserved and replaced contracts

- Antigravity `crates/aether-provider/transport/src/antigravity/request.rs` is unchanged from baseline: both Git blob IDs are `facb3af834c4892adf54622c7cac420b8df1f621`. Schema-node-only `$schema` cleanup, literal/property names, extensions, `$ref`, aliases and public Gemini isolation remain.
- `chat_failover.rs` and `stream/error.rs` are unchanged. Existing first-error/split-error request counts, visible-output/stop behavior, raw bytes and terminal accounting assertions remain CI gates.
- Standard Chat/Responses now use `FirstSseSemanticEvent` rather than the old `FirstClassifiedBody` selection. This intentionally widens the recoverable window through role-only/created setup events while using complete SSE records. `FirstClassifiedBody` remains in use for non-SSE JSON, conversion/no-finalizer and explicit image prefetch; no parallel replacement enum was added.
- `maybe_record_first_stream_event_started` remains the single first-Data recorder during prefetch, including empty Data. Handoff reuses captured telemetry and does not reinitialize it. Its independent HTTP-not-yet-committed assertion remains.
- Request lifecycle defaults to continue after policy selection, drains disconnected bodies without collecting full response, and preserves admission/diagnostics. Explicit true cancellation continues to use existing attempt/finalizer terminal ownership. Before policy resolution cancellation remains immediate.
- Cancelled request billing uses the upstream server-calculated request-fee marker through usage record, wallet settlement and policy-cost reservation. Token/cache/image billing dimensions are zero; unconfigured fees remain void.
- Global transfer count tracks provider/endpoint/key changes, not same-key retry. Cumulative deadline is checked before the next attempt and does not kill an active attempt. Provider budget exhaustion remains provider-scoped.
- Memory auth revision/singleflight, lazy ranking/hydration, lightweight maintenance/model projections, batch reconciliation, Endpoint binding, balance/runtime quota/manual recovery and original capture behavior are preserved. No production changes to their untouched owners.
- Existing CI workflows, shared `tools/ci.py`, no-fail-fast behavior and build-version/worktree watcher remain untouched. No package/application version bump or fork publishing identity change.

## Verification

All commands use the isolated worktree. Cargo compilation is serial, Rust 1.95.0, existing MSVC/NASM/VS CMake, unset `CMAKE_GENERATOR`, `CARGO_INCREMENTAL=0`, dev/test debug=0, jobs=2, stack=16777216, and the Root-approved existing main target cache. No persistent environment changes.

| Check | Result |
| --- | --- |
| `cargo metadata --no-deps --locked --format-version 1` | PASS; 42 workspace packages. |
| `cargo fmt --all --check` | PASS after fixing one hand-edited `matches!` layout. |
| `git diff --cached --check`, unmerged paths, conflict-marker scan | PASS; U=0 and no product conflict markers. |
| `python tests/ci_contract_test.py` | PASS; shared dispatcher/env/dry-run/failure propagation/workflow gates. |
| `python tests/gateway_build_watch_test.py` | PASS; normal/worktree/packed/detached/archive/version scenarios, 24.79s. |
| `cargo test -p aether-routing-core --lib --locked` | PASS 26/26; build 27.04s. |
| `cargo test -p aether-ai-formats --lib diagnostic_tests --locked` | PASS 3/3 including new Gemini diagnostics; build 1m16s. |
| `cargo test -p aether-billing -p aether-usage-runtime --lib cancelled --locked` | PASS billing 1/1 and usage-runtime 9/9; build 3m00s. Initial attempt failed at FormatError compilation before any tests, fixed as above. |
| `npm --prefix frontend ci --no-audit --no-fund` | Installed 434 packages only in worktree, 41s; lockfile unchanged. Existing npm allow-scripts policy warned about four scripts; no approval policy bypass. |
| `npm --prefix frontend run type-check` | PASS. |
| `npm --prefix frontend run test:run -- src/features/routing/__tests__/routingPolicy.spec.ts src/features/routing/__tests__/routingFailover.spec.ts src/features/usage/utils/__tests__/diagnosticExport.spec.ts src/features/usage/utils/__tests__/failureDiagnostic.spec.ts` | PASS 4 files, 48 tests, 25.77s. An earlier npm-exec invocation selected the wrong Vitest root, found zero tests and failed setup; corrected only the invocation, no source/config workaround. |
| `cargo check -p aether-gateway --lib --locked` | PASS, exit 0, 8m26s; one production-library check, zero compiler diagnostics, no gateway test codegen/full suite. |

## Root/check follow-up

- Production-library check is complete. No overall runtime/Gateway/CI PASS is claimed from compile-only results.
- Full exact-SHA GitHub CI must exercise Chat counts and visible boundary, generic multiline/control SSE test, first empty Data accounting, Gemini public JSON-array fixtures, disconnected request/body/heartbeat/WebSocket lifecycle, global/provider budgets, quota+global-rule overlap, conversion diagnostics, DNS/SMTP/Tunnel, wallet queries and PostgreSQL settlement.
- Root-owned spec updates: stream lifecycle contract sections 3/4/6 must distinguish default continue vs explicit cancellation and replace exact Chat/Responses enum assertion with semantic-gate contract; runtime quota section 3 should record global-success-rule overlap preserving credential scope; conversion diagnostics should include fork blocked/empty response variants.
- No deployment, real provider calls, database runtime, UI visual inspection, full local Gateway/workspace test suite or release operation performed. Standard ignored worktree dependencies/build artifacts are for Root cleanup with the task worktree; main cache is intentionally retained. All integrator validation processes have finished; unrelated preexisting processes were not touched.
