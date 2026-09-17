# Design
Port upstream commit rather than reimplementing the solution. Review its six-file diff and current queue/runtime callers with CodeGraph before applying. Preserve local changes when adapting hunks; do not replace whole files from upstream.
Owner files:
- crates/aether-usage/runtime/src/queue.rs
- crates/aether-usage/runtime/src/runtime.rs
- crates/aether-data/adapters/postgres/src/usage/queries/find_by_id_sql.sql
- crates/aether-data/adapters/postgres/src/usage/tests.rs
- frontend/src/features/usage/utils/__tests__/body-document-engine.spec.ts
- docs/operations/concurrency-design-audit-2026-09-09.md
Verify the actual upstream file paths with git show before editing. No overlap with signature implementation. Preserve upstream attribution in the eventual root commit.
