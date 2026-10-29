# Implementation

1. Backend slice: correct cursor group reason, make marked summary persistence keyless, update affected behavior tests and add a mixed-group real HTTP regression using existing fixtures. Preserve real-key skips and request diagnostics.
2. Frontend slice: recognize marked pool summaries, render group diagnosis/counted reasons, suppress misleading key metadata, and add focused timeline behavior regressions including historical summary rows.
3. Integrate; run targeted failing-before/passing-after reproduction, relevant Rust/frontend checks, and isolated live UI/HTTP smoke. Agents skip build/lint/test/format while implementation is in flight; parent runs checks after integration.
4. Independently review backend/frontend changes, resolve verified findings, and update the existing observability contract with the ownership rule after smoke proof.
   Review found ImageDecision omits request diagnostics; extend the existing persisted-candidate capacity fallback to require nonempty, positive integer member counts whose reasons are all capacity codes. Add a real image HTTP pure-capacity 429 / mixed-reason 503 regression and boundary coverage before rerunning backend validation.
5. Commit only this task's paths, push master to origin as explicitly requested, and observe GitHub Actions for the exact pushed commit. Resolve task-induced failures without weakening unrelated checks or changing deployment state.

## Validation entry points
- Existing gateway pool HTTP regressions in `src/tests/ai_execute/stream/pool_failover.rs` and cursor/materialization tests.
- Image HTTP exhaustion regression and `executor/outcome.rs` persisted capacity classification boundary cases.
- `cargo fmt --all --check`, targeted `cargo nextest run -p aether-gateway` with CI-equivalent environment, gateway Clippy.
- Frontend targeted Vitest suite, `npm run type-check`, `npm run build`, live browser timeline.
- `gh run list --commit <sha>` then wait for each applicable exact-commit workflow and inspect failures.

## Risks
Group summaries and concrete member rows share the candidate DTO. Scope recognition must use the explicit group payload, never infer from status or blank keys. No scheduler or HTTP error classification changes.
