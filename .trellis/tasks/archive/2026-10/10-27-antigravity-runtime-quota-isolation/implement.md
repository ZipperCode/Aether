# Implementation

1. Reuse existing failure, quota-refresh and strong candidate-read seams. No remote generation or production mutation. Add a real localhost regression for runtime upstream quota429 and cross-model serving first; integration owner records failing baseline before enabling runtime behavior changes.
2. Implement one shared RuntimeState-backed Antigravity model-quota observation policy and effect path, exact-key/model/credential identity, upstream reset/configured-backoff deadline, and positive fresh exact-model refresh precedence. Integrate all actual sync/stream failure paths and ordinary/Pool/sticky admission. Preserve account-level errors/manual bans and other Provider behavior.
3. Add behavior-focused regressions for classifier boundaries, reset/refresh order, stale credential/refresh behavior, and unaffected model/account restrictions. Review integration for no unintended account health/RPM/score penalties and no abandoned Pool leases.
4. Parent runs the incident HTTP smoke failing-before/passing-after, relevant Rust suites, final formatting, Clippy and Gateway full lib/bin suite. After successful smoke, update existing executable quota contracts. Commit only this task, archive and record session, then push per repository preference.

## Validation

Use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, Gateway `RUST_MIN_STACK=16777216`.

- Target real HTTP regression: `cargo test -p aether-gateway --lib gateway_antigravity_runtime_quota_isolation -- --nocapture`.
- Existing quota/discovery serving regressions and focused model-quota/classification/effects tests.
- `cargo nextest run -p aether-provider-pool --locked --no-fail-fast` when shared policy is affected.
- `python3 tools/ci.py preflight` for fmt, Gateway Clippy lib/bins/examples and Gateway nextest lib/bins.
- Related new-module all-targets Clippy when necessary.

Workers must not build/test/lint/format mid-flight; parent verifies after integration. Permanent tests assert consumer-visible isolation, boundary classification, error and restoration semantics, never source-text/wiring echoes. No schema, external services, real credentials, Google traffic or deployment.

## Observed results

- Before fix: the isolated HEAD baseline with the HTTP regression failed at
  `ordinary/step2-secondary-quota-429`; one exhausted model was generated four
  times, with `health_failures=4` and `rpm_429_count=4` (expected zero).
- After fix: `gateway_antigravity_runtime_quota_isolation` passed in ordinary
  and Pool/cache-affinity modes. Actual native sync/SSE HTTP served unaffected
  Claude/Gemini immediately, skipped repeated exhausted-model generation,
  failed over to the eligible account, restored on positive refresh and kept
  the restriction after failed refresh. Account penalties stayed zero.
- Targeted nextest filter passed: 24 tests, including runtime quota identity,
  reader diagnostics, account discovery and generic account rate-limit control.
- `python3 tools/ci.py preflight` passed: workspace format check, Gateway
  Clippy lib/bins/examples with `-D warnings`, and 5735 tests passed, 3 skipped.
- Review found no remaining blockers after credential/refresh-order and
  failed-refresh namespace-preservation fixes. No production deployment,
  Google generation or remote business writes were performed.
