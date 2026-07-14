# AETHER GATEWAY KNOWLEDGE BASE

## OVERVIEW

Main Axum ingress, compatibility front door, control plane, and runtime composition crate; persistence and reusable policy remain in workspace crates.

## STRUCTURE

```text
src/
├── api/                # Route registration only
├── handlers/           # Admin, public, internal, proxy behavior owners
├── state/              # AppState and runtime capability assembly
├── execution_runtime/  # Sync/stream execution and transport
├── routing/            # Candidate resolution
├── orchestration/      # Policy effects and execution coordination
├── scheduler/          # Candidate scheduling
├── maintenance/        # Background operational loops
└── tests/              # Behavior and source-scanning architecture suites
```

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| Startup/CLI/database lifecycle | `src/main.rs` | Composition root; avoid adding domain logic |
| Router order and SPA fallback | `src/router.rs` | Specific routes precede `/{*path}` catch-all |
| Public API mounts | `src/api/` | Thin registration seam |
| Admin domains | `src/handlers/admin/` | Provider, model, system, users, billing, observability |
| Compatibility proxy | `src/handlers/proxy/` | Catch-all HTTP entry |
| Shared dependencies | `src/state/core.rs` | `AppState::new()?.with_*` assembly |
| Sync/stream provider calls | `src/execution_runtime/` | Keep transport separate from route registration |
| Architectural constraints | `src/tests/architecture/` | SQL ownership, module boundaries, sensitive logging |

## CONVENTIONS

- `api/` mounts routes; domain implementation stays in the matching handler/runtime owner.
- Keep each admin `mod.rs` thin and expose one stable route seam. Put shared code in the nearest legitimate `shared` owner.
- Small unit tests stay beside code. Large HTTP/runtime behavior belongs under `src/tests/{frontdoor,control,ai_execute,...}`.
- Use `GatewayError`/existing response paths for HTTP failures; preserve sanitized client-facing errors.
- Payload changes must be traced through contract crates and frontend endpoint types.

## ANTI-PATTERNS

- Never write `sqlx` or domain SQL in handlers; go through data contracts/backends.
- Never borrow helpers across unrelated admin domains or create glob/compatibility re-export hubs.
- Never mount a specific route after the proxy catch-all.
- Do not accumulate business logic in `main.rs`, router registration, or `AppState` construction.
- Do not expand crate-wide lint allowances or use existing giant files as a template.
- Do not remove or bypass architecture tests; update ownership rather than weakening checks.
- Never log tokens, keys, raw bodies, or other secrets.

## COMMANDS

```bash
cargo check -p aether-gateway
cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings
RUST_MIN_STACK=16777216 cargo nextest run -p aether-gateway --lib
RUST_MIN_STACK=16777216 cargo nextest run -p aether-gateway --bin aether-gateway
cargo run -p aether-gateway -- --app-port 8084
```
