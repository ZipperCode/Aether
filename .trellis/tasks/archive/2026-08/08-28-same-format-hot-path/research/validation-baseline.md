# Research: Same-format hot-path validation baseline

- Query: Identify existing benchmarks, pressure tools, metrics, fixtures, and the smallest reproducible validation for same-format request and streaming performance work.
- Scope: internal
- Date: 2026-08-28

## Findings

### Conclusion

The repository has no Criterion, `cargo bench`, `#[bench]`, `[[bench]]`, or other dedicated Rust microbenchmark framework in the searched workspace manifests and source. Do not add one for this task. Reuse the existing layers:

1. Focused Rust tests prove byte/field preservation and required rewrite behavior; they are correctness checks, not performance measurements.
2. `aether-loadtools` measures release-build HTTP throughput plus headers, first-body, and full-body latency against a running gateway.
3. `gateway_pressure_probe` adds gateway operational sampling and a versioned report, but requires a running, fully instrumented gateway.
4. Gateway stage histograms can attribute millisecond-scale changes. Their 1 ms minimum bucket and integer-millisecond observations cannot substantiate a sub-millisecond optimization by themselves.

No new dependency, benchmark framework, configuration surface, or permanent fixture is needed.

### Files found

- `apps/aether-gateway/src/stage_metrics.rs` — built-in request/stream stage counters, sums, maxima, and histogram buckets.
- `crates/aether-testing/loadtools/src/load.rs` — shared HTTP load engine and serialized latency/throughput result contract.
- `crates/aether-testing/loadtools/src/runtime.rs` — load-generator process CPU/memory sampler.
- `crates/aether-testing/loadtools/src/bin/http_load_probe.rs` — smallest standalone HTTP load CLI; no gateway metrics dependency.
- `crates/aether-testing/loadtools/src/bin/gateway_pressure_probe.rs` — load plus Prometheus sampling, settle/drain checks, and JSON report output.
- `tools/pressure/check_gateway_stage_report.js` — versioned report acceptance checker with overridable load thresholds.
- `tools/pressure/check_gateway_stage_report.test.js` — dependency-free `node:test` coverage for the report checker.
- `tools/pressure/run_gateway_realistic_profile.sh` — existing realistic-stream/TPS orchestration and before/after Prometheus snapshots; Bash-only.
- `tools/pressure/run_gateway_mock_streaming_stage.sh` — existing S1-S5 mock streaming profiles; intentionally large and long-running.
- `crates/aether-testing/integration/src/bin/mock_openai_upstream.rs` — deterministic local upstream supporting both `/v1/chat/completions` and `/v1/responses`.
- `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/payload.rs` — exact original-request-byte reuse and its edited-body controls.
- `crates/aether-provider/transport/src/same_format_provider/mod.rs` — native Responses request construction and unknown-field preservation.
- `crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs` — no-rewriter, same-family compatibility, and cross-format stream rewrite decisions.
- `crates/aether-ai/formats/src/formats/shared/sync_products.rs` — same-family Responses sync finalization and error pass-through.
- `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs` — same-format Responses SSE precommit policy.
- `apps/aether-gateway/src/execution_runtime/stream/execution.rs` — first-body prefetch/error handling and end-to-end regressions.

### Existing code patterns and contracts

