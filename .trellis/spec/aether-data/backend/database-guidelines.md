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

### Assuming the managed postgres fixture URL shape

`ManagedPostgresServer::database_url()` points at the test's uniquely created
database. It only ends in `/postgres` for the ephemeral local-binary mode; with
`AETHER_TEST_POSTGRES_URL` it is `{base}/{owned_database}`. Never derive sibling
database URLs with `strip_suffix("/postgres")` or string concatenation — that
panics or mis-targets under the external-server mode. Parse and rewrite the
path instead:

```rust
let target_url = server
    .create_sibling_database("dashboard_restore_test")
    .await
    .unwrap();
```

`create_sibling_database` parses the base URL, sets the path to a unique
`{hint}_{pid}_{uuid}` database, creates it, and registers it so the fixture
`Drop` terminates lingering connections and drops every sibling it created.
Tests must not `CREATE DATABASE` with fixed names on a shared server — repeated
runs collide, and nobody owns the cleanup.
