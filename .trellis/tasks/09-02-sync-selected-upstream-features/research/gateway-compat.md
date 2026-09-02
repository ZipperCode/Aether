# Research: gateway/provider compatibility fixes

- Query: Selectively integrate R3 gateway/provider compatibility fixes from `fawney19/Aether` into fork baseline `50c96d060`, identify duplicate/branch relationships and true prerequisites, and preserve fork-specific Responses and scheduling behavior.
- Scope: internal
- Date: 2026-09-02

## Findings

### Conclusion

Select these six semantic commits:

1. `a39048eccea368a49e7a36dafb673bef55271046` — stable Codex logical-turn identity.
2. `3e540ce589648f24823e7b84e637b1e2aa4ce33e` — mandatory facade correction for `a39048e`.
3. `d07dc863761533a6b66540344943f7bfe9adca3f` — mandatory final form that generalizes convergence from OAuth-only to ordinary Codex auth channels.
4. `633363e190415943c37792946c4a63acefcf3408` — Pool saturation observability/client status plus Gemini precommit malformed-call detection.
5. `88d2b002be8f5147a61c76014b0ff55c3998bfcd` — ignore `ping` in Responses-to-standard stream conversion.
6. `7ae984df4ba66387d6aba548c68ad05499781c35` — detect DeepSeek behind a custom relay by final provider model.

Do **not** select `e2154629caf89359e3f594acc5c746106c9a7983`: it and `7ae984df4` have the identical stable patch id `64899ead1e38210a9bb455a6024a204aecd76bc4`. `7ae984df4` is the copy merged directly by upstream PR #776 and is the canonical choice.

True prerequisite outside this list: apply the R2 concurrency/cache-affinity commit `9631b229b3ac07d5ecccb7990268717a72d086a2` before `633363e19`. That commit creates `ProviderPoolInFlightAdmission`, `acquire_provider_pool_execution_guard`, Key-level permits, and the HTTP/stream/WebSocket saturation branches which `633363e19` augments. The other ancestors visible in Git history (Antigravity UI/quota commits, nightly CI, and formatting) are not R3 semantic prerequisites.

Recommended linear integration order, matching upstream topology where it matters:

```text
R2 9631b229b (and the rest of the selected R2 batch)
  -> a39048ecc -> 3e540ce58 -> d07dc8637
  -> 633363e19 -> cargo fmt equivalent of 3d87bbf23
  -> 88d2b002b -> 7ae984df4
```

`88d2b002b` is a Git descendant of the merge containing `633363e19`, and `7ae984df4` is later on main, but both patches are semantically independent and passed `git apply --check` directly against `50c96d060`. Keep the order above to minimize cross-batch conflicts. Apply R1 routing commit `415b2da81` only after the R3 edits to `decision_input.rs`, matching upstream order.

### Commit and branch relationships

