# Database Guidelines

> Database patterns and conventions for this project.

---

## Overview

<!--
Document your project's database conventions here.

Questions to answer:
- What ORM/query library do you use?
- How are migrations managed?
- What are the naming conventions for tables/columns?
- How do you handle transactions?
-->

(To be filled by the team)

---

## Query Patterns

<!-- How should queries be written? Batch operations? -->

(To be filled by the team)

---

## Migrations

<!-- How to create and run migrations -->

(To be filled by the team)

---

## Naming Conventions

<!-- Table names, column names, index names -->

(To be filled by the team)

---

## Common Mistakes

<!-- Database-related mistakes your team has made -->

(To be filled by the team)

## Scenario: Live regression database preparation

### 1. Scope / Trigger
Run when exercising ignored PostgreSQL tests or auditing schemas during an upstream merge.

### 2. Signatures
- `AETHER_TEST_DATABASE_URL` points only at a disposable database.
- Prepare through `AETHER_DATABASE_URL=<isolated-url> target/debug/aether-gateway db prepare` before the test selector.
- `cargo test -p aether-data-postgres live_ -- --ignored --test-threads=1` runs the existing live regression group.

### 3. Contracts
- Use the production bootstrap/migration API so schema contents and migration ledger agree.
- Applying raw migration SQL is valid for a separate schema inventory audit, not as the fixture for tests that call the migrator again.
- Seed referenced users and API keys before usage facts; remove dependent facts before their owners.
- Keep runtime smoke data in a separate database from regression tests.

### 4. Validation & Error Matrix
| Condition | Result |
| --- | --- |
| SQL applied without migration ledger | Historical view migrations may replay and fail with PostgreSQL `42P16` |
| Usage references an absent API key | PostgreSQL `23503`; fix the fixture, not the foreign key |
| Explicit ignored test lacks its database URL | Fail clearly, do not silently report success |

### 5. Good / Base / Bad Cases
- Good: production `db prepare`, then live tests against the same isolated URL.
- Base: ordinary unit tests do not require PostgreSQL.
- Bad: reuse a manually SQL-migrated audit database and misdiagnose migration replay as a production regression.

### 6. Tests Required
- `live_overview_cache_pricing_and_record_drilldown` must retain real foreign keys and create its API-key owner fixture.
- `live_usage_health_timeline_totals_match_breakdown` verifies database aggregation, not SQL text.
- Maintenance regression must verify actual analyzed relation statistics and missing-table skipping.

### 7. Wrong vs Correct
Wrong: insert a usage fact with an arbitrary `api_key_id`, or rerun the migrator after raw SQL setup without its ledger.

Correct: seed the referenced API key and prepare a fresh disposable database through the production migration path.
