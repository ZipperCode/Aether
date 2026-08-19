# PROJECT KNOWLEDGE BASE

**Generated:** 2026-07-13
**Commit:** 8d9c4d75
**Branch:** master

## OVERVIEW

Aether is a self-hosted, multi-tenant AI gateway: Rust/Axum services normalize and route provider traffic, while a Vue 3/TypeScript admin UI manages users, providers, routing, quota, and operations.

## STRUCTURE

```text
Aether/
├── apps/aether-gateway/   # Main HTTP gateway and control plane
├── apps/aether-tunnel/    # Independently shipped outbound relay agent
├── crates/                # 27 focused policy, transport, data, and runtime crates
├── frontend/              # Vue/Vite admin and user application
├── docs/api/              # Compatibility contracts and generated audit matrices
├── tools/pressure/        # Standalone load and stage-report tooling
├── Makefile               # Local orchestration and database lifecycle commands
└── docker-compose*.yml    # Prebuilt, single-node, and release-local deployments
```

Generated/tool state such as `target/`, `node_modules/`, `frontend/dist/`, `.serena/`, `.omx/`, and `.agents/` is not an ownership domain.

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| Process bootstrap and HTTP composition | `apps/aether-gateway/src/main.rs`, `router.rs` | `run` builds state/tasks; catch-all proxy route is mounted last |
| Provider execution and transport | `apps/aether-gateway/src/execution_runtime/`, `crates/aether-provider-transport/` | Keep routing, execution, and wire policy separated |
| Admin/control behavior | `apps/aether-gateway/src/handlers/admin/`, `crates/aether-admin/` | Check matching frontend API contracts |
| Format conversion | `crates/aether-ai-formats/`, `docs/api/` | Strict passthrough and fail-closed rules apply |
| Persistence | `crates/aether-data-contracts/`, `crates/aether-data/` | Three SQL engines plus memory implementations |
| Provider selection and health | `crates/aether-provider-pool/`, `crates/aether-model-fetch/`, `crates/aether-scheduler-core/` | Policy belongs in focused crates, not handlers |
| Frontend feature work | `frontend/src/features/`, `frontend/src/views/` | Features own reusable domain UI; views orchestrate routes |
| Tunnel agent | `apps/aether-tunnel/` | Separate CLI/service with security-sensitive egress |
| Architecture constraints | `apps/aether-gateway/src/tests/architecture/` | Source-scanning tests enforce ownership boundaries |

## CODE MAP

| Symbol | Type | Location | Role |
|---|---|---|---|
| `run` | function | `apps/aether-gateway/src/main.rs` | CLI dispatch and process composition root |
| `AppState` | struct | `apps/aether-gateway/src/state/core.rs` | Shared gateway capabilities and runtime state |
| `build_router_with_state` | function | `apps/aether-gateway/src/router.rs` | HTTP route composition |
| `proxy_request` | function | `apps/aether-gateway/src/handlers/proxy/` | Compatibility catch-all request entry |
| `FormatId` | enum | `crates/aether-ai-formats/src/formats/id.rs` | Wire-format identity and aliases |
| `CanonicalRequest` | struct | `crates/aether-ai-formats/src/protocol/canonical.rs` | Cross-format request representation |
| `main` | function | `frontend/src/main.ts` | Vue, Pinia, i18n, and router bootstrap |

## CONVENTIONS

- Rust workspace uses edition 2021 and pinned Rust 1.95.0. Bare Cargo commands target only `aether-gateway`; use `--workspace` or `-p` deliberately.
- Cross-crate DTOs, traits, and errors belong in contracts crates. Gateway handlers do not own SQL or persistence implementations.
- Rust tests are predominantly inline; large gateway behavior suites live under `apps/aether-gateway/src/tests/`, not a conventional root `tests/` tree.
- Frontend uses npm lockfiles, strict TypeScript, Vue Composition API with `<script setup>`, and `@/*` for `frontend/src/*`.
- When a backend payload changes, trace its admin/data contract and frontend endpoint type before considering the change complete.

## ANTI-PATTERNS (THIS PROJECT)

- Do not edit `target/`, any `node_modules/`, `frontend/dist/`, or generated data-schema SQL directly.
- Do not bypass gateway architecture tests with compatibility shims, cross-domain re-export hubs, or handler-owned `sqlx` calls.
- Do not log tokens, provider keys, raw credentials, sensitive request bodies, or unredacted proxy URLs.
- Do not treat the API field-coverage matrix as a runtime allowlist; same-format traffic must preserve unknown JSON fields.
- Do not run `npm run lint` as a read-only check: it invokes ESLint with `--fix` and mutates files.
- Do not assume `npm run build` includes `vue-tsc`; use `build:with-typecheck` or run `type-check` separately.

## COMMANDS

```bash
cp .env.example .env
make dev                       # Gateway + Vite; memory backend by default
make dev-backend
make dev-frontend
make migration                 # Gateway --migrate
make backfill                  # Gateway --apply-backfills

cargo fmt --all --check
cargo nextest run -p aether-data
cargo nextest run --workspace --exclude aether-gateway --exclude aether-data

cd frontend && npm run type-check
cd frontend && npm run test:run
cd frontend && npm run build
```

## NOTES

- `make dev` may start Postgres/Redis through Docker when selected dependencies are unavailable; the default local runtime is in-memory.
- Prebuilt Compose and local-source deployment are different paths. Use `./deploy.sh` for a current-source image; standard Compose pulls its configured registry image.
- PostgreSQL/MySQL smoke tests require explicit test URLs; skipped tests are not proof of database compatibility.
- Current compose image defaults and release/example namespaces have historically diverged. Verify `APP_IMAGE` before deployment.
<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->
