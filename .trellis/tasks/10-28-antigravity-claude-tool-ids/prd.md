# 补齐 Antigravity Claude 历史工具调用 ID

## Goal

Allow Gemini-native clients to replay tool history through Antigravity Claude without failing on missing tool-call identifiers.

## Requirements

- Repair missing tool-call and matching result IDs only at the Antigravity private Claude request boundary, for ordinary and already-enveloped input.
- Preserve explicit nonempty IDs, tool arguments/results, signatures, text, ordering, and original client/audit input.
- Pair repeated/parallel tool names by existing identities first and then occurrence order; keep generated IDs unique and deterministic for retries.
- Leave public Gemini, non-Claude Antigravity targets, and unrelated routing/retry behavior unchanged. No deployment, schema migration, or provider requests are authorized by this task.

## Acceptance Criteria

- [x] ID-less Gemini call/result pairs receive matching nonempty IDs in the final Claude envelope, including the 178-pair shape observed in request c09a882d.
- [x] Mixed existing/missing identities and repeated or parallel calls do not overwrite explicit IDs or associate a result with the wrong invocation.
- [x] Repeated normalization is idempotent and generated IDs do not collide with explicit IDs.
- [x] Existing inputs, payloads, signatures and out-of-scope Gemini/non-Claude histories remain unchanged.
- [x] Direct package regressions and an executable smoke using the original captured request succeed locally; report upstream-serving verification separately if not performed.

## Evidence and Scope

- Request c09a882d on 192.168.2.212 had 178 calls and 178 results without IDs in both captured client and provider histories; all 47 recorded 400 responses reported `messages.1.content.0.tool_use.id: Field required`.
- `crates/aether-provider/transport/src/antigravity/request.rs:219` owns private request normalization in both envelope branches. Public Gemini passthrough must not inherit this adaptation.
- Existing unrelated work in `crates/aether-provider/transport/src/conversion.rs` and `.trellis/tasks/10-28-codex-account-model-eligibility/` must remain untouched.
- This bounded repair is a lightweight PRD-only task. Retry policy changes and online rollout are out of scope.

## Verification

- `cargo nextest run -p aether-provider-transport`: 606 passed, 0 skipped.
- `cargo clippy -p aether-provider-transport --lib -- -D warnings`: passed.
- Task-owned Rust files passed scoped rustfmt checking. Workspace-wide formatting also reported unrelated parallel-work diffs, which were not modified by this task.
- Original c09a882d captured-body executable failed before repair and passed afterward: 178 unique matching call/result IDs; payload/signature preservation, idempotence and non-Claude isolation asserted. Throwaway runner and local body file removed after proof.
- Review found complementary one-sided identity ordering and later-result reservation bugs. Both were reproduced with failing regressions and fixed using chronological pending-call matching.
- No deployment, account requests, upstream generation replay, schema change or retry-policy change performed. 192.168.2.212 remains unchanged by this task.
