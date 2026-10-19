# Maintenance Review: PR #885, Current 106 Tables & Body Retention Defaults

## 1. PR #885: Gateway Database Automatic Maintenance

- **Source**: Upstream PR #885 (`feat(maintenance): expand gateway db maintenance to include all database tables`)
- **Upstream PR Diff / URL**: `https://github.com/fawney19/Aether/pull/885` (commit `0fd0c5050451f24dcc9a1ddce565e02741d67f15`)
- **Problem Analyzed**:
  The gateway weekly maintenance worker (scheduled on Sunday morning to execute `VACUUM ANALYZE`) originally only targeted three tables: `["usage", "request_candidates", "audit_logs"]`. High-volume data tables such as `usage_body_blobs`, `usage_http_audits`, and reporting rollups (`stats_*`, `dashboard_*`) were left unmaintained by the gateway automated task.
- **Upstream Proposal**:
  1. Expand `DB_MAINTENANCE_TABLES` to cover all database tables in the project.
  2. Add a `to_regclass` existence check in PostgreSQL maintenance worker before issuing `VACUUM ANALYZE`.
  3. Update `db_maintenance_continues_across_table_failures` unit test to assert dynamically over `DB_MAINTENANCE_TABLES`.

---

## 2. Current 106 Schema Tables vs. PR #885's 105 Tables

- **Investigation**:
  - PR #885 included 105 table names in `DB_MAINTENANCE_TABLES`.
  - To determine the true ground-truth schema of the current repository, all 86 SQL migration files (`crates/aether-data/adapters/postgres/migrations/*.sql`) were applied sequentially to an isolated PostgreSQL 18 container (`aether-upstream-oct19-test` on `127.0.0.1:25439`).
  - Querying `SELECT tablename FROM pg_tables WHERE schemaname = 'public'` revealed exactly **106 tables**.
- **Discrepancy Analysis**:
  - Computing the set difference:
    ```python
    current_migrated_tables - pr_885_tables == {"model_endpoint_bindings"}
    pr_885_tables - current_migrated_tables == set()
    ```
  - `model_endpoint_bindings` was introduced in migration `20260811000000_add_model_endpoint_bindings.sql` but was omitted from the static list in PR #885.
- **Resolution**:
  - In `apps/aether-gateway/src/maintenance/runtime.rs`, `DB_MAINTENANCE_TABLES` has been populated with the complete, ground-truth list of all **106** tables (sorted alphabetically), correctly including `model_endpoint_bindings`.

---

## 3. Missing-Table Semantics & Robustness (`to_regclass`)

- **Code Location**: `crates/aether-data/runtime/src/backend/maintenance/postgres.rs` (`PostgresBackend::run_table_maintenance`)
- **Implementation**:
  The pre-check executes `SELECT to_regclass($1) IS NOT NULL` binding `format!("\"{table_name}\"")`.
  - **Relation Resolution**: Rather than hardcoding a `public.` prefix, it uses an unqualified quoted identifier `"{table_name}"`. This resolves the relation against PostgreSQL's active `search_path` using the identical resolution semantics as the succeeding `VACUUM ANALYZE "{table_name}"` statement. This prevents false skips in deployments utilizing custom schemas or search paths.
  - **Transparent Error Handling**: Unlike PR #885's `.unwrap_or(true)`, the existence pre-check propagates errors with `map_postgres_err()?`. Real connection errors or query cancellations are reported honestly rather than suppressed.
  - **Skip Semantics**: When `exists` is `false` (relation does not exist), the loop executes `continue;` immediately without incrementing `summary.attempted` or issuing `VACUUM ANALYZE`. This prevents `42P01` (`relation does not exist`) PostgreSQL errors and eliminates spurious warnings for optional or unmigrated tables.
---

## 4. Body Capture Policy & Oversized Retention Default Interaction

- **System Config Keys (PR #755)**:
  - `request_record_compact_base64_images`: boolean, default `false`.
  - `request_record_max_body_size_kb`: u64, default `0`.
- **Conflict Resolution**:
  - In `crates/aether-admin/src/system.rs`, conflict #1 was resolved by preserving `agent_format_bridge_mode` alongside the boolean keys match arm including `REQUEST_RECORD_COMPACT_BASE64_IMAGES_KEY`.
- **Interaction with Local Retention (`8e9e1836c` / PR #830)**:
  - In `apps/aether-gateway/src/data/state/integrations.rs`:
    `usage_body_capture_limit_bytes_from_value` filters `*kilobytes > 0`. Therefore, the default `0` maps to `max_body_bytes: None`.
  - In `crates/aether-usage/runtime/src/body_capture.rs`:
    `limit_usage_body_capture_value` does not truncate when `max_body_bytes` is `None` and image compaction is `false`.
  - **Result**: Default behavior retains **unlimited full bodies**. When captured bodies exceed the 1 MiB queue payload ceiling, local commit `8e9e1836c`'s direct persistence path (`try_write_terminal_direct_fallback`) writes the unstripped bodies directly to PostgreSQL. Only when explicitly configured ($> 0$) are bodies truncated up front before queuing.
- **Specification**:
  - Updated `.trellis/spec/aether-usage-runtime/backend/terminal-body-retention-contract.md` with a complete 7-section specification covering the real configuration keys and default unlimited semantics.

---

## 5. Verification & Test Suite

1. **Docker PostgreSQL 18 Live Checks**:
   - Tested `SELECT to_regclass('public."test_table"') IS NOT NULL` on table creation and dropping in `aether-upstream-oct19-test`.
   - Applied all 86 migrations and verified exact 106 public tables count.
2. **Gateway Unit Tests**:
   - `apps/aether-gateway/src/maintenance/runtime/tests.rs`:
     - `db_maintenance_continues_across_table_failures` dynamically validates all `DB_MAINTENANCE_TABLES.len()` (106) tables and assertions.
3. **Data Runtime Live Test**:
   - `crates/aether-data/runtime/src/backend/postgres.rs`:
     - Added `live_table_maintenance_skips_missing_tables_and_vacuums_existing` (`#[ignore = "requires AETHER_TEST_DATABASE_URL"]`).
     - Explicitly expects `AETHER_TEST_DATABASE_URL` (avoiding false-positive passes when `--ignored` is called).
     - Generates an isolated probe table with unique UUID suffix (`maint_probe_{probe_id}`) and seeds two test rows.
     - Verifies unanalyzed baseline `pg_class.reltuples == -1.0`.
     - Runs maintenance with both the probe table and an explicit nonexistent table (`maint_probe_missing_{probe_id}`).
     - Verifies `summary.attempted == 1` and `summary.succeeded == 1`.
     - Verifies physical maintenance execution: `pg_class.reltuples == 2.0` and `pg_stat_user_tables.last_analyze IS NOT NULL`.
     - Guarantees teardown with `DROP TABLE IF EXISTS`.
     - Selector: `-p aether-data live_table_maintenance_skips_missing_tables_and_vacuums_existing -- --ignored`.
4. **Admin System Config Tests**:
   - `request_record_body_capture_defaults_are_backward_compatible`: asserts `request_record_compact_base64_images` is `false` and `request_record_max_body_size_kb` is `0`.
   - `request_record_body_capture_updates_validate_types_and_bounds`: asserts boolean parsing and boundary checks.