| Commit | Relationship | Decision |
| --- | --- | --- |
| `a39048ecc` | Feature branch root; merged by upstream merge `7fb8d5fc0` (PR #771). Its parent contains earlier R2 work, but the identity feature itself carries its new contracts. | Select. |
| `3e540ce58` | Direct child of `a39048ecc`; moves the new context type import/export through the gateway transport facade. | Select immediately after `a39048ecc`; do not squash away the facade result. |
| `d07dc8637` | Descends from the merge of `a39048ecc` + `3e540ce58`; merged by `715f2773c` (PR #773). Its direct parent also contains unrelated nightly CI. | Select; exclude nightly CI parent `ef7caa40e`. |
| `633363e19` | Developed on the `zhefox/main` side branch and later merged by `5a69cfe40` (PR #772). | Select after R2 `9631b229b`; semantically merge the conflicted stream files. |
| `3d87bbf23` | Formatting-only child of `633363e19` (13 insertions/22 deletions, no behavior). | Do not cherry-pick; running `cargo fmt --all` reproduces its required effect. |
| `88d2b002b` | Separate branch merged by `0bfd48b9d` (PR #774); patch touches only the Responses parser and its conversion regression. | Select. No semantic dependency on `633363e19`. |
| `e2154629c` | DeepSeek fix on the `zhefox/main` branch, parented by formatting commit `3d87bbf23`. | Skip as duplicate. |
| `7ae984df4` | Byte-equivalent DeepSeek patch rebased/cherry-picked onto upstream main and merged by `24bf92a8b` (PR #776). | Select this canonical copy only. |

### Codex identity across HTTP retry, replan, Responses WebSocket, and Live WebSocket

The baseline already converges installation/session/thread headers, but creates a fresh UUIDv7 turn and timestamp every time `apply_codex_oauth_fingerprint_convergence` is called (`crates/aether-provider/transport/src/codex_fingerprint.rs:37-119`). A retry or replan can therefore look like a new turn.

The selected chain fixes the ownership boundary:

- `CodexFingerprintContextSlot(Arc<OnceLock<_>>)` captures request signals exactly once across cloned HTTP `Parts`; the proxy installs it before planning and `attach_routing_policy_to_local_requested_model_input` resolves it into `LocalRequestedModelDecisionInput` (`upstream/main:apps/aether-gateway/src/ai_serving/codex_context.rs:11-101`, `upstream/main:apps/aether-gateway/src/handlers/proxy/mod.rs:1100-1103`, `upstream/main:apps/aether-gateway/src/ai_serving/planner/decision_input.rs:474-503`).
- The immutable context carries original turn/session/thread/prompt-cache signals and a single turn start time. Provider finalization derives a stable UUIDv7 turn from that context and updates `decision.prompt_cache_key` after namespacing (`upstream/main:crates/aether-provider/transport/src/codex_fingerprint.rs:192-242`, `upstream/main:apps/aether-gateway/src/ai_serving/planner/decision_input.rs:370-398`). It namespaces a prompt-cache key only when the original signal and the final provider body both still contain one, so a routing/body rule cannot have a deleted key resurrected.
- Responses WebSocket creates one context for each logical `response.create`, stores it on `LogicalTurn`, carries it through pinned continuation, same-socket replan, and physical rebind, and restores it before a quota retry (`upstream/main:apps/aether-gateway/src/handlers/proxy/websocket/responses/client.rs:248-265,456-485,780-784,1008-1012,1093-1098`, `upstream/main:apps/aether-gateway/src/handlers/proxy/websocket/responses/turn_state.rs:15-67`, `upstream/main:apps/aether-gateway/src/handlers/proxy/websocket/responses/quota.rs:147-164`).
- The first Responses WebSocket turn gets the same treatment during bootstrap (`upstream/main:apps/aether-gateway/src/handlers/proxy/websocket/responses/session.rs:447-460,961-966`).
- Codex Live planning stores the context on `PlannedLiveCandidate` and restores it when building the later admission attempt, so the plan/admission split does not mint a second identity (`upstream/main:apps/aether-gateway/src/handlers/proxy/websocket/live/planner.rs:48-60,233-245,341-350,562-578`).
- OAuth exchange/refresh persists an account-member fingerprint independent of access-token rotation; fallback is account/member, then member, then account, then Key id (`upstream/main:crates/aether-oauth/src/provider/providers/generic.rs:20-47,401-464,527-607`, `upstream/main:crates/aether-provider/transport/src/codex_fingerprint.rs:257-290`). `d07dc8637` removes the OAuth-only guard, while still excluding Agent Identity and compact requests (`upstream/main:crates/aether-provider/transport/src/codex_fingerprint.rs:121-181`).

Semantic merge requirement: replace baseline `original_client_session_id` with `codex_fingerprint_context`, but preserve the fork's `endpoint_capability_client_api_format`, `endpoint_capability_require_streaming`, and `endpoint_capability_request_operation` fields, setters, filtering, and every test literal (`apps/aether-gateway/src/ai_serving/planner/decision_input.rs:49-145,515-530`). The same literal update is needed in `standard/family/payload.rs` and `standard/openai/chat/decision/request.rs`. Do not resolve these conflicts by taking the upstream file wholesale.

### Pool saturation behavior and local scheduling contracts

`9631b229b` already makes a saturated Key a candidate-scoped miss and tries another candidate on HTTP sync/stream paths; its per-turn Responses WebSocket admission returns a direct 429. The remaining defect is observability/final classification: saturation at HTTP execution time was written only to a request-candidate status row and was absent from the in-memory runtime-miss diagnostic, while a Pool group collapsed multiple internal skip reasons to one dominant reason.

`633363e19` fixes that by:

- recording `provider_key_concurrency_limit_reached` in the runtime-miss diagnostic at both sync and stream admission before returning `Ok(None)` (`upstream/main:apps/aether-gateway/src/execution_runtime/stream/execution.rs:3776-3809`, `upstream/main:apps/aether-gateway/src/execution_runtime/sync/execution.rs:1999-2030`);
- persisting that reason into the request-candidate `skip_reason` field (`upstream/main:apps/aether-gateway/src/request_candidate_runtime.rs:360-470`);
- recording every distinct Pool-group skip reason instead of only the dominant one (`upstream/main:apps/aether-gateway/src/dispatch/pool_scheduler.rs:590-623`);
- returning HTTP 429 and a capacity-specific message only when every skipped reason is either `provider_key_concurrency_limit_reached` or `key_rpm_exhausted`; mixed/non-capacity misses stay 503 (`upstream/main:apps/aether-gateway/src/handlers/proxy/mod.rs:108-114,1915-1931,2377-2399`).

Preserve the fork's balance and runtime-quota semantics. In particular, `key_balance_below_minimum`, `pool_balance_below_minimum`, runtime quota hard blocks, strong Key reads, sticky fallback, and cache invalidation must remain separate facts. The `633363e19` all-reasons change is compatible and important: any balance/quota reason in a mixed exhaustion prevents it from being mislabeled as transient capacity. Keep the current Pool scheduling code around `provider_pool_key_balance_below_minimum` and runtime quota fences; merge only the targeted diagnostic changes. Do not broaden `local_request_candidate_skip_reason` beyond the upstream concurrency reason without a separate contract decision.

### Gemini malformed-call precommit detection

`633363e19` adds `FirstGeminiSemanticEvent { max_bytes, max_wait }` to the shared commit gate. For Gemini SSE, thought-only/signature-only frames remain pending, real text/function calls commit, and terminal reasons such as `MALFORMED_FUNCTION_CALL`, `UNEXPECTED_TOOL_CALL`, `TOO_MANY_TOOL_CALLS`, `MISSING_THOUGHT_SIGNATURE`, or `MALFORMED_RESPONSE` become a synthetic upstream 502 before downstream success is committed (`upstream/main:apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs:48-64,91-112,162-203,365-470`). The existing `handle_prefetch_provider_private_stream_error` then owns configured candidate failover. The terminal observer also records unsupported Gemini finish reasons as parser errors instead of treating them as successful usage terminals (`upstream/main:crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs:349-386`).

This is not duplicated by local commit `4b0fca3d6`: that one only changed Clippy formatting in `gemini_stream_error_status` after downstream terminal error construction. Baseline has no `FirstGeminiSemanticEvent` or precommit malformed-call classifier.

Critical semantic merge: baseline's same-format Responses safeguard must survive. Keep the `openai:responses` same-format SSE branch returning `FirstClassifiedBody` (`apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs:44-59`) while adding the Gemini branch. In `execution.rs`, retain the baseline special `EmbeddedError` path that sends same-format Responses errors through `handle_prefetch_provider_private_stream_error` (`apps/aether-gateway/src/execution_runtime/stream/execution.rs:6931-7051`), and add Gemini to the semantic-gate path that bypasses the generic prefetched-body inspector. Never resolve this conflict by replacing the files with upstream versions, because upstream main does not contain the fork's bare-Responses-error fix.

### Responses `ping` filtering

`88d2b002b` changes `OpenAIResponsesProviderState` from ignoring only `keepalive` to ignoring `keepalive | ping` (`upstream/main:crates/aether-ai/formats/src/formats/openai/chat/stream.rs:1843`). This state is used by `StreamingStandardFormatMatrix`, so the change affects conversion/normalization paths and prevents `ping` from becoming `unsupported_stream_event`. It does not rewrite native same-format Responses SSE, which continues to preserve unknown bytes/events under the fork's relay contract. The patch applies cleanly to `50c96d060` and has no true prerequisite.

### DeepSeek custom relay detection

Baseline already has a strict URL parser which accepts explicit DeepSeek provider types or valid HTTP(S)/WS(S) hosts under `deepseek.com`, including bare authority/path input, and rejects userinfo/query/path spoofing (`apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs:3-79`). Preserve it.

`7ae984df4` adds final provider-model evidence: after taking the last `/` or `:` segment, only `deepseek`, `deepseek-*`, and `deepseek_*` match. Compatibility applies when provider type/valid host **or** this model evidence matches (`upstream/main:apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs:18-50`). It threads the final mapped provider model through same-format, standard, Chat, Responses, WebSocket, and provider-test finalization call sites. This is essential for custom relays whose URL is unrelated to DeepSeek, while `not-deepseek-compatible` remains untouched (`upstream/main:apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs:474-518`).

`handlers/admin/provider/query/models/model_test.rs` overlaps the fork's local model-capability detection. Preserve the local capability resolution and add only the new `request_model`/mapped-model argument to `openai_responses_reasoning_replay_policy`; do not take the upstream file wholesale.

### Risky overlapping files and symbols

- `apps/aether-gateway/src/ai_serving/planner/decision_input.rs`: highest cross-batch risk. R3 replaces session-id-only state, R1 later changes routing policy, and the fork owns endpoint capability fields. Preserve all three concerns.
- `apps/aether-gateway/src/ai_serving/codex_context.rs`, `client_session_affinity.rs`, `handlers/proxy/websocket/{live,responses}/**`: one immutable context must survive every attempt transition; partial wiring compiles but recreates identity on a missed path.
- `crates/aether-provider/transport/src/codex_fingerprint.rs`, `crates/aether-oauth/src/provider/providers/generic.rs`, `crates/aether-ai/formats/src/formats/openai/responses/codex.rs`: account/member fingerprint persistence, auth parsing, prompt-cache behavior, Agent Identity exclusion, and ordinary-auth generalization are one contract.
- `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs` and `stream/execution.rs`: combine Gemini semantic precommit with the fork's Responses first-body classification/failover. This is the main manual conflict.
- `apps/aether-gateway/src/{dispatch/pool_scheduler.rs,request_candidate_runtime.rs,handlers/proxy/mod.rs,execution_runtime/sync/execution.rs,executor/outcome.rs}`: merge capacity diagnostics without weakening balance or runtime-quota eligibility.
- `crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs`: retain local Gemini/Anthropic conversion improvements while adding terminal-observer validation; `88d2b002b` then applies its one-line parser change.
- `apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs` and its seven callers: use final mapped model and retain the strict URL host parser.
- `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs`: preserve fork model-capability logic while updating the DeepSeek replay-policy signature.

### Smallest validation after integration

Run formatting first because `3d87bbf23` is deliberately not selected:

```powershell
cargo fmt --all
cargo fmt --all --check
cargo check -p aether-provider-transport -p aether-oauth -p aether-ai-formats
cargo check -p aether-gateway --lib
```

Then one focused regression per R3 behavior plus two fork-preservation checks:

```powershell
cargo test -p aether-provider-transport codex_fingerprint::tests --lib
cargo test -p aether-oauth codex_persisted_fingerprint_is_member_scoped_and_token_independent --lib
cargo test -p aether-gateway ai_serving::codex_context::tests --lib
cargo test -p aether-gateway codex_fingerprint_convergence_runs_at_every_provider_routing_success_exit --lib
cargo test -p aether-gateway malformed_antigravity_function_call_retries_before_stream_commit --lib
cargo test -p aether-gateway provider_key_capacity_requires_every_skip_reason_to_be_capacity_related --lib
cargo test -p aether-gateway saturated_provider_key_snapshot_persists_capacity_skip_reason --lib
cargo test -p aether-gateway pool_key_cursor_records_runtime_miss_when_exhausted_without_returning_key --lib
cargo test -p aether-ai-formats ignores_openai_responses_keepalive_events_for_chat_clients --lib
cargo test -p aether-gateway custom_relay_ --lib
cargo test -p aether-gateway same_format_responses_prefetch_retries_bare_error_before_committing_success --lib
cargo test -p aether-gateway pool_key_cursor_skips_low_balance_sticky_key_and_falls_back_once --lib
```

If time permits, add `cargo test -p aether-ai-formats terminal_observer_marks_malformed_gemini_function_call_as_failure --lib`; it isolates the usage-terminal half of `633363e19`. Do not substitute broad workspace tests for the focused failures first.

### Integration requests

1. R2 integrator must land `9631b229b` before this batch, or explicitly hand off the final `ProviderPoolInFlightAdmission` API and Key-limit semantics.
2. The R3 integrator should use semantic conflict resolution for `decision_input.rs`, `commit_policy.rs`, and `execution.rs`; never accept an upstream whole-file resolution.
3. R1 integrator should rebase its `415b2da81` work on the final R3 `decision_input.rs` and retain `codex_fingerprint_context` plus the fork endpoint-capability fields.
4. R4/formats integrator should retain the `88d2b002b` conversion-only `ping` behavior and the `633363e19` terminal-observer error, without changing native same-format Responses passthrough.
5. Select only `7ae984df4`; record `e2154629c` as duplicate evidence, not an additional cherry-pick.

## Files Found

- `apps/aether-gateway/src/ai_serving/codex_context.rs` — new logical-turn context owner shared by HTTP and WebSocket planning.
- `apps/aether-gateway/src/ai_serving/planner/decision_input.rs` — terminal provider request convergence and the main R1/R3/local-capability overlap.
- `apps/aether-gateway/src/client_session_affinity.rs` — normalized Codex session/thread/turn/prompt-cache signal precedence.
- `apps/aether-gateway/src/handlers/proxy/websocket/live/planner.rs` — carries context across Codex Live planning and admission.
- `apps/aether-gateway/src/handlers/proxy/websocket/responses/{client,session,quota,turn_state}.rs` — carries one context through logical turn, replan/rebind, and quota retry.
- `crates/aether-provider/transport/src/codex_fingerprint.rs` — derives converged headers/body fields from immutable context and stable account identity.
- `crates/aether-oauth/src/provider/providers/generic.rs` — persists stable Codex account-member fingerprint across token refresh.
- `apps/aether-gateway/src/execution_runtime/stream/{commit_policy,execution}.rs` — shared precommit gate; local Responses and incoming Gemini behavior must coexist.
- `apps/aether-gateway/src/{dispatch/pool_scheduler,request_candidate_runtime,handlers/proxy/mod}.rs` — Pool exhaustion evidence and final 429/503 classification.
- `crates/aether-ai/formats/src/formats/{openai/chat/stream,shared/stream_core/format_matrix}.rs` — Responses ping filtering and terminal parser-error reporting.
- `apps/aether-gateway/src/ai_serving/planner/standard/deepseek.rs` — strict provider/host/model classifier and DeepSeek request compatibility.

## External References

- Repository comparison baseline: fork `50c96d060442fb1b612a27c587b91dec4f79a613`; inspected upstream `cae9aa4134b6bfd4b21dab0c535186232002ed34`.
- Upstream merge evidence is available locally in PR merge commits `7fb8d5fc0` (#771), `715f2773c` (#773), `5a69cfe40` (#772), `0bfd48b9d` (#774), and `24bf92a8b` (#776).
- No live web documentation was required; this research concerns repository-local behavior and exact Git objects.

## Related Specs

- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md` — native Responses preserves unknown events and classifies the first complete same-format body before downstream 2xx.
- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md` — low-balance is an independent fail-open eligibility fact with shared ordinary/Pool/sticky behavior.
- `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md` — runtime quota is a credential-fenced persistent hard block, separate from transient capacity.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` — relevant because identity and error facts cross planner, transport, WebSocket, persistence, and client boundaries.

## Caveats / Not Found

- `git apply --check` against the untouched baseline was clean for `88d2b002b` and `7ae984df4`; it reported expected conflicts for `a39048ecc`/`3e540ce58`/`d07dc8637` (fork endpoint-capability and facade overlap) and `633363e19` (missing R2 prerequisite plus fork stream changes). This is conflict evidence, not a post-integration build result.
- No Rust tests were run because the requested commits are not integrated and this agent is research-only. The commands above are the required post-merge validation set.
- The Codex HTTP spec still describes Responses WebSocket as separate product work, while the baseline already contains the WebSocket implementation. Do not use that stale sentence to omit the R3 WebSocket identity wiring; the executable same-format HTTP SSE rules remain authoritative for the stream conflict.
- The task directory is untracked in this isolated worktree as expected. No product file, index, ref, branch, or remote was changed.
