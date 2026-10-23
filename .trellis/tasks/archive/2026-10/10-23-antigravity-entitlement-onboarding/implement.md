# Implementation plan

1. Shared prerequisite: transport account parser/policy, migrate admin parser and all imports/tests, safe metadata allowlist.
2. Parallel independent ownership: (A) model-fetch discovery/entitlement request path and regression cases; (B) gateway request hydration + quota entitlement path/observation persistence; (C) explicit admin onboarding mutation + frontend action/status display, after route mapping and interface agreement. Parent integrates shared parser and affected contracts.
3. No mid-flight test/build/lint/format runs. After all edits, run cargo fmt --check (format touched Rust only if needed), focused crate tests and gateway regressions, frontend type-check and directly affected suite. Fix regressions within owning slice.
4. Run actual local mock HTTP discovery/entitlement fallback and initialization scenarios with throwaway runner or existing gateway local-memory harness; observe UI action/status in browser with local mock account data. Never call live Google using actual credentials.
5. Update existing relevant operational docs/specs to reflect implemented semantics; remove throwaway scaffolds after proof. Commit only this task's isolated files after successful verification, never push.

## Regression boundaries
- paid/current/no-entitlement order, duplicates and free exclusion; stop on authentication/throttling/transport error.
- success unknown clears old classification; discovery failure preserves tier and usable project; TTL suppresses repeated upstream discovery.
- model metadata and other namespaces survive enrichment.
- initialization idempotency, missing entitlement/default tier, pending LRO deadline and admin/provider/key validation.
- account display remains independent of pool scheduling, quota and signature behavior.
