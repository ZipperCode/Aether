# Implementation plan

1. Backend: add pure authoritative tier/project parser and regression cases (paid precedence, aliases, missing/unknown information). Integrate one best-effort discovery into quota refresh with proxy override; preserve existing project fallback and metadata. Allow the tier through safe API serialization; retain null quota snapshot plan_type to avoid changing scheduling.
2. Frontend: reuse account-type and badge conventions for Antigravity desktop/mobile rows and quota detail. Treat absent/unknown as unknown; keep other providers unchanged. Add focused behavior cases only where current coverage is suitable.
3. Integration owner runs checks once after workers finish: targeted Cargo tests using `cargo test -p aether-admin` and relevant `aether-gateway` filters, frontend type-check and relevant Vitest suites. Run actual local Vue UI with controlled local fixture responses, observe all three tiers and unknown in browser, including detail/mobile.
4. After smoke proof, update existing relevant documentation with authoritative tier source and refresh behavior, remove temporary fixtures, and commit isolated task changes. No push.

## Risks

- Successful unknown evidence must clear stale classification; failed lookup must not invent Free or break known-project quota refresh.
- Existing metadata merges must not overwrite the newly discovered project/tier with an older key snapshot.
- Account-type projection must remain display-only; avoid changing plan tier ranking.
- No real Google account credentials are available for upstream live acceptance; local HTTP fixtures must exercise the real gateway parsing/execution path where feasible.
