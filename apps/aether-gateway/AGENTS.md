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
python tools/ci.py preflight --dry-run
python tools/ci.py preflight
python tools/ci.py clippy-gateway
python tools/ci.py gateway
cargo run -p aether-gateway -- --app-port 8084
```

- 从仓库根目录运行；需预先安装 Python 3、仓库固定的 Rust 1.95.0（含 rustfmt/Clippy）、宿主平台编译链接工具及 cargo-nextest。入口不安装工具、不读取 `.env`、不启动服务。
- `preflight` 依次执行全仓格式检查、Gateway Clippy（lib/bins/examples）和 Gateway 测试（lib/bins）；任一阶段失败即返回失败。Gateway 使用 `--no-fail-fast --locked`，一次收集所有测试失败，仍返回非零退出码。
- 入口统一 CI 的 incremental=0、dev/test debug=0 和 Gateway 16 MiB 测试栈；`--dry-run` 仅展示计划，并非编译或测试通过。已有 Bash/Make 环境可用 `make ci` / `make preflight` / `make ci-gateway`。
- 这不是完整工作区、前端或真实 PostgreSQL CI；原生 Windows 与 Linux CI 的 OS/linker/数据库条件仍不同，Linux mold 只在 CI job 中配置，不强制用于本地。
