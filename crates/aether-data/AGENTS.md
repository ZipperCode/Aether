# AETHER DATA KNOWLEDGE BASE

## OVERVIEW

Runtime persistence implementation for memory and PostgreSQL, plus migration, backfill, export, bootstrap, and schema-maintenance workflows. MySQL and SQLite are intentionally unsupported after the full upstream synchronization.

## STRUCTURE

```text
src/driver/        # Pool/transaction/infrastructure primitives
src/repository/    # Domain implementations per backend
src/backend/       # Driver selection and app-facing composition
src/lifecycle/     # Migration, backfill, export
schema/logical/    # Portable human-maintained table definitions
schema/drivers/    # Authoritative executable baseline fragments
schema/generated/  # Checked-in generator output; audit only
migrations/        # Composed runtime SQL embedded by sqlx
backfills/         # Versioned operational data changes
```

## WHERE TO LOOK

| Task | Location | Notes |
|---|---|---|
| Public implementation surface | `src/lib.rs` | Shared contracts usually belong in `aether-data-contracts` |
| Driver/config selection | `src/database.rs`, `src/backend/` | Composition owner |
| Domain persistence | `runtime/src/repository/<domain>/`, `adapters/postgres/src/` | Explicit memory and PostgreSQL implementations |
| Migration/bootstrap | `src/lifecycle/migrate*`, `schema/bootstrap/postgres/` | PostgreSQL snapshot is build output |
| Schema source and generation | `schema/README.md`, `schema/logical/` | Start portable shape changes here |
| Cross-database lifecycle | `src/lifecycle/{backfill,export}.rs` | Backfills are not migrations |

## CONVENTIONS

- Cross-crate DTOs, repository traits, and `DataLayerError` belong in `aether-data-contracts`; implementation-only types may stay here.
- Repository modules use explicit PostgreSQL adapters and `memory.rs` implementations. Do not restore MySQL/SQLite modules or hide domain SQL in a generic `sql.rs`.
- Driver modules own connection primitives, not domain queries. Backend chooses drivers; repositories do not.
- Portable table changes begin in `schema/logical/*.toml`; physical SQL remains dialect-specific where required.
- Append timestamped migrations/backfills. Applied versions and paths are compatibility surfaces.

## ANTI-PATTERNS

- Never hand-edit `schema/generated/**`; regenerate it.
- Never edit composed executable baseline SQL independently of its manifest fragments.
- Never check in the PostgreSQL empty-database snapshot; `build.rs` emits it into `OUT_DIR`.
- Do not use `schema/overrides/` for ordinary tables, columns, or indexes.
- Do not put domain SQL in pool modules or driver-selection branches in repositories.
- Do not introduce MySQL/SQLite migrations or compatibility branches.
- Do not renumber/remove applied migration or backfill versions casually.

## COMMANDS

```bash
bash crates/aether-data/schema/compose_schema.sh generate
bash crates/aether-data/schema/compose_schema.sh compose
bash crates/aether-data/schema/compose_schema.sh check
cargo test -p aether-data
cargo test -p aether-data split_baseline_sources_match_executable_migrations
cargo run -p aether-data-schema --bin aether-schema -- check
```

Use `compose_schema.sh split` only for a deliberate rebaseline or bulk rewrite.
