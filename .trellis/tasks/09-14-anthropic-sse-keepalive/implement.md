# Execution

1. Trellis implement agent reads manifests, PRD/design, guidelines and all build_sse_body_stream callers. It owns execution_runtime/stream/execution.rs and strictly necessary existing stream tests.
2. Add a scoped regression around real build_sse_body_stream, then minimally fix record-boundary-aware keepalive emission. Reuse existing Tokio/test helpers without new dependencies.
3. Include a local mock-upstream/final-HTTP-body test covering native Anthropic thinking, consecutive tools, escaped arguments, tool results and message_stop. Synthetic requests only.
4. Validate in existing aether-gateway-builder:trellis-check Docker image (Rust 1.95). Main coordinator owns Docker execution to avoid cache contention. Use aether-trellis-target, aether-trellis-cargo-registry and aether-trellis-cargo-git caches. Set RUST_MIN_STACK=16777216. Commands: cargo test --locked -p aether-gateway --lib <scoped SSE filter> -- --nocapture; cargo check --locked -p aether-gateway --lib; rustfmt --edition 2021 --check <changed Rust files>; git diff --check. Require nonzero selected tests.
5. Independent trellis-check agent reviews/fixes scoped code and acceptance. Coordinator records SSE framing contract and receipts, commits in Chinese and finishes task/journal.
6. Merge into master without losing or committing Antigravity WIP, verify preservation, and remove task-owned temporary worktree/containers/files. No remote push or deployment.
