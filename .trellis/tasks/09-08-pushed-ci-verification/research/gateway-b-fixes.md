# Gateway B: execution, capture, finalization CI repairs

- Status: READY; B writer stops after this receipt and scoped checks. Root owns integration, Cargo/CI verification, commit, and push.
- Baseline: `948c1c16f2f9927b37a8570b86767128abd6b7c8`, Rust CI `34210248162`, Gateway job `102009265753`.
- Scope: failure rows 4–12, 14, 31–35 (15 exact tests). Seven assigned Rust files plus this report only. No lock expansion requested or required.
- Production/test distinction: all Rust changes are inside existing tests or their test-only helpers. Production capture, persistence, admission, retry, response headers, conversion, terminal guards, and accounting code are unchanged.

## Root causes and retained assertions

| Rows | Established source evidence | Minimal repair and retained coverage |
| --- | --- | --- |
| 4, 31–35 | These bodyless/lightweight fixtures explicitly configured `request_record_level = full`. `UsageBodyCaptureEngine::apply_to_payload` keeps Full bodies and marks Basic bodies Disabled. The failed assertions expect absent bodies/Disabled. | Explicit Basic in the six affected fixtures (Windsurf has two state constructions). Keep response conversion assertions, captured raw Authorization header, status/billing/route/candidate identity, retry count, timing, skipped-candidate reason, large-body and deep-metadata checks. Do not disable Full globally. |
| 10–11 | The heartbeat fixtures did not set capture level; `GatewayDataState::body_capture_policy` defaults to Basic. The deferred recorder sets Unavailable because it does not own/consume the returned body, but Basic then correctly changes that to Disabled. | Explicit Full in the two deferred heartbeat fixtures. Keep Unavailable, shell HTTP 200 versus upstream error, original response headers, and exactly-once submission assertions. |
| 12 | Same default-Basic/Unavailable mismatch. Separately, `sanitize_usage_record_metadata` intentionally clears deprecated `error_message` for both storage paths, regardless of record level. This is not missing retry metadata. | Explicit Full; assert deprecated `error_message` remains absent and structured `error_category = server_error`. Seed the existing dedicated `local_execution_runtime_miss_reason` field and verify that field round-trips instead of expecting the deprecated display field. Keep immediate terminal existence, failed/void/503, endpoint/candidate attribution, body absence and Unavailable assertions. |
| 7–8 | Capability classification reads the internal runtime-miss header before response creation; credential retry, quarantine and failure candidate recording already pass in CI. `build_client_response_from_parts_with_mutator` uses the existing `should_skip_response_header` rule (`x-aether-*` excluded), so the client-facing fallback correctly does not expose the internal header. | Require the header to remain absent and add full JSON equality for the retained original upstream error body. Keep 503, credential retry, quarantine, failed-candidate classification, and the stream test's unchanged Key health equality. No response-header policy exception or production metadata change. |
| 9 | `acquire_provider_pool_execution_guard` strongly reads the planned Key and requires its Provider ID to match, including when the sync transport is a test override. This fixture supplied billing and settlement only, so execution failed before the synthetic next-candidate error. | Seed only the required active `key-1`/`provider-1` record using the existing memory catalog, matching the existing sync test fixture pattern. Keep exact synthetic Internal error equality and the settlement probe requiring AlreadyTerminal/Released for the original reservation token. No admission bypass and no transport credential needed. |
| 5–6 | `authenticated_local_tunnel_test_state` created node authentication data, but each caller immediately replaced the whole GatewayDataState with catalog-only data. `with_data_state_for_tests` replaces, not merges; node authentication vanished before headers could be sent. | Existing helper now takes the plan and attaches its bound-credential catalog to the node data before the single AppState installation and proxy registration. Both callers use it directly. Retain authenticated proxy generation/key, live Key admission, 5-second deadlines, first-visible-data rule, original error non-disclosure and same-stream terminal SSE assertions. No timeout increase. |
| 14 | `GeminiProviderState::push_line` captures top-level `responseId` before emitting Start. The test still derived its message ID from the old fallback `resp-local-stream`. Shared `openai_responses_message_item_id` is UUIDv5 over `response_id:output_index`. | Use the actual `resp_antigravity_cli_xfmt_123` input. Keep equality for the entire response object, output content, annotations, usage, created/completed time and routing/refresh checks. No ID normalization or conversion production change. |

## Files changed

1. `apps/aether-gateway/src/execution_runtime/stream/execution.rs`
2. `apps/aether-gateway/src/execution_runtime/sync/execution.rs`
3. `apps/aether-gateway/src/executor/candidate_loop.rs`
4. `apps/aether-gateway/src/executor/orchestration.rs`
5. `apps/aether-gateway/src/executor/outcome.rs`
6. `apps/aether-gateway/src/tests/usage/local.rs`
7. `apps/aether-gateway/src/tests/ai_execute/finalize_local_cli/cross_format.rs`
8. This report.

