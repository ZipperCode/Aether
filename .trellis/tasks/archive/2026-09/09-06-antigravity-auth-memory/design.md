# Technical Design

## Auth allowlist snapshot

- Extend the system-config contract and every adapter with an atomic monotonic `revision` and a single-row strong-read result `{ revision, value }`.
- Gateway auth resolution performs a strong revision read on every refresh boundary. A process-local singleflight snapshot stores `revision` plus an immutable parsed, null, or invalid outcome; unchanged revisions reuse it, changed revisions perform one full read and parse.
- Parse bearer hashes into `HashSet<String>`. Normalize auth-context cache keys to a bearer hash so raw bearer strings are not retained by cache entries.
- Keep the existing strong-read boundary so cross-node revocation remains observable on the next refresh.

## Candidate materialization

- Keep routed cross-page collection and the existing scheduler ranking algorithm.
- Add a gateway-side lightweight ranking snapshot containing only IDs and ranking/auth-channel facts. Resolve and decrypt a transport only when `next_attempt()` selects that candidate.
- Move required ranking facts into the minimal projection; do not read transport or retain raw auth JSON during ranking.
- Change routed resolved-page cache values to lightweight ranked/skipped snapshots. Full transports remain only in the existing bounded short-lived transport cache.

## Model-fetch and Pool startup

- Add one shared `StoredProviderCatalogModelFetchCandidate` projection across memory, SQLite, PostgreSQL, and MySQL. It excludes credentials and runtime status, and returns `upstream_metadata` only for active auto-fetch Keys.
- Read that projection once for target collection and again at batch-end reconciliation so current writes and concurrent admin changes remain visible. Persist each successful Key result, but reconcile Provider-wide whitelist availability once per successful Provider.
- Select Pool score rebuild IDs from maintenance summaries, then retain the existing strong single-Key read before updating a score. Run one explicit startup rebuild, consume the interval's immediate tick, and enter the periodic loop without a duplicate startup cycle.

## Compatibility and rollout

- Update memory, SQLite, PostgreSQL, and MySQL adapters plus schema migrations/baselines consistently.
- Preserve cache-key dimensions and existing format/provider special cases.
- Code rollback may restore the previous consumers while retaining the additive schema revision column; the migration itself is not destructive.
- Destructive whole-config purge remains outside this design. Ordinary delete uses an incrementing null tombstone so delete/recreate cannot reuse a live snapshot revision.
