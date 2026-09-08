# Second CI model-fetch failure receipt

- Status: READY for Root's coordinated targeted Cargo validation; no local test PASS claimed.
- Base SHA: `9026380d1957e7a57a746a409335f15eb1675515`.
- CI evidence: run `34217818968`, Workspace Rest; exact failure recorded by the read-only collector in `research/run-34217818968.md`.
- Failure: `strategy::tests::vertex_pagination_failure_does_not_claim_endpoint_authority`, original `strategy.rs:2895:9`, expected the error to contain `page unavailable`.

## Root cause and confirmed contract

The old regression originated in `60377958cb` (2026-08-12). The existing API-boundary diagnostic projection originated in `579f2c7cc1` (2026-09-04). `sanitize_model_fetch_error_detail` admits fixed phrases only; `page unavailable` is not one. The fixture's HTTP 503 therefore becomes `HTTP 503: upstream service failed`, and Vertex adds its fixed source label. This is a stale raw-message assertion, not a lost status/classification or Endpoint-authority regression.

The current call path is `fetch_models_from_transports` -> `fetch_models_from_transports_for_client_version` -> `execute_model_fetch_strategy` -> `fetch_vertex_models` -> `fetch_vertex_api_key_models` -> `fetch_vertex_models_from_url`. `execution_result_error_message` derives the HTTP status and fixed category; the Vertex aggregate sanitizes the result and retains it in `errors`.

- A later failed page returns an error and no partial page models. The aggregate records the failed Endpoint and continues without granting success or replacement authority.
- `successful_endpoint_ids` excludes every failed Endpoint; the service-account sibling uses the same exclusion rule.
- The adjacent `vertex_partial_failure_on_one_endpoint_keeps_positive_evidence_without_authority` regression preserves models from a fully successful discovery source while withholding authority when another source for that Endpoint fails. It is unchanged.
- Existing credential/URL diagnostic projection regressions remain unchanged. No raw upstream text, fallback, guard, or allowlist entry was restored.

## Actual changes

- `crates/aether-model-fetch/src/strategy.rs`: add a substantive Chinese doc comment to the changed test and replace its raw-text substring check with exact equality to `vertex google models fetch failed: HTTP 503: upstream service failed`.
- All original success, cached-model, successful-Endpoint, error-count, and two-request assertions remain. No production function changed; no shared caller behavior changed.
- This receipt is the only other authored file.

## Verification and handoff

- `rustfmt --edition 2021 --check crates/aether-model-fetch/src/strategy.rs`: PASS.
- `git diff --check -- crates/aether-model-fetch/src/strategy.rs`: PASS.
- Reviewed the complete scoped diff: five inserted lines and one removed line in the product tree, all inside the existing test/comment.
- No Cargo command started: Root owns the main target directory slot and requested coordinated small-crate validation after the independent fixes are ready.

Exact original failure filter:

```text
test(=strategy::tests::vertex_pagination_failure_does_not_claim_endpoint_authority)
```

Small relevant validation command for the integrator:

```text
cargo nextest run -p aether-model-fetch --lib --locked -E 'test(=strategy::tests::vertex_pagination_failure_does_not_claim_endpoint_authority) | test(=strategy::tests::vertex_partial_failure_on_one_endpoint_keeps_positive_evidence_without_authority) | test(=strategy::tests::execution_result_error_projection_discards_execution_error_message)'
```

No temporary files, persistent process, real provider/API access, database access, credentials, dependency/environment mutation, staging, commit, push, workflow rerun/cancellation, release, or deployment. Root retains task/spec/Git ownership and exact-SHA Linux CI verification.
