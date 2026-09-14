# Validation receipt

## Environment
- Isolated worktree: D:/Project/GitHub/Aether-anthropic-sse-keepalive.
- Docker context: desktop-linux. Existing builder aether-gateway-builder:trellis-check, image sha256:e4977617277d9929f2f77f5cd35e8dfed881a4b4955cb721d36af152bb969c48, Rust 1.95.0.
- Source mounted read-only at /build. Existing aether-trellis-target, aether-trellis-cargo-registry and aether-trellis-cargo-git caches. RUST_MIN_STACK=16777216, CARGO_BUILD_JOBS=4. Locked/offline Cargo; no dependencies downloaded or production/provider requests made.

## Completed evidence
- Initial gateway test build (`cargo test --locked --offline -p aether-gateway --lib --no-run`): exit 0, 18m32s. This warm build began before the new test module was parsed and is not fixed-source acceptance.
- Existing baseline SSE tests: 6 passed, 0 failed, 5524 filtered. Not a claim about new tests.
- Red mutation: mounted the base commit's exact build_sse_body_stream over the changed file while retaining all new tests and the independent boundary helper. `cargo test --locked --offline -p aether-gateway --lib sse_body_stream_keepalive_waits_for_prefetched_and_live_event_boundaries -- --nocapture` returned 101: 0 passed, 1 failed, 5533 filtered; failure is the intended `no heartbeat inside a partial event` assertion, not compilation. Test runtime 0.01s; build 13m07s. See red-output.txt.
- Independent source review: no heartbeat regression found. Changed-file rustfmt and git diff --check passed. See check-receipt.md.

## Build-cache correction
The first return from the newer-mtime red bind overlay to the older-mtime fixed source reused the red executable. Its 8 passed / 3 failed result in stale-mutant-output.txt is stale-mutant evidence, not a fixed-source result. The fixed source mtime was advanced without changing bytes; SHA-256 remained A929417A37E774A60533C4750C994179D07F622602F69FE0502484F4CE87F5FF. The authoritative rerun aether-sse-verify-fixed printed Compiling aether-gateway and completed its test/check commands successfully.

## Fixed-source test result
- `cargo test --locked --offline -p aether-gateway --lib sse_body -- --nocapture`: 11 passed, 0 failed, 0 ignored, 5523 filtered; runtime 1.18s. Includes all four new cases, six existing wrapper tests and native Anthropic sender-alive termination.

## Compiler and formatting result
- `cargo check --locked --offline -p aether-gateway --lib`: passed; Finished dev profile in 12m32s. Fixed test build completed in 7m05s. See verify-output.txt.
- The combined container exits 1 solely because its Rust 1.95 Linux image lacks rustfmt. No compiler/test failure occurred. Formatting was substituted using the existing pinned Windows 1.95.0 toolchain, rustfmt 1.9.0-stable (59807616e1 2026-04-14).
- Both changed Rust files pass `rustfmt --edition 2021 --config skip_children=true --check`; staged and unstaged `git diff --check` pass.
- No full-workspace test suite, Clippy or live-provider run was performed; these are outside the approved scoped test/check plan.

## Integration pending
- Commit, protected merge and original WIP preservation check.

## Limits
Live GLM/Claude acceptance and production deployment are unverified and excluded. The HTTP regression is an actual two-hop loopback test around the production output wrapper; it does not boot the complete auth/routing/billing stack. General >1 MiB filter continuation corruption is an existing separately recorded limitation.
