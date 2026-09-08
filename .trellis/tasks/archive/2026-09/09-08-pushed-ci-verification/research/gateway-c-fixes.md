# Gateway Group C repair receipt

- Status: READY for integrator validation, 2026-09-08. Product writes stopped.
- Baseline: run `34210248162`, head `948c1c16f2f9927b37a8570b86767128abd6b7c8`; 11 observed Gateway failures (#13, #17-19, #24-30).
- Scope: seven assigned Gateway fixture files plus the explicitly granted shared projection file `crates/aether-admin/src/provider/redaction.rs`. Assigned `handlers/shared/catalog.rs` was inspected but deliberately left unchanged, including its original balance regression. All other owners' changes preserved.

## Root causes and repairs

| CI rows | Evidence and classification | Repair and preserved contract |
| --- | --- | --- |
| #13 | **Production projection omission.** `catalog.rs::quota_snapshot_has_materialized_data` already recognizes `balances`; `provider_key_status_snapshot_payload` retains that snapshot, then passes it through `admin_provider_status_snapshot_safe_json`. `redaction.rs::project_quota_status_snapshot` omitted `schema_version`, `kind`, and `balances`. The final provider-key response projects again, so bypassing only the catalog call would remain wrong. | Extend the existing shared projection, not its callers. Preserve schema version, kind, balance unit, and the five existing amount fields (`available`, `total`, `granted`, `topped_up`, `used`). Amount strings are only checked for finite-number syntax and cloned unchanged: no monetary calculation, currency conversion, precision normalization, or rounding. Preserve null. New inline regression checks a decimal above 2^53 with trailing zeroes, negative/zero/string values, null, invalid numeric diagnostics, unknown-field omission, non-object omission, and repeated-projection idempotence. Original Gateway `13.82` assertion remains untouched. |
| #17 | **Contradictory fixture expectations.** The same test expected one global model and no Provider Models near its midpoint, then already expected three discovered Provider Models and exact Endpoint bindings at the end. `antigravity.rs::sync_antigravity_discovered_models` imports all routable IDs with the refresh Endpoint; `antigravity_model_id_is_routable` excludes `chat_23310`. | Require exactly the three discovered global names and three Provider Models. Reuse the prior global record, comparing every field after accounting only for `snapshot.rs::enrich_admin_global_model` deriving `provider_count=1` and `active_provider_count=1`. Preserve all existing quota persistence, auth/identity headers, internal-model exclusion, exact Endpoint ID, discovered source, and active-binding assertions. |
| #18, #19 | **Legacy credential fixture in a read-only repository.** Both seed `sample_key`, which encrypts a legacy Fernet envelope, then mount `with_provider_catalog_reader_for_tests`. Inference calls `list_provider_catalog_keys_by_provider_ids` before consuming the cached models. That calls `open_provider_catalog_keys`, whose credential open requires a writer for migration; `catalog_credentials.rs` returns `stored provider catalog credentials require migration but the catalog writer is unavailable`. | Reuse existing `sample_bound_key` in exactly these two fixtures. It seals for the actual Provider/Key IDs. No writer, migration branch, auth relaxation, Endpoint inference change, or cache bypass added. Preserve global regex inference and automatic binding replacement on rename; add response JSON to status-failure diagnostics. HTTP 500 cause is source-traced, not locally rerun in this worker. |
| #24 | **Stale Claude-only expectation, not generic pool loss.** Fixture Provider is `custom`, and its stored config contains `pool_advanced: {}` plus `failover_rules`, not `claude_code_advanced.pool_size=2`. `write/provider/update.rs` removes `config.claude_code_advanced` when `target_provider_type != "claude_code"`; `summary/value.rs` explicitly emits that response field as null. | Assert explicit `claude_code_advanced: null`. Keep all generic `pool_advanced`, failover rules, Ops architecture, transfer limit, timeout, billing, and manually updated configuration assertions, including later persistence reads. No provider configuration implementation changed. |
| #25, #26 | **Null-tombstone read contract.** `clear_proxy_node_references_before_delete_with_cache` persists null selectors. `data/state/core.rs::find_system_config_value` and its strong ordinary-value counterpart filter tombstones to None; `find_system_config_value_with_revision_strong` retains raw null and revision. | Expect None for ordinary reads and independently assert strong-read null tombstones with positive revisions. Keep cleared counters, Provider/Endpoint/Key proxy removal, deleted-node absence, cache-failure continuation, and cache-failure node-presence checks. |
| #27 | **Unset secret is a tombstone, not an exported live setting.** Fixture explicitly seeds `turnstile_secret_key=null`. `list_system_config_entries` filters all null tombstones before export processes `RecoveryBackup`/interactive modes. | Recovery export must omit the unset Turnstile entry; independently verify the stored raw null tombstone. Preserve recovery plaintext assertions for configured test credentials, rollback checkpoint semantics, and interactive export's existing secret omission/masking and disabled-credential behavior. No real credentials inspected. |
| #28 | **Deleted proxy selector omitted by export.** Import's skipped legacy node selects direct mode using a null selector; export lists live entries and omits its tombstone. | Assert selector absent from exported config and independently assert raw null with a positive revision through the retained state. Keep import statistics, all actual persisted Provider/Endpoint/Key/model/LDAP/OAuth assertions, manual configuration, and no-secret export checks. |
| #29 | **Same ordinary-read tombstone contract as #25/#26/#28.** Import rejects unsupported legacy proxy nodes and persists a direct selector. | Expect ordinary None and raw null/positive revision. Keep skipped-node/imported-config counters and both explanatory import errors. |
| #30 | **Missing authoritative Provider directory fixture.** The allowlist contains Provider ID `provider-allowed`, but the fixture only mounts auth snapshots and model/global catalogs. `auth_snapshot_allows_requested_provider` explicitly rejects unresolved Provider IDs without a Provider catalog; the existing `data_backed_auth_context_denies_unresolved_provider_id_without_catalog_reader` test covers that fail-closed behavior. | Mount the existing read-only Provider reader with matching custom Provider and OpenAI Endpoint. Keep both user/key Provider-ID restrictions, visible static association, and exclusion of the unassociated active global model. No auth implementation, unrestricted alias, writer, or key fixture required. Add response JSON diagnostics; 401 cause is source-traced, not locally rerun here. |

## Projection boundary evidence

- Actual balance contract: `crates/aether-provider/pool/src/quota_snapshot.rs::ProviderQuotaBalance`: unit string; five optional decimal strings. Frontend `frontend/src/api/endpoints/types/statusSnapshot.ts::QuotaBalanceSnapshot` consumes those fields; no frontend edits or tests needed.
- Shared projection call paths reviewed: Gateway catalog construction/synchronization and final key response (`handlers/shared/catalog.rs`), admin pool payload (`handlers/admin/provider/pool_admin/payloads.rs`), and `crates/aether-admin/src/provider/pool.rs`.
- Unknown diagnostic objects and secret-looking nonnumeric amount strings remain omitted. Existing diagnostic redaction test remains unchanged. The extension does not expose arbitrary snapshot keys or add a parallel passthrough path.

## Changed files

- `apps/aether-gateway/src/tests/control/admin/endpoints/quota.rs`
- `apps/aether-gateway/src/tests/control/admin/models/provider.rs`
- `apps/aether-gateway/src/tests/control/admin/providers.rs`
- `apps/aether-gateway/src/tests/control/admin/proxy_nodes.rs`
- `apps/aether-gateway/src/tests/control/admin/system.rs`
- `apps/aether-gateway/src/tests/control/admin/system_import.rs`
- `apps/aether-gateway/src/tests/frontdoor/ai.rs`
- `crates/aether-admin/src/provider/redaction.rs` (explicit Root lock grant)
- This receipt.

## Actual validation

- PASS: `rustfmt --edition 2021 --check --config skip_children=true` against all eight changed Rust files. First check found only import wrapping in `models/provider.rs`; corrected with apply_patch and reran successfully.
- PASS: scoped `git diff --check` on all eight Rust files; LF-to-CRLF Git notices are informational, not whitespace failures.
- PASS: repeated scoped rustfmt/diff check for the final quota fixture correction after reading dynamically enriched association counts.
- Read-only review traced the remaining assertions after each original first-failure point, including manual Provider configuration, import/export, exact discovered bindings, and redaction idempotence.
- No Cargo, live local HTTP, Gateway compile, workspace tests, database, remote CI, Git commit/stage/push, workflow operations, real provider calls, or production changes by this worker. Parallel Gateway writers require the integrator to build once after all write sets stop. Syntax/format evidence is not a runtime PASS.
- No new temporary files or persistent processes; no cleanup needed beyond this retained task report.

## Exact pending Gateway test filters

Use these fully qualified test names in the integrator's one no-fail-fast Gateway selection; original #13 is still unchanged and exercises the production fix.

```text
handlers::shared::catalog::tests::provider_key_status_snapshot_payload_preserves_materialized_balance_snapshot
tests::control::admin::endpoints::quota::gateway_refreshes_admin_provider_quota_locally_for_antigravity_with_trusted_admin_principal
tests::control::admin::models::provider::gateway_rebuilds_automatic_endpoint_bindings_when_provider_model_name_changes
tests::control::admin::models::provider::gateway_uses_global_model_regex_to_infer_created_model_endpoint_binding
tests::control::admin::providers::gateway_updates_admin_provider_locally_with_trusted_admin_principal
tests::control::admin::proxy_nodes::gateway_continues_proxy_reference_cleanup_when_external_models_cache_delete_fails
tests::control::admin::proxy_nodes::gateway_deletes_proxy_nodes_and_clears_proxy_refs_locally
tests::control::admin::system::gateway_handles_admin_system_config_export_locally_with_trusted_admin_principal
tests::control::admin::system_import::gateway_imports_admin_system_config_locally_and_persists_data
tests::control::admin::system_import::gateway_skips_proxy_nodes_during_admin_system_config_import
tests::frontdoor::ai::gateway_models_list_keeps_provider_restricted_keys_within_static_associations
```

Also run the two `aether-admin` tests selected by `test(provider_status_projection)`:

```text
provider::redaction::tests::provider_status_projection_rebuilds_diagnostic_text_from_codes
provider::redaction::tests::provider_status_projection_preserves_exact_balance_amounts
```

No remaining lock requests or known external blockers. Root owns type/runtime validation, whole-batch check, commit/push, and final exact-SHA CI result.