- Raw request bytes are reusable only for `NativeTransparent`, an unchanged parsed JSON value, no redaction, no compatibility edits, no content encoding, and no enabled request gzip (`payload.rs:352-374`). `unchanged_same_format_body_preserves_original_json_bytes` asserts exact byte equality, while `request_edits_or_encoding_disable_original_json_bytes` covers every disqualifier (`payload.rs:628-724`). This pair is the smallest request hot-path guard.
- Native OpenAI Responses construction copies the object rather than applying a field allowlist. `same_format_responses_body_preserves_opaque_extension_fields` covers current Codex fields and an unknown future field (`same_format_provider/mod.rs:1943-1994`).
- Exact same-format streams with no required envelope/model/compat operation fall through without a rewriter; a native private-envelope example explicitly asserts `None` (`stream_rewrite.rs:1002-1012`). This is the raw-stream class.
- OpenAI Responses-family streams intentionally keep a compatibility rewriter. Non-terminal opaque data stays present, while the terminal event may receive required defaults (`stream_rewrite.rs:1248-1306`). This is the rewrite control; optimizing the raw class must not bypass it.
- Same-family sync success preserves the complete provider JSON; 400/500 bodies return `None` so the generic HTTP boundary retains status/body (`sync_products.rs:5482-5573`).
- Native same-format Responses SSE is not allowed to commit on headers: it waits for the first classified body (`commit_policy.rs:19-80`, regression at `commit_policy.rs:393-418`). A first bare error must still enter the existing candidate-retry/error path before downstream HTTP 200 (`execution.rs:11528-11537`). Any latency improvement must retain this precommit boundary.
- Load results include `throughput_rps`, `p50/p95/p99/max/mean`, headers percentiles, first-body percentiles, completed/failed requests, status/error counts, and a runtime snapshot (`load.rs:159-216`). The runtime snapshot describes the load-generator process, not the gateway (`runtime.rs:7-22`). On Windows its file-descriptor fields are zero because FD sampling is Unix-only (`runtime.rs:117-141`).
- Stage metrics use buckets `1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000` ms (`stage_metrics.rs:8`). Each observation records integer milliseconds and performs one exclusive bucket increment; Prometheus cumulative buckets are rebuilt on export (`stage_metrics.rs:194-246`). Metrics default on and can be disabled with `AETHER_GATEWAY_STAGE_METRICS_ENABLED` (`stage_metrics.rs:139-160`).
- Relevant stage labels include `frontdoor_stream_fast_path_total`, `frontdoor_to_stream_response_ready`, `frontdoor_to_stream_first_client_yield`, `stream_path_step_same_format_provider`, `stream_candidate_execute`, `stream_response_ready`, `stream_first_client_yield`, `direct_request_prepare`, `direct_build_body`, and direct-passthrough first-body/send stages (`stage_metrics.rs:10-136`).
- `gateway_pressure_probe` requires target URL, metrics URL, request count, and concurrency; it samples before/during/after load, emits JSON, and optionally writes `--output` (`gateway_pressure_probe.rs:2485-2562`, `2586-2769`). Its report summary does not include `gateway_stage_latency_*`; the existing realistic-profile script captures raw Prometheus before/after snapshots separately (`run_gateway_realistic_profile.sh:190-227`).
- The report checker hard-fails incomplete requests, any load error, non-200 status under acceptance contract v2, and mismatched response mode (`check_gateway_stage_report.js:701-760`). It can additionally gate throughput and headers/first-body/full-body p95/p99 (`check_gateway_stage_report.js:762-767`).

### Fixtures / scenarios

Use the same gateway build, provider row, model mapping, auth source, mock process, protocol, request count, concurrency, and machine power state for baseline and candidate runs.

#### A. Unchanged request bytes (micro correctness; no services)

Existing fixture:

```json
{ "unknown": {"enabled":true}, "messages": [], "model": "claude-sonnet-4" }
```

Expected: the byte sequence, including spacing/order, is reused exactly. The paired edited-body test is the negative control.

#### B. Raw same-format stream (integration measurement)

Use downstream and provider format `openai:chat`, no private envelope, no display-model rewrite, and no format conversion:

```json
{"model":"gpt-5-mini","messages":[{"role":"user","content":"ping"}],"stream":true,"opaque_future_field":{"enabled":true}}
```

The mock upstream exposes `/v1/chat/completions`. In the current resolver, this exact/no-directive case falls through without a local stream rewriter. There is no dedicated OpenAI-chat no-rewriter unit test; treat provider/report context verification as part of integration setup.

#### C. Required same-format rewrite control (integration measurement)

Use downstream and provider format `openai:responses`:

```json
{"model":"gpt-5-mini","input":"ping","store":false,"stream":true,"opaque_future_field":{"enabled":true}}
```

The mock upstream exposes `/v1/responses` and emits Responses SSE. This exercises first-body classification plus Responses compatibility finalization. It must retain unknown events/fields and the terminal compatibility behavior while remaining error-free.

### Exact focused commands (no running services)

