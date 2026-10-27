# Quality Guidelines

> Code quality standards for backend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

<!-- Patterns that should never be used and why -->

(To be filled by the team)

---

## Required Patterns

<!-- Patterns that must always be used -->

(To be filled by the team)

---

## Testing Requirements

<!-- What level of testing is expected -->

(To be filled by the team)

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)

## Scenario: Parallel Managed PostgreSQL Startup

### 1. Scope / Trigger

`ManagedPostgresServer` in `crates/aether-testing/testkit/src/postgres.rs` starts isolated local database processes for integration suites. A released ephemeral port can be claimed by another test before PostgreSQL binds it.

### 2. Signatures

- `ManagedPostgresServer::start() -> Result<Self, Box<dyn Error>>`
- `restart(&mut self) -> Result<(), Box<dyn Error>>`
- `database_url(&self) -> &str` and `port(&self) -> u16`
- Binary overrides: `AETHER_INITDB_BIN`, `AETHER_POSTGRES_BIN`, `AETHER_PG_CTL_BIN`.

### 3. Contracts

- Hold the initial TCP reservation throughout `initdb`; release it immediately before spawning PostgreSQL.
- Only initial startup may select a fresh port, and only after a failed bind reports `address already in use`. Limit attempts to five.
- Once returned to the caller, the URL, port, and data directory remain fixed across `restart()`, including failed restarts.
- Readiness requires the owned child to remain running and `current_setting('data_directory')` to match the fixture directory after canonicalization. A successful connection to a foreign database is not readiness.
- Bound each connect/query probe; a raw TCP listener can accept a connection without completing the PostgreSQL handshake.
- Stop failures retain child ownership and preserve the directory for cleanup retry.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Initial bind collision | Reserve a new port and retry before publishing the fixture |
| Restart bind collision | Return an error; preserve the published URL and database contents |
| Missing binary, initdb failure, unrelated startup failure | Return the failure without bind retries |
| Foreign database or incomplete handshake | Reject readiness within the startup deadline |
| Successful stop/drop | Reap the owned process and remove its workdir |

### 5. Good / Base / Bad Cases

- Good: a forced first-bind collision recovers and SQL reads/writes use the fixture's own database.
- Base: stop/restart keeps prior table data reachable through the same URL.
- Bad: changing the URL on restart, rerunning the entire suite to hide a collision, or accepting any listener as the fixture.

### 6. Tests Required

Run the opt-in live fixtures with real local PostgreSQL binaries:

```bash
cargo test -p aether-testkit --features postgres --lib live_ -- --ignored --nocapture
cargo test -p aether-integration-tests --test responses_websocket_e2e -- --nocapture
```

Collision regressions must assert owned data-directory identity, data persistence, stable restart endpoint, and workdir cleanup; keep them isolated from application databases.

### 7. Wrong vs Correct

- Wrong: reserve and release a port before `initdb`, then treat any successful PostgreSQL connection as startup success.
- Correct: retain the reservation during initialization, retry only initial bind collisions, and verify the connected database belongs to the owned child.
