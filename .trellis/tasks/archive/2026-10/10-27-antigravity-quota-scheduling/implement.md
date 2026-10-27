# Execution

Approved full execution detail is supplied inline to the parent and in `local://antigravity-quota-scheduling-plan.md` for delegated handoff.

1. Implement quota identity/map/reset behavior and regressions; verify provider-pool before dependent changes.
2. Implement Antigravity derived hard block and probe eligibility assertion; verify provider-pool and focused probe behavior.
3. Implement key/model snapshot isolation and order/switch/manual-ban/PoolGroup regressions; verify focused candidate runtime behavior.
4. Invalidate candidate caches only after successful persisted model quota refresh; verify refresh behavior.
5. Add real-catalog Pool cursor ordinary/sticky refresh recovery and all-exhausted assertions.
6. Add localhost HTTP serving regression for ordinary and Pool modes, A→B→A, failed fetch, manual guard, and all-exhausted 503 with separated generation/probe counts and final Gemini text.
7. Run the complete approved commands, relevant formatting/checks, update the existing quota contract, and deliver verified changes according to repository commit rules.

Commands from repository root:

```sh
cargo nextest run --locked -p aether-provider-pool
RUST_MIN_STACK=16777216 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo nextest run --locked -p aether-gateway -E 'test(antigravity) | test(scheduler::candidate::runtime) | test(pool_key_cursor) | test(pool_runtime_guards)'
RUST_MIN_STACK=16777216 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --locked -p aether-gateway gateway_antigravity_quota_refresh_restores_serving -- --nocapture
```

Delegated editors do not run builds, tests, lint, or formatters. Parent owns verification gates and integration. No real Google/PostgreSQL/Redis, production credentials, source upload, or external infrastructure changes.