Run from the repository root with the pinned Rust 1.95.0 toolchain:

```powershell
cargo test -p aether-gateway --lib original_json_bytes
cargo test -p aether-provider-transport --lib same_format_responses_body_preserves_opaque_extension_fields
cargo test -p aether-ai-formats --lib same_family_responses_passthrough_preserves_encrypted_content
cargo test -p aether-ai-formats --lib same_family_responses_without_display_model_runs_terminal_compat_only
cargo test -p aether-ai-formats --lib falls_back_to_body_json_for_openai_responses_same_family_sync_payload
cargo test -p aether-ai-formats --lib rejects_openai_responses_same_family_error_body_json
cargo test -p aether-gateway --lib policy_prefetches_same_format_openai_responses_sse_only
cargo test -p aether-gateway --lib same_format_responses_prefetch_retries_bare_error_before_committing_success
cargo fmt --all --check
```

If the report checker is changed, its smallest check is:

```powershell
node --test .\tools\pressure\check_gateway_stage_report.test.js
```

These commands prove behavior only. Do not time `cargo test` or `cargo run` compilation and call it a hot-path benchmark.

### Bounded service-backed measurement

The existing deterministic upstream command is:

```powershell
cargo run --release -p aether-integration-tests --bin mock_openai_upstream -- --bind 127.0.0.1:18181 --chunks 8 --first-byte-delay-ms 0 --chunk-delay-ms 0 --payload-bytes 64
```

It is a long-lived service and must be started with tracked PID/process-tree cleanup by the main session. The gateway must already route the selected model to this same-format upstream. Keep `AETHER_GATEWAY_STAGE_METRICS_ENABLED=true`; keep `AETHER_GATEWAY_STAGE_TRACE_MODE` identical across revisions (prefer `off` when isolating steady-state cost).

For a smallest release-build load check against an already running gateway, run the same command three times per fixture and parse the emitted JSON. This CLI has no key-file option, so the shown environment expansion must never be logged or copied with a literal secret:

```powershell
$authHeader = "Authorization: Bearer $env:AETHER_API_KEY"
$chatBody = '{"model":"gpt-5-mini","messages":[{"role":"user","content":"ping"}],"stream":true,"opaque_future_field":{"enabled":true}}'
cargo run --quiet --release -p aether-loadtools --bin http_load_probe -- --url http://127.0.0.1:8084/v1/chat/completions --requests 2000 --concurrency 64 --warmup-connections 64 --client-shards 4 --pool-max-idle-per-host 64 --start-ramp-ms 1000 --method POST --timeout-ms 30000 --connect-timeout-ms 5000 --http1-only --header $authHeader --header 'Content-Type: application/json' --body $chatBody --response-mode full --require-sse-done

$responsesBody = '{"model":"gpt-5-mini","input":"ping","store":false,"stream":true,"opaque_future_field":{"enabled":true}}'
cargo run --quiet --release -p aether-loadtools --bin http_load_probe -- --url http://127.0.0.1:8084/v1/responses --requests 2000 --concurrency 64 --warmup-connections 64 --client-shards 4 --pool-max-idle-per-host 64 --start-ramp-ms 1000 --method POST --timeout-ms 30000 --connect-timeout-ms 5000 --http1-only --header $authHeader --header 'Content-Type: application/json' --body $responsesBody --response-mode full --require-sse-done
```

For an acceptance report with secret-file handling and gateway operational metrics, use the existing `gateway_pressure_probe` instead. It requires a metrics-complete gateway; the default in-memory dev runtime may not satisfy its preflight:

```powershell
$reportPath = Join-Path $env:TEMP 'aether-same-format-responses.json'
cargo run --quiet --release -p aether-loadtools --bin gateway_pressure_probe -- --url http://127.0.0.1:8084/v1/responses --metrics-url http://127.0.0.1:8084/_gateway/metrics --requests 2000 --concurrency 64 --warmup-connections 64 --client-shards 4 --pool-max-idle-per-host 64 --start-ramp-ms 1000 --method POST --timeout-ms 150000 --connect-timeout-ms 5000 --sample-interval-ms 500 --settle-after-ms 10000 --api-key-file $env:AETHER_API_KEY_FILE --header 'Content-Type: application/json' --body $responsesBody --response-mode full --require-sse-done --output $reportPath
node .\tools\pressure\check_gateway_stage_report.js --stage realistic-stream --min-requests 2000 --min-concurrency 64 $reportPath
```

