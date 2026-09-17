# Oversized FULL usage body retention
## Requirements
Integrate upstream PR #830 (5842c7232ecd6cdfa95994da9afaac14f5a8deb0) while retaining this fork's current contracts.
- Preserve original complete terminal FULL bodies through existing bounded direct persistence when queue encoding would strip them.
- Avoid duplicate enqueue after successful direct persistence.
- Retain existing queue limits, pressure checks, write concurrency and fallback rules; report truncated state when full retention is unavailable.
- Expose four body capture states in usage-ID PostgreSQL queries consistently with request-ID queries.
## Acceptance
Focused oversized event and unavailable/bounded fallback regressions pass; projection and upstream frontend body-download regressions pass. No schema migration, broader queue rewrite or old-data recovery claim.
## Scope
Only six files from upstream commit and necessary directly related fixes. No deployment or other upstream PRs.
