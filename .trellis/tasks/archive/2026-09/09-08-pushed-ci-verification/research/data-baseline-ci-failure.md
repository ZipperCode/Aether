# Data baseline CI failure

## Observed failure

- Commit: `948c1c16f2f9927b37a8570b86767128abd6b7c8`; Rust CI run `34210248162`, Test (Data) job `102009266095`.
- Read-only raw job log: `gh api repos/ZipperCode/Aether/actions/jobs/102009266095/logs`.
- `cargo nextest run -p aether-data` compiled successfully in 57.95 s at `2026-09-08T09:30:36Z`.
- `lifecycle::migrate::tests::split_baseline_sources_match_executable_migrations` failed at `runtime/src/lifecycle/migrate/tests.rs:629:5` in 0.010 s, at `2026-09-08T09:30:43Z`.
- This is a schema-source consistency failure, not a compiler or database-runtime failure. The strict full-SQL equality assertion must remain intact.

## Exact cause and ownership

- Concatenating `runtime/schema/drivers/postgres/baseline/manifest.txt` produces exactly one additional SQL line versus `adapters/postgres/migrations/20260403000000_baseline.sql`: `revision bigint DEFAULT 0 NOT NULL,` in `public.system_configs`.
- The extra line is at `runtime/schema/drivers/postgres/baseline/001_types_and_tables.sql:1052`. `git blame` attributes it to local commit `b917064af8e78c65932ea2c39645ff5a9d05ff7b`.
- That commit also added `20260906000000_add_system_config_revision.sql` without changing the executable historical baseline. The incremental migration already adds the same column using `ADD COLUMN IF NOT EXISTS`.
- Current schema and new-database behavior separately retain revision in `runtime/schema/logical/003_auth_config.toml`, `runtime/schema/generated/postgres/baseline/003_auth_config.sql`, and `runtime/schema/bootstrap/postgres/001_types_and_tables.sql`.
- The source README explicitly distinguishes historical driver fragments, incremental executable migrations, and the current generated/bootstrap schema. `compose_schema.sh compose` would overwrite the historical executable artifact; `split` would bulk rewrite fragments. Neither is appropriate for this isolated extra source line.
- Local pre-edit baseline artifact SHA-256 (CRLF checkout bytes): `D964C8F630B162F751FD6CD1D48E2EB9EC8981D950267643E5F967B234090915`.

## Applied bounded fix

Removed only the prematurely backported revision column from the historical driver baseline fragment after Root granted the additional single-file lock. The incremental migration, current logical/generated/bootstrap schema, strict test, and all executable migration checksums remain unchanged. No database connection, migration execution, source rebaseline, or generated-file edit was required.

The driver's pg_dump-origin header records the historical source provenance; the schema README designates `drivers/postgres` as maintained source, separately from the machine-written `generated/**` output. The one-line source correction therefore uses `apply_patch`, not a rebaseline generator. `apply_patch` left three context lines with LF in this CRLF checkout; the existing native `unix2dos` formatter restored the original checkout line-ending convention without adding any Git-visible changes.

## Validation status

- Before fix: exact in-memory concatenation comparison fails; baseline 137,639 characters and composed 137,680 characters in the CRLF checkout, with only the 41-character revision line differing.
- After fix: exact in-memory full-string equality passes, both 137,639 characters. Independently concatenating `ReadAllBytes` for all six fragments gives 137,639 bytes with the same SHA-256 as the immutable baseline; the baseline artifact SHA-256 is unchanged from the value above.
- `git diff --check -- crates/aether-data/runtime/schema/drivers/postgres/baseline/001_types_and_tables.sql`: PASS. Source diff is exactly zero insertions and one deletion.
- `git diff --quiet -- crates/aether-data/adapters/postgres/migrations crates/aether-data/runtime/schema/bootstrap crates/aether-data/runtime/schema/logical crates/aether-data/runtime/schema/generated`: PASS.
- Root granted the exclusive Cargo slot after the parallel data-contracts writer finished source changes. `cargo test -p aether-data --lib lifecycle::migrate::tests::split_baseline_sources_match_executable_migrations -- --exact`: PASS, exit 0, one test passed, zero failed, 369 filtered out, test execution 0.01 s. Native main-target data/dependency compilation took 2m 27s; no gateway target was compiled. The unchanged test checked both historical baseline and build-generated bootstrap snapshot composition.
- No generator invocation is needed: logical/generated SQL is unchanged, and the full historical manifest composition was checked directly. No full Data/gateway/workspace suite or real database test.

## Final status and cleanup

- READY for Root integration/review. Files changed by this writer: the single historical source fragment and this receipt; the initially assigned test file did not need modification.
- No remaining finding in this bounded failure. Native focused PASS is not a Linux/exact-final-SHA CI pass; Root owns the combined commit, push, and remote verification.
- Cargo session `93800` exited successfully. No task-owned persistent process, temporary file, database service, generated schema edit, Git staging/commit, push, or CI mutation was created by this writer. Shared Cargo cache/build outputs are retained for subsequent task validation.
