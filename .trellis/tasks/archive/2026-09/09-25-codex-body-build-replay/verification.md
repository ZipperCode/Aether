# Verification

Baseline: master `8e765877e69a17f98c928003b5740bd7d6b293b1`.
Production v0.7.39 was inspected read-only; no paid generation, service/config
change, deployment, push or new tag occurred during this repair.

## Formats and candidate diagnostics

Owner: `codex_image_fields_fix`, completed and released Cargo.

- RED: `cargo test -p aether-ai-formats --lib codex_image_output_format -- --nocapture`
  reproduced native success versus Codex `None` for the captured PNG fields.
- RED: `cargo test -p aether-data-contracts --lib candidate_body_build_diagnostic_survives_persistence_round_trip -- --nocapture`
  reproduced loss of exactly diagnostic kind/source.
- GREEN: `cargo test -p aether-ai-formats --lib formats::openai::image::request::tests` — 41 passed.
- GREEN: `cargo test -p aether-data-contracts --lib repository::candidates::types::tests` — 14 passed.
- PASS: `cargo clippy -p aether-ai-formats -p aether-data-contracts --all-targets -- -D warnings`.
- PASS: targeted `rustfmt --check` for the two owned files and `git diff --check`.
- PASS: `python docs/api/generate_format_field_coverage.py --check`.

## Gateway

- RED: corrected OAuth all-skipped router regression returned HTTP 200 instead
  of expected 503, reproducing the observed server behavior before the guard.
- Initial fixture corrections were kept separate from that proof: bearer was
  filtered before Codex candidate selection; before the later model repair, a
  genuinely empty candidate set used unknown model. Its absent-provider-body
  capture contract remains unchanged.
- Tests use the gateway's existing CI environment: incremental=0, dev/test
  debug=0 and RUST_MIN_STACK=16777216. cargo-nextest is not installed; use Cargo
  test/libtest filters without changing tooling.
- First green group: 19/20 passed; 503, failed/void usage, zero upstream and
  skipped/unstarted candidate assertions passed. Remaining new-test failure
  exposed model=unknown, traced to production ImageDecision policy=false and
  stripped candidate model metadata. Repair the shared writer using its existing
  original JSON parse. Preserve the existing rule that candidate diagnostics
  are not duplicated into usage metadata.
- GREEN: `cargo test -p aether-gateway --lib --locked -- tests::ai_execute::sync::image:: openai_image_sync_heartbeat gateway_records_failed_usage_when_all_local_claude_cli_candidates_are_skipped gateway_records_failed_usage_for_claude_runtime_miss_without_execution_exhaustion --nocapture`
  — 20 passed, including all-skipped/no-candidate generation and edits, capture
  on/off, explicit models, HTTP503, one failed/void record and zero upstream.
- GREEN: existing `executor::outcome::tests::` and `runtime_miss` filtered
  siblings on the same compiled libtest binary — 46 passed.
- PASS: `python tools/ci.py clippy-gateway` — runs `cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings`, exit0.
- Gateway groups overlap by one pre-existing test: 65 distinct gateway tests,
  66 successful executions. Together with41 formats and14 contracts tests,
  there are120 distinct directly related tests passing.

## Independent review

- Formats/data/shared callers, admin versus user projection, and final empty
  heartbeat guard reviewed with no findings by `codex_image_replay_check`.
- Final green/Clippy receipt received. Independent reviewer completed source
  review with no findings, then hit its usage limit while handing off the final
  format result. Parent reran `cargo fmt --all --check` and `git diff --check`:
  both passed. No unverified agent completion claim is used.

## Limits

- The 13:06 captured generation request provides the output_format root cause.
- The 13:09 edits prove missing usage and pre-execution skip; their original
  bodies were never persisted, so no exact-payload replay is claimed.
- Local mocks and regressions cannot certify a live Codex account's current
  entitlement or acceptance. The server still runs the previously deployed tag.
