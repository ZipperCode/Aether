# Gateway Group D: Kiro OAuth CI fixture repair

- Status: READY for the Root integrator's frozen-write-set runtime verification.
- Scope: `apps/aether-gateway/src/tests/control/admin/oauth.rs` and this receipt only.
- Baseline evidence: Rust CI `34210248162`, SHA `948c1c16f2f9927b37a8570b86767128abd6b7c8`, Gateway failure inventory rows 20–23.

## Root cause and actual call path

All four failures share missing Profile-discovery fixture coverage, not invalid real credentials or a production permission requirement.

1. Batch import (`handlers/admin/provider/oauth/dispatch/batch/kiro_import.rs`) and IDC device poll (`dispatch/device/poll.rs`) call `refresh_admin_provider_oauth_kiro_auth_config` (`dispatch/kiro.rs`). The poll path intentionally revalidates the device grant through IDC refresh before saving credentials or resolving email.
2. `crates/aether-oauth/src/provider/providers/kiro.rs::refresh_auth_config` refreshes and then calls `with_discovered_profile_arn`. When Profile is missing, `discover_kiro_profile_arn_in_region` sends `provider-oauth:kiro-profile-discovery` to `https://q.us-east-1.amazonaws.com/ListAvailableProfiles`. Transport errors propagate to the existing generic Token validation failure.
3. The old four fixtures registered only loopback token/refresh URLs (one also registered email). `oauth/http_executor.rs::GatewayOAuthHttpExecutor::execute` bypasses execution-runtime transport only for the registered test-loopback origins, not the fixed Profile URL. All four fixtures supplied a temporary proxy-node ID without a node repository. `oauth/proxy.rs` therefore produced `temporary_proxy_node_unavailable`; `execution_runtime/transport.rs::resolve_proxy_url` rejects an enabled proxy without a URL with `ProxyUnsupported`. This explains the shared error before the success assertions. This is source-traced against the immutable CI failure, not a new runtime reproduction.
4. Existing passing `gateway_batch_imports_admin_provider_oauth_kiro_via_execution_runtime_proxy_node` already models Profile discovery and an actual in-memory proxy node. The social callback/manual refresh fixtures already use the supported `profileArn` refresh-response field. IDC refresh does not consume `profileArn`, and the device-poll caller starts with `profile_arn: None`; adding that JSON field to its token response alone would not fix the devices.

## Changes

- Two social-import fixtures return a complete existing-contract refresh response with `profileArn`; assert that the exact profile is persisted. Keep inactive-duplicate reactivation, active/expired-duplicate replacement, refresh rotation, proxy persistence and invalid-state clearing assertions unchanged.
- Two device-poll fixtures use the existing `build_state_with_execution_runtime_override` helper and a local `/v1/execute/sync` fixture. A small shared response helper models only the observed IDC token, Profile discovery, email and post-save quota call paths. A shared in-memory node fixture supplies the session's actual proxy binding.
- Assert native IDC method/URL/client ID/client secret/grant/User-Agent, Profile method/URL/Bearer/native Kiro marker, and proxy node binding. The no-email case still obtains email only from the simulated upstream usage response and asserts exactly one email query, two distinct grant requests, use of the initial refresh token during revalidation, rotated credential persistence, and discovered profile persistence.
- Post-save quota calls are modeled explicitly because they share the execution transport; they do not count as token or email calls. No generic success fallback: unknown request IDs fail the fixture assertion.
- Production refresh, signature/identity policy, duplicate identity matching, credential sealing, refresh fencing, native clients, network policy and route permissions are unchanged. Existing fake JWT helpers remain unchanged.

## Validation actually run

- `rustfmt --edition 2021 --config skip_children=true apps/aether-gateway/src/tests/control/admin/oauth.rs`: completed; scoped to the exclusively owned file.
- `rustfmt --edition 2021 --config skip_children=true --check apps/aether-gateway/src/tests/control/admin/oauth.rs`: PASS after final edits; syntax/format evidence only.
- `git diff --check -- apps/aether-gateway/src/tests/control/admin/oauth.rs`: PASS after final edits; Git only warns about its normal LF-to-CRLF policy.
- Read-only call-path review verified the runtime override returns before real transport, and all new device requests go only to the local fixture server. No provider/account/DB/credential/network experiment was performed.

## Integrator checks still required

No gateway compilation or test execution was run while other groups owned changing gateway files, per dispatch. Do not label these tests runtime-PASS until the integrator executes them on the frozen combined source:

```text
tests::control::admin::oauth::gateway_batch_imports_admin_provider_oauth_kiro_locally_with_trusted_admin_principal
tests::control::admin::oauth::gateway_batch_imports_admin_provider_oauth_kiro_over_active_expired_duplicate
tests::control::admin::oauth::gateway_handles_admin_provider_oauth_device_poll_locally_with_trusted_admin_principal
tests::control::admin::oauth::gateway_revalidates_kiro_device_poll_via_idc_refresh_and_backfills_email
```

Suggested focused nextest expression for these four existing tests:

```text
cargo nextest run -p aether-gateway --lib --bins --locked --no-fail-fast -E 'test(=tests::control::admin::oauth::gateway_batch_imports_admin_provider_oauth_kiro_locally_with_trusted_admin_principal) | test(=tests::control::admin::oauth::gateway_batch_imports_admin_provider_oauth_kiro_over_active_expired_duplicate) | test(=tests::control::admin::oauth::gateway_handles_admin_provider_oauth_device_poll_locally_with_trusted_admin_principal) | test(=tests::control::admin::oauth::gateway_revalidates_kiro_device_poll_via_idc_refresh_and_backfills_email)'
```

Use the existing CI profile/16 MiB stack configuration, not a new setting. Adjacent unchanged discovery coverage: `tests::control::admin::oauth::gateway_batch_imports_admin_provider_oauth_kiro_via_execution_runtime_proxy_node`; lower-level `provider::providers::kiro::tests::refresh_discovers_missing_idc_profile_arn`.

## Ownership and cleanup

- No production file expansion or LOCK_REQUEST needed. No Git staging/commit/push, CI operation, task-state/spec edits, real accounts, settings or permissions changes.
- No persistent processes, new dependencies/frameworks or temporary artifacts were created. Existing test server aborts are preserved; the obsolete separate email fixture server was removed from the changed backfill test because the same local runtime now models that request.
- Other groups' changes and all eight earlier fixes were preserved. Root owns final runtime verification, check delegation, exact-SHA CI and integration.
