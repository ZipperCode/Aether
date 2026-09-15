# Fixture unblock acceptance receipt

## Changed files

- `apps/aether-gateway/src/execution_runtime/stream/execution.rs`
  - tunnel fixture now seals the PSK in `encryption_key_encrypted` and performs a
    pre-test migration through `decrypt_or_migrate_proxy_tunnel_psk`;
  - removed the stream tunnel `#[ignore]` and awaited the shared fixture;
  - terminal usage assertions wait for `completed`/`failed` before checking
    `billing_status` (`pending`/`void`).
- `apps/aether-gateway/src/execution_runtime/sync/execution.rs`
  - removed the sync tunnel `#[ignore]`, awaited the shared fixture, and waits
    for the successful terminal usage row before checking billing state.
- `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test/tests.rs`
  - removed the admin tunnel `#[ignore]` and awaited the shared fixture.

## Docker verification

Builder: `aether-gateway-builder:trellis-check`, Rust 1.95.0, desktop-linux,
shared `aether-trellis-cargo-registry`, `aether-trellis-cargo-git`, and
`aether-trellis-target` volumes.

- `cargo test --locked -p aether-provider-transport --lib antigravity::request -- --nocapture`: **8 passed, 0 failed, 490 filtered**.
- `cargo test --locked -p aether-data-contracts --lib antigravity_signature -- --nocapture`: **1 passed, 0 failed, 225 filtered**.
- `cargo test --locked -p aether-gateway --lib antigravity_signature -- --nocapture`: **8 passed, 1 failed, 0 ignored, 5427 filtered**.
- Gateway `cargo check` was not reached in the final `set -e` run because the focused gateway test failed.

## Blocking condition

The stream and admin tunnel tests now pass after correcting the fixture PSK to
the valid 44-character test key and pre-migrating the repository-backed
encrypted key. The only remaining failure is
`antigravity_signature_sync_authenticated_tunnel_recovery`, which times out
waiting for the first local-tunnel request frame.

Source evidence: `execute_execution_runtime_sync_with_retry_scope` calls
`execute_direct_sync_runtime_candidate`, which calls
`DirectSyncExecutionRuntime::execute_sync_with_response_started`; its
`send_request_inner` routes any tunnel proxy to the HTTP relay URL. The local
embedded tunnel path is instead owned by
`execute_sync_plan_with_report_context` →
`execute_sync_plan_via_local_tunnel`, but the retry-scope function does not
call it. Consequently no test-only fixture can exercise both this sync
recovery loop and the local embedded tunnel without changing that production
execution routing. No production retry semantics, public API, schema, or
configuration was changed.
