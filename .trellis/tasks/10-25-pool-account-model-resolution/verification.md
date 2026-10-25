# Verification

## Exercised

- Before fix: real loopback HTTP mixed-account regression observed second account receiving bare `gpt-5` instead of its mapped `gpt-5-high`; explicit-empty-list scenario also failed. Initial fixture restrictions were then corrected to align row and catalog facts and enforce exact upstream account/model selection.
- Fixed mixed-account HTTP regression and explicit-empty-list regression:2 passed. The fixture performs real gateway and upstream HTTP, rejects wrong model IDs, and checks persisted first-account404 retries plus second-account200 success.
- Pool regression suite before final test-only replacement:257 passed. Earlier equivalent suite:256 passed.
- Candidate materialization suite:24 passed before the wiring-only test was removed in favor of a real cursor eligibility regression.
- In-memory candidate-selection suite:18 passed.
- Final full gateway lib suite:5602 passed,4 failed,3 ignored out of5609. All new pool/HTTP/model-policy/cursor tests passed in this final run.
- `cargo fmt --all --check`:passed.
- `cargo clippy -p aether-gateway --lib --locked -- -D warnings`:passed.

## Full-suite failures

Three failures require unavailable local temporary-PostgreSQL tools (`Os NotFound`):
- control::auth::resolution::tests::due_antigravity_bearer_refresh_observes_cross_node_allowlist_revocation
- control::auth::resolution::tests::strong_system_config_read_bypasses_app_and_data_caches
- tests::video::xai::xai_video_native_and_compatibility_http_lifecycle_postgres

One additional failure:
- tests::control::admin::oauth::gateway_starts_admin_provider_oauth_kiro_batch_import_task_locally_with_trusted_admin_principal, oauth.rs6564:expected1,got0. Investigated separately; not suppressed or re-pinned.
- Source inspection: the Kiro fixture seeds only provider-catalog data and invokes credential refresh/provisioning; no candidate model selection or pool hydration is called. Its JWT has no expiration claim. The task completed and processed one entry but reported success0; the existing assertion omits the per-item error, so the exact import failure is not established. No failure rerun or unrelated code change was made.
- Temporary PostgreSQL helpers spawn `initdb`, `postgres`, and `pg_ctl` (or AETHER_INITDB_BIN/AETHER_POSTGRES_BIN/AETHER_PG_CTL_BIN). No installed binaries were found; two auth tests require managed local PostgreSQL even with a test URL. These three failures are environmental.

No PostgreSQL binaries found in installed Homebrew/Postgres.app paths. No dependencies installed, real provider calls replayed, or remote deployments/configuration changes performed. Full-suite result remains failed, not reported as passed. After these results were reported, the user explicitly requested committing the changes; this authorizes the commit without treating the failed checks as passed.
