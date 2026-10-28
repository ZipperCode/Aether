# Implementation

1. Implement transport thinking level decoding and suffix parsing, private-envelope normalization, and targeted tests in the existing Antigravity owner.
2. Implement the borrowed account-model resolver in provider-pool using authoritative-map helpers; cover disabled/missing tiers, no-effort and exact family boundaries.
3. Integrate request-level effort through gateway preselection, runtime eligibility, lazy materialization and Pool/sticky member selection. Resolve before quota/discovery checks and preserve administrator restrictions. Extend or bypass final candidate caches where effort-sensitive; retain source-row caching.
4. Add gateway behavior regressions using existing HTTP/runtime fixtures. Exercise native Gemini and converted Chat/Responses; account rotation, selected-model attribution and quota isolation; explicit suffix precedence and xhigh/max normalization.
5. After all code edits, run once: `cargo fmt --all --check`, targeted transport/pool tests, affected gateway tests and gateway Clippy. Use existing CI command environment; do not run formatters/checks while implementers are active.
6. Run actual gateway HTTP smoke with an isolated local upstream, verifying upstream envelope plus client-visible completion for alternating effort levels. No production requests or credentials.
7. Update the existing account-model/quota contract after successful smoke, remove temporary smoke scaffolding, isolate this task's changes and commit/push using repository policy. Do not create release tags for this task.

## Review Gates

- No variant may be selected from a different account or model family.
- No effort resolution after a quota snapshot has already rejected the default tier.
- No fallback model lists, new persistence, provider-wide cache of selected tier, or unrelated API behavior changes.
- Tests assert consumer-visible behavior, not source text or wiring.
