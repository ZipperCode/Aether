# Execution

1. Add consumer-visible failing regression before production edits; parent runs baseline once.
2. Implement successful raw remote evidence, shared constant/export and focused provenance tests; verify before proceeding.
3. Verify background/admin persistence, preset non-evidence and failed-refresh LKG with real behavior; no forwarding-only wrappers.
4. Implement exact shared guard and four integrations; preserve PoolGroup/Antigravity and existing denial precedence. Verify guard before scenario expansion.
5. Migrate affected OAuth fixtures and implement dedicated real loopback acceptance matrix, including independent versioned model bootstrap and WebSocket per-turn checks.
6. Run final checks once after edits, fix failures directly, and exercise independent curl streaming/basic smoke using a temporary ignored fixture launcher. Remove the launcher afterwards.

## Commands

```sh
cargo test --locked -p aether-provider-transport codex_model_discovery
cargo test --locked -p aether-model-fetch codex_model_discovery
RUST_MIN_STACK=16777216 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo nextest run --locked -p aether-gateway --lib --bins -E 'test(codex_model_discovery)'
RUST_MIN_STACK=16777216 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo nextest run --locked -p aether-gateway --lib --bins -E 'test(codex) | test(responses_pool) | test(antigravity)'
cargo test --locked -p aether-provider-transport
cargo test --locked -p aether-model-fetch
```

All workers skip builds/tests/lint/formatters while editing. Parent owns sequential verification gates, final integration and commit. LSP references attempted at session start: server exited unexpectedly (code 0); use approved anchors and Codegraph fallback.

## Verification evidence

- Before production changes, transport behavior failed (`None` instead of `key_model_disabled`); four fetch provenance regressions failed on missing namespace.
- Producer focused tests: 4 passed. Background/direct-admin persistence and independently authenticated catalogs: 5 passed.
- Shared eligibility focused tests: 3 passed. Final provider-transport full suite: 606 passed; model-fetch full suite: 115 passed (concurrent Antigravity tests increased the transport count).
- Final dedicated discovery/production-body-bounds suite after removing smoke scaffolds: 17 passed, including real HTTP/SSE/WebSocket, final-body admin rewrites, pool page/frozen-window/hot-fallback and versioned bootstrap. Cleanup verification including Gateway architecture and pool cursor/scheduler scenarios: 95 passed.
- Final related Gateway Codex/Responses/Antigravity and affected compact/cross-format/scheduler fixtures: 406 passed, including the final-body rule regression. Gateway Clippy (`lib/bins/examples`, `-D warnings`) and touched-file rustfmt check passed.
- Review corrected redundant cursor precheck (common transport remains authoritative) and moved admin eligibility to the actual body model after endpoint rules/finalization, still returning the original test request on skip. The real HTTP rewrite regression proves free zero-send denial and paid advanced execution.
- Independent curl advanced SSE observed full `response.completed`, `free=0`, `paid=1`, upstream `gpt-6.1-sol`; following basic SSE completed with `free=1`, `paid=1`, `free_models=["gpt-basic-test"]`. Deterministic `single_account` fixture ordering avoids existing cache-affinity promotion. Temporary launcher/counts endpoint removed. No production account calls or settings changed.
- Concurrent unrelated Antigravity files appeared during execution; leave them untouched and unstaged. Initial workspace and staged area were clean.
