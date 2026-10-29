# CI-Gated v0.7.51 Release

## Goal

Publish the next Aether patch tag only after GitHub Rust CI succeeds for the exact release commit.

## Confirmed blockers

The pushed `99362bb83f20918451591423efee9f0213753529` failed Rust CI run `40800632381`:

- Workspace Rest Clippy: `antigravity/tool_history.rs:676` has `needless_range_loop` in a test.
- Gateway integration architecture guards: new planner references bypass the existing format/transport root seams (`planner/antigravity.rs`, candidate materialization/source, decision input, normalize Chat/Responses).
- Integration WebSocket scenario `provider_quota_exhaustion_transparently_retries_onto_another_key`: quota feedback yields `responses_provider_unavailable` instead of selecting the eligible sibling account.

The complete local architecture run also exposed an obsolete Compact alias in a Codex discovery bypass test. Remove that retired case, retaining current Compact coverage and the unchanged format guard.

## Requirements

- Repair these concrete blockers without bypassing warnings, architecture guards, model-discovery admission, or the failing behavior assertions.
- Preserve account-scoped fetched model evidence, native-format bypass, quota exclusion/recovery and existing transport ownership.
- Verify relevant local behavior and complete CI on the exact release SHA.
- Follow existing annotated stable-tag conventions: latest `v0.7.50`, next `v0.7.51`. Never move or force an existing tag.
- Push authorized fixes, then publish the tag only after CI success; inspect automatic release workflow.

## Acceptance criteria

- A1: Workspace Clippy and Gateway architecture integration targets pass unchanged guards.
- A2: Real loopback WebSocket quota failover completes on the eligible second account with exact completion/count assertions.
- A3: GitHub Rust CI, including its aggregate `check`, succeeds for the SHA tagged `v0.7.51`.
- A4: The annotated `v0.7.51` is pushed to origin and resolves to that successful SHA; release automation is observed.

## Out of scope

Production deployment, new retry policies, model fallbacks, global static catalogs, changes to GitHub Pages policy, or weakening CI checks.