All newly changed test functions/helpers have substantive Chinese explanations. No new framework, dependency, production helper, configuration setting, compatibility path, security control, or test suppression was introduced.

## Checks actually run

- Scoped `rustfmt --edition 2021 --config skip_children=true <seven assigned files>` followed by the same command with `--check`: PASS. No global formatter and no child-module writes.
- Scoped `git diff --check -- <seven assigned files>`: PASS. Git only warns that its configured checkout conversion will replace LF with CRLF; there is no whitespace error.
- Dependency-free Python UUIDv5 check: PASS. The actual response identity produces `msg_aether_b49aaa692b89530e994de3ec279b0686`; the former fallback produces `msg_aether_55aeb11a4d3b5cc181e19fdd1fb41776`, exactly both immutable CI values. This is algorithm/input evidence, not a compiled Gateway test pass.
- Read-only source review traced capture policy through gateway config, usage event capture and persistence; response header filtering through both sync/stream fallbacks; strong Key admission; the single data-state replacement; dynamic retry/cost release; Gemini identity generation. Existing Full sync/stream admin-detail regressions are unchanged.
- No Gateway compilation, Cargo/Clippy, full-suite test, or CI rerun performed: explicit concurrent-writer boundary requires the unique integrator to validate after all writers stop. These 15 tests remain unexecuted locally after the edits.

## Exact 15 test filters for the integrator

Use the existing pinned-toolchain/shared gateway environment, 16 MiB test stack, and `--no-fail-fast --locked`. Select these exact test names with nextest `test(=...)` joined by `|`; require 15 nonzero selected tests (or the known lib/bin target multiplicity), not a zero-test success.

```text
execution_runtime::stream::execution::tests::execute_stream_from_frame_stream_decodes_non_success_windsurf_connect_error_body
execution_runtime::stream::execution::tests::execute_execution_runtime_stream_emits_terminal_sse_error_event_after_body_started
execution_runtime::stream::execution::tests::execute_execution_runtime_stream_sanitizes_local_tunnel_error_before_first_data
execution_runtime::stream::execution::tests::no_local_stream_plans_retries_at_credential_scope_without_key_penalty
execution_runtime::sync::execution::tests::no_local_sync_plans_retries_at_credential_scope_without_key_penalty
executor::candidate_loop::tests::dynamic_sync_planning_error_releases_reserved_http_plan_cost
executor::orchestration::tests::openai_image_sync_heartbeat_records_deferred_usage_for_shell_response_once
executor::orchestration::tests::standard_text_sync_heartbeat_records_deferred_usage_for_shell_response
executor::outcome::tests::deferred_upstream_response_records_failed_void_usage_immediately
tests::ai_execute::finalize_local_cli::cross_format::gateway_executes_openai_responses_antigravity_cross_format_upstream_stream_via_local_finalize_response
tests::usage::local::gateway_records_failed_usage_for_claude_runtime_miss_without_execution_exhaustion
tests::usage::local::gateway_keeps_failed_usage_request_capture_lightweight_for_large_local_claude_cli_runtime_miss
tests::usage::local::gateway_records_failed_usage_when_all_local_claude_cli_candidates_are_skipped
tests::usage::local::gateway_records_failed_usage_when_all_local_openai_chat_candidates_exhaust_after_retryable_sync_failure
tests::usage::local::gateway_records_failed_usage_when_sync_runtime_transport_is_unavailable_without_plan_fallback
```

Adjacent existing capture regressions (unchanged, useful if the integrator broadens the bounded selection):

```text
tests::usage::local::gateway_strips_request_and_response_bodies_when_request_record_level_is_base
tests::usage::local::gateway_full_request_record_level_preserves_sync_bodies_in_admin_detail
tests::usage::local::gateway_full_request_record_level_preserves_stream_bodies_in_admin_detail
```

Dependencies: normal `aether-gateway` lib/bin test build and its workspace crates; these tests use memory repositories and test-local loopback servers/tunnel queues, not a real database, provider account, Docker service or external credentials. The next-Candidate test uses the existing settlement/billing memory contract and the actual strong Key guard; the ID test uses `aether-ai-formats`' existing shared helper.

## Remaining risks / cleanup

- Runtime execution is pending, especially the two real tunnel-queue paths and post-admission cost-release path. Static proof and formatting do not establish their runtime success.
- No claim about final CI SHA success; Root must collect remaining current/new CI results and verify the eventual pushed exact SHA.
- All prior eight dirty repairs and other groups' ownership are preserved. No git staging/commit/push, external mutation, temporary source file, persistent process, test server or background task was created by B; no cleanup remains.
