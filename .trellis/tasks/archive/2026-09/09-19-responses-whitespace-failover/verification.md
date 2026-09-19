# Verification receipt

## Scope and baseline

- Repository: `D:/Project/GitHub/Aether`; branch `master`; baseline `e88651c7999886f603969af44311b2843648e321` with a clean working tree.
- Live investigation reference: `c65c1e30-8499-4928-946e-016c36da0492`, v0.7.37. Original failed preamble was not retained; the regression uses the opening-space shape confirmed in two adjacent xmapi captures.
- Product change is limited to Responses precommit text classification. No production model calls, configuration/database changes, deployment, push, or release in this task.

## Test environment

- Local Docker image: `aether-gemini-builder:20260917`, Rust 1.95.0. Source mounted read-only at `/build`; Cargo artifacts in `aether-trellis-target` at `/target`.
- Profile: `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_BUILD_JOBS=2`, `RUST_MIN_STACK=16777216`.
- Linker: `RUSTFLAGS=-C linker=clang -C link-arg=-fuse-ld=lld`.
- Host rustfmt is used for formatting only. Compilation and tests execute inside Docker, with local mock upstreams and memory repositories.

## Red regression

Command: `cargo test -p aether-gateway --lib --locked -- responses_whitespace --test-threads=2 --nocapture`.
Docker session 31612 exited 101 after 18m22s build and 1.19s execution: 3 passed, 4 failed, 5548 filtered out.

- Empty Responses text returned `Commit` instead of `Pending` in the classifier regression.
- Both gateway error/split scenarios called providers `[1, 0]` rather than `[1, 1]`; the first provider's opening space was followed by a 503 and the second provider was never contacted.
- The fourth failure was a separate fixture defect: its incomplete successful Responses payload triggered existing compatibility fields (`created_at`, `completed_at`, `output_text`). Complete the mock response rather than weakening byte equality or changing compatibility normalization.
- Read-only test review requested distinct A/B preamble markers and exact provider/candidate status attribution. Those assertions will be strengthened with the production patch.

## Implementation and checks

- Production change: 19 lines at the existing Responses classifier. Four exact text-delta types and known opening text fields treat string whitespace as pending; the shared semantic helper, bytes, limits and error paths are unchanged.
- Fixture corrections preserve strict byte equality: complete successful terminal fields and distinct A/B response/item IDs. Assertions exclude failed-provider preambles and require A Failed/503 and B Success/200 individually.
- Host scoped rustfmt, `cargo fmt --all --check`, and `git diff --check` passed.
- Green Docker session 12514 passed: **48 passed, 0 failed, 0 ignored**, 5507 filtered out; 10m27s build, 10.42s execution.
  Filters: `responses_whitespace`, `chat_stream_failover`,
  `execution_runtime::stream::commit_policy::tests`, `generic_sse_`,
  `generic_stream_`, `same_format_responses_prefetch_`,
  `prefetched_codex_cyber_`, `execute_execution_runtime_stream_records_first_`,
  `prefetch_handoff`, `gateway_production_body_collection_stays_bounded`.
  These cover all 7 new tests and 41 directly related regressions, including
  first-byte lifecycle and v0.7.37 replay integrity.
- Final independent full-scope static review passed with no unresolved findings. It verified both earlier test-proof corrections, exact predicate scope, error precedence, first-byte accounting, replay and specs. The reviewer independently passed scoped rustfmt and diff checks; it did not claim unrun Cargo checks.
- The builder has no Python executable, so the first attempt to invoke
  `python3 tools/ci.py clippy-gateway` could not start (exit 127). Run the exact
  Cargo argument list owned by that dispatcher with its same profile env;
  do not install an unnecessary runtime into the builder.
- Clippy passed: Docker session 9695, exit 0, 8m30s. Executed
  `cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings`,
  matching the dispatcher argument list and CI profile environment.

## Green test command

Run inside the Docker environment above:

```text
cargo test -p aether-gateway --lib --locked -- responses_whitespace chat_stream_failover execution_runtime::stream::commit_policy::tests generic_sse_ generic_stream_ same_format_responses_prefetch_ prefetched_codex_cyber_ execute_execution_runtime_stream_records_first_ prefetch_handoff gateway_production_body_collection_stays_bounded --test-threads=2 --nocapture
```

No full-workspace, frontend, real PostgreSQL, CI/release or live-deployment validation is claimed. Those surfaces are outside this classifier-only repair.
