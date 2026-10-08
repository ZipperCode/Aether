# Execution

1. Confirm clean baseline and pinned upstream; load relevant specs.
2. Start task and run git merge --no-commit --no-ff 911c7f887; inventory conflicts.
3. Assign disjoint backend/frontend conflicts; integrate semantic preservation of local fixes. Ask user only for unresolved behavior decisions.
4. Run cargo fmt --all --check, workspace compilation/tests as applicable, frontend type-check/test/build, and isolated runtime smoke. Investigate failures and rerun affected checks after fixes.
5. Record verification in task artifacts, remove throwaway scaffolds, commit the merge only after acceptance. Do not push.

## Integration decisions

- Merged upstream `911c7f887` (27 upstream-only commits) into local `1866164e9`; retained both histories and local Endpoint/Responses/quota/auth-memory fixes.
- Adopted upstream unified Provider scheduling workspace, Provider-first group ordering, selectable groups and separate customer billing. Removed retired card-view/global-key UI tests; migrated valid pagination coverage.
- Unified CLI profile cache synchronization on the upstream four-profile mechanism while preserving local pin precedence, newer-only restoration and frontdoor-only node synchronization.
- Retained three locally enhanced protocol/health documents deleted by upstream's broad documentation cleanup; accepted other retired documentation deletions and removed dangling links.

## Executed verification

- `cargo check --workspace --all-targets --locked`: passed after supplying new fields to a local CLI-profile test fixture.
- `cargo clippy --workspace --exclude aether-integration-tests --all-targets --locked -- -D warnings` and `cargo fmt --all --check`: passed after final Rust repairs. Exclusion matches repository CI. A broader Clippy invocation found an unchanged `collapsible_match` in `crates/aether-testing/integration/src/bin/capacity_curve_baseline.rs:804`; left unrelated code unchanged.
- Workspace tests excluding gateway/data/integration tooling: 4,136 passed; 48 ignored.
- Frontend type-check, 265 test files / 2,198 tests, and production build (including VSCodex): passed. Two retired UI-contract tests were removed rather than restoring removed behavior.
- `bash tests/update_compose_safety_test.sh`, protocol field-coverage generator `--check`, and `git diff --check`: passed.
- Real gateway startup on port 18084 with memory coordination and disposable Docker PostgreSQL 18: health and administrator login passed; routing groups, billing plans and dashboard summary returned HTTP 200. Public API-Key routing selection persisted; private selection was rejected without changing the previous group.
- Final PostgreSQL-backed `aether-data` suite: 419 passed, 1 ignored (including snapshot roundtrip and both managed-process shutdown regressions).
- Final gateway library/binaries/architecture/security suite: 5,860 passed, 3 ignored. This includes the repaired Grok provenance invariant and both cross-node PostgreSQL authentication regressions.
- Integration tooling plus real Responses WebSocket end-to-end suite: 31 passed, including all 13 WebSocket cases. Docker adapters supplied real isolated PostgreSQL for the managed-server fixtures.
- Real browser: login, dashboard, unified provider workspace, cost analysis and user analysis rendered. Selected a public group with multiplier 0.5, changed it through the UI to 0.75, and read back 0.75 via API. Desktop/mobile screenshots captured; no application JavaScript errors. Stripe's external script was intentionally blocked by the localhost-only browser allowlist; payment checkout was not exercised.

## Regression root causes

- Change propagation: new upstream CLI-profile fields were absent from a local full struct literal. All-target compilation caught this; fixture fields now match the production contract.
- Fixture assumption: upstream dashboard restore tests assumed every test URL ended with `/postgres`; local external-server mode uses unique database names. Added fixture-owned sibling databases with URL path rewriting and cleanup. The real PostgreSQL export/restore regression passes; the fixture contract is recorded in the data spec.
- Token semantics: upstream Grok provenance test added reasoning tokens twice to total usage, conflicting with the local corrected usage contract. Preserved production arithmetic and asserted `total = input + output` for sync/stream; reasoning is already included in output.
- Local PostgreSQL binaries and cargo-nextest are unavailable. Used Cargo test equivalents and temporary Docker-backed initdb/postgres/pg_ctl adapters; no production database or service was modified.
