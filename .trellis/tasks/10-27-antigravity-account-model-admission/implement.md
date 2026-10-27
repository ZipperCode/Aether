# Implementation

1. Extend the existing isolated Antigravity HTTP fixture with an incident-shaped model-admission regression. Parent runs it once against unchanged runtime code and records the failing unsupported-account hit assertion.
2. Add the borrowed Antigravity discovered-model eligibility policy in provider-pool and integrate ordinary and actual Pool/sticky runtime gates. Preserve mapping/allowlist behavior, diagnostic precedence and Pool representative isolation. Add focused policy/runtime regression cases.
3. Parent runs the new HTTP scenario, relevant provider-pool and gateway scheduler/pool/quota tests, formatting and Clippy/checks once implementation is integrated. Extend existing fixture only; avoid permanent source-text/wiring tests.
4. After passing smoke proof, update the existing Key admission/model-quota contract with discovered-model availability and incident regression guarantees. Review scope, commit only this task, archive the task and record verified session results.

## Validation commands

Use pinned Rust 1.95.0 and the repository CI environment (`CARGO_INCREMENTAL=0`, dev/test debug=0, `RUST_MIN_STACK=16777216`).

- Failing-before/passing-after: `cargo test -p aether-gateway --lib gateway_antigravity_account_scoped_argon_admission -- --nocapture`.
- Policy: `cargo nextest run -p aether-provider-pool`.
- Gateway: relevant `scheduler::candidate`, `dispatch::pool_scheduler`, Antigravity quota refresh and newly added native HTTP regression filters; run gateway library/binary suite if practical with existing cached build.
- Formatting: `cargo fmt --all --check`.
- Rust quality: repository `python3 tools/ci.py clippy-gateway` and relevant provider-pool Clippy.

No checks/build/tests/formatters in implementation agents; integration owner runs verification afterward. A real localhost HTTP exchange with observed per-account generation counts is mandatory, independent of unit-test results. No production mutation, external provider traffic or credential use.
