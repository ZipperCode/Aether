# Implementation sequence

1. Verify official CLI identity and map current public identity callers. Inspect all account query/cache branches and focused existing behavioral coverage. Reproduce the proven identity override and disabled-parser problems before editing (already demonstrated in the preceding investigation with current-source executable smoke).
2. Activate this task after recording final design. User explicitly approved the recommendation in the preceding investigation; no new product/scope decision is outstanding.
3. Delegate independent ownership slices:
   - Identity: provider transport identity/auth/envelope and required initialization/transport callers; migrate all affected callers and delete obsolete incidental pinning tests. No model catalog changes.
   - Discovery: admin all-account aggregation and obsolete provider cache removal, upstream-disabled parsing, and focused consumer-visible regression cases. Preserve account-local cache/permissions.
4. Parent integrates shared caller changes, reviews warnings/cache isolation, then runs checks once all edits settle. Workers must skip builds/tests/lint/formatters mid-flight.
5. Run focused `cargo test --locked --lib` Antigravity cases in `aether-provider-transport`, `aether-model-fetch`, `aether-provider-pool` and gateway admin query coverage, plus affected existing routing/account-isolation cases. Rust 1.95.0 is installed; cargo-nextest is unavailable, so use Cargo's built-in runner. Set `RUST_MIN_STACK=16777216` for gateway tests. Do not run mutating frontend lint.
6. Execute a temporary real loopback HTTP smoke with synthetic accounts: inspect request identity; verify first successful account does not hide later exclusive models, disabled models disappear, and selected-account catalogs remain isolated. Remove temporary source/binaries after successful proof.
7. Update existing relevant docs/spec sections, run `cargo fmt --all --check` and relevant final checks. If verification succeeds, isolate and commit only this task's changes with Chinese commit message. Never push without explicit user authorization.

## Risk gates

- No Google credential read/transmission; do not report a real Gemini 4 fetch unless actually observed with authorized credentials.
- No manual model supplementation or quota-group model synthesis.
- No provider union copied to an account's allowed_models.
- Avoid changes to unrelated user work or active task directories.
- If full gateway linkage fails due environment/resource constraints, finish reachable runtime smoke and state the exact verification gap; do not commit unverified changes.