The repository's S1-S5 scripts start at 1,000 concurrent two-minute streams and are not the minimal validation for this change. Run them only if the focused A/B exposes shared-capacity risk or release/CI policy requires them.

### Non-flaky acceptance thresholds

Correctness gates are absolute for every measured run:

- `completed_requests == total_requests`
- `failed_requests == 0`
- HTTP 200 count equals total requests; `error_counts` is empty
- full-stream runs use `require_sse_done=true`
- all focused preservation/rewrite/error tests pass

Performance claims use paired release runs, not a single absolute number:

1. Discard one warmup run, then collect at least three baseline and three candidate reports for both raw-stream and rewrite-control fixtures.
2. Compare medians under unchanged service/configuration. Candidate throughput must not fall below 95% of baseline median.
3. Candidate headers, first-body, and full-body p95 must not exceed baseline median by more than `max(2 ms, 10%)`.
4. Claim an improvement only when one primary metric improves by at least 10% (and at least 2 ms for a latency metric), at least two of three candidate runs move in that direction, and no correctness/regression gate fails.
5. Use stage deltas only for attribution: subtract before/after counter snapshots, require at least 1,000 observations, and do not claim sub-millisecond improvement from these integer-ms buckets. For a sub-millisecond hot path, aggregate throughput under the same load is the usable existing signal.

This deliberately avoids brittle machine-independent throughput targets. The built-in `realistic-stream`/TPS values are capacity-profile gates, not universal proof that a small local hot-path edit is faster.

### Minimal required validation for this task

- Always: focused request-byte, native Responses field, stream passthrough/compat, sync error, and precommit tests listed above; `cargo fmt --all --check`.
- To claim measurable performance improvement: three paired release reports for raw stream and rewrite control, all absolute correctness gates green, and the median thresholds above.
- Only if the change touches stage metric/report code: `stage_metrics` focused tests and `node --test tools/pressure/check_gateway_stage_report.test.js`.
- Only if focused load reveals broader resource pressure: the existing `gateway_pressure_probe` plus report checker, then S1 or larger profiles as justified. Do not begin with S1-S5.

## External references

- No external web source was needed; all findings come from current workspace code and project specs.
- Rust is pinned to `1.95.0` (`rust-toolchain.toml:2`).
- The report-checker test uses Node's built-in `node:test`/`node:assert`, so it adds no package dependency (`check_gateway_stage_report.test.js:1-9`).

## Related specs

- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md` — requires unknown-field/event preservation, first-event classification, same-format error handling, and the focused regressions used above.
- `.trellis/spec/aether-gateway/backend/quality-guidelines.md` — shared gateway response-boundary and focused-test expectations.
- `.trellis/workflow.md` — research persistence and focused verification workflow.

## Caveats / Not Found

- No dedicated microbenchmark exists, and none is recommended for this task.
- The current task `prd.md` still contains TBD placeholders, and `task.json.package` says `aether-tunnel` even though the confirmed scope is gateway/formats/loadtools. This research follows the parent dispatch's confirmed scope; planning should correct task metadata before implementation context is finalized.
- The existing orchestration scripts are Bash and use `/tmp`; the direct Cargo/PowerShell commands above are the Windows-compatible path. They were inspected but not executed in this read-only research turn.
- No service, load test, Cargo test, or long pressure profile was run. Commands and thresholds are a reproducible baseline plan, not current performance results.
- Raw OpenAI-chat no-rewriter behavior is inferred from the current resolver branches and should be confirmed through integration report context; there is no exact dedicated unit test for that case.
- `gateway_pressure_probe` summarizes operational metrics but not `gateway_stage_latency_*`; stage attribution needs raw Prometheus snapshots or an existing metrics backend query.
- Host scheduling, antivirus, power state, release-build reuse, database/Redis mode, provider mapping, HTTP version, and mock delay can dominate small differences. Record them with each A/B report and do not compare across changed environments.
