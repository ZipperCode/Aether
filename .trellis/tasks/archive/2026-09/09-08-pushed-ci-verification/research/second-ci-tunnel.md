# Second CI: tunnel oversized-response category

- Status: source fix READY; focused runtime tests pending Root's exclusive Cargo slot.
- Candidate/run supplied by Root: `9026380d1957e7a57a746a409335f15eb1675515`, Rust CI `34217818968`.
- Sole write set: `apps/aether-tunnel/src/tunnel/stream_handler.rs` and this receipt. No Git mutation, CI operation, external provider/DB request, dependency/configuration change, or shared-source edit.

## Actual failures and common root cause

- `tunnel::stream_handler::tests::declared_oversized_response_emits_stable_stream_error`, original line `3303`.
- `tunnel::stream_handler::tests::streamed_oversized_response_emits_stable_stream_error`, original line `3348`.
- Both original assertions expected `Some("response_too_large")` but received `Some("upstream request failed")`.
- `BoundedBodyCollector::new` rejects declared length `9` against cap `8`; `push` rejects streamed accumulation `8 + 1` against the same cap (`crates/aether-http/src/body.rs:40,60`). Both `relay_upstream_response` failure branches increment the error metric and call `send_error(..., "response_too_large")` (original lines `1471,1603`). The rejection itself is already present.
- `send_error` routes all messages through `safe_stream_error_message`. That finite classifier omitted the existing fixed oversize category, so the two valid failures reached its generic fallback. This is a production error-category projection loss, not stale assertion text.

## Wire-format and consumer proof

- `apps/aether-tunnel/src/tunnel/protocol.rs` re-exports `aether_gateway_tunnel::protocol`, which re-exports the canonical contracts protocol.
- `aether_contracts::tunnel::encode_stream_error` (`crates/aether-contracts/src/tunnel.rs:695`) encodes `msg.as_bytes()` into `STREAM_ERROR`, flags `0`. In contrast, `ResetStreamPayload` is JSON; these are distinct frame formats.
- Agent `send_error` (original `stream_handler.rs:2219`) creates `MsgType::StreamError` with `Bytes::from_static(safe_message.as_bytes())`. There is no JSON `error_code` field to assert instead.
- Test `collect_stream_result` (original line `4086`) decodes that frame payload with `String::from_utf8` into `StreamResult.error`; neither schema conversion nor a second code field is hidden by the helper.
- Gateway `handle_proxy_frame` (`apps/aether-gateway/src/tunnel/embedded/hub.rs:1502`) likewise decodes the error frame as UTF-8, then applies its existing `safe_peer_stream_error` projection. That separate projection currently classifies this text as `tunnel stream relay error`; this is not an Agent wire-field migration. Root was notified of the separate downstream behavior and retains the ownership/scope decision for that file. It was not edited here.

## Minimal change

- Added one exact normalized comparison in `safe_stream_error_message` returning the static `"response_too_large"` category.
- Updated the modified function's purpose comment in Chinese: known bounded-response codes are preserved without exposing upstream diagnostics.
- Preserved both original CI tests and all their category/body assertions. No limit, timeout, frame schema, collector, decompression, credential handling, or raw-error forwarding changed. Unknown and diagnostic-bearing messages still use the existing finite projection.
- Existing classifier callers were traced: log projection, body-read failure, redirect failure, `send_error`, and `send_reset_stream`, all in the owned file. No new helper, registry, branch outside the exact category, or compatibility path was introduced.

## Verification

- PASS: `rustfmt --check --edition 2021 apps/aether-tunnel/src/tunnel/stream_handler.rs`.
- PASS: `git diff --check -- apps/aether-tunnel/src/tunnel/stream_handler.rs` and focused diff inspection. Git only reports the existing LF/CRLF working-copy warning.
- NOT RUN: Cargo compilation, Clippy, and runtime tests; Root prohibited main-target Cargo while concurrent writers are active. No full Tunnel/Gateway/workspace run is warranted by this source fix.
- Required next focused check after slot assignment: `cargo nextest run -p aether-tunnel --bin aether-tunnel --locked --no-fail-fast -E 'test(=tunnel::stream_handler::tests::declared_oversized_response_emits_stable_stream_error) | test(=tunnel::stream_handler::tests::streamed_oversized_response_emits_stable_stream_error)'`.
- Wire/decoded size enforcement is untouched, not newly runtime-validated by the source/format checks above. Final Linux CI belongs to the next exact pushed SHA.

## Cleanup / handoff

- No persistent process, local HTTP server, debug switch, temporary fixture, or generated build artifact was created by this worker.
- No ownership expansion was performed. Root owns downstream classification decisions, slot scheduling, integration, push, and exact-SHA CI verification.
