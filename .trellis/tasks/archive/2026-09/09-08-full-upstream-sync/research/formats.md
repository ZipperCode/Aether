# Formats merge receipt

## Scope and ownership

- Worktree: `C:/Users/Zipper/AppData/Local/Temp/aether-full-upstream-20260908`.
- Local parent: `b0ad8ff7f7bdb1888abac8c06f47f61e76761431`.
- Upstream parent: `c7e403b410139d12a6189dda9c2bdf0c7c80782e`.
- Merge base: `7892aa94853461c1e634f7a5babbb1280128720f`.
- Owned product scope: `crates/aether-ai/formats/**`; no other product files edited.
- No Cargo/lock edits, Cargo builds/tests, service or database operations, main-checkout writes, commits, or nested agents.

## Manually resolved conflict paths

All paths below are relative to `crates/aether-ai/formats/src/`:

1. `formats/gemini/generate_content/request.rs`
2. `formats/gemini/generate_content/response.rs`
3. `formats/gemini/generate_content/stream.rs`
4. `formats/openai/chat/stream.rs`
5. `formats/openai/request_contract.rs`
6. `formats/openai/responses/codex.rs`
7. `formats/openai/responses/mod.rs`
8. `formats/registry.rs`
9. `formats/shared/mod.rs`
10. `formats/shared/model_directives.rs`
11. `formats/shared/standard_matrix.rs`
12. `formats/shared/stream_core/format_matrix.rs`
13. `formats/shared/sync_products.rs`
14. `formats/shared/sync_to_stream.rs`
15. `protocol/canonical.rs`
16. `protocol/stream.rs`

Several paths resolve to unchanged local content after keeping Chinese contract comments and deduplicating semantically identical upstream tests. They were resolved individually, not by selecting one merge side wholesale.

## Semantic integration

- Retained local Gemini mixed-tool model gates, function-call signatures, tool result pairing, strict cross-format material-field auditing, immediate non-empty thought deltas, all five tool-terminal error codes, and accumulated Responses failure emission.
- Adopted upstream signature-only `MAX_TOKENS` success with positive reasoning usage in synchronous/aggregated output. Empty text plus signature is now recognized by the shared response-part audit as a non-visible control part; non-empty unmarked thought signatures remain rejected. The stream regression now uses a real Responses target and rejects visible text/reasoning or unknown-event output.
- Retained local official OpenAI/Codex function-call ID cleanup and unchanged `call_id` pairing. Adopted upstream deterministic `msg_aether_` generation on generated/conversion responses and official ordinary Responses message replay only. Non-official native Responses IDs remain unchanged; Compact and compaction-trigger operations retain strict ID stripping. The existing shared helper owns the official boundary, including gateway callers that run before the finalizer. Its return value counts changed items, including normalized message IDs.
- Carrier cleanup now calls the existing full decoder, removing only valid Aether Gemini carriers. Prefix collisions, invalid direction/Base64, real provider ciphertext, and valid provider `rs_` references remain unchanged. Existing synthetic-reference and DeepSeek replay-policy boundaries remain intact.
- Adopted Codex client version/User-Agent `0.153.4`, preserving local parsed member identity, persisted fingerprint, FedRAMP, and backend-scoped identity behavior.
- Adopted upstream format improvements including image generation/edit streaming, stable generated message IDs, canonical Debug protection, and the 64 MiB sync-report body decoder. The local complete-JSON capture recovery helper reuses that same bounded decoder rather than allocating through a parallel direct Base64 path.
- Removed 33 duplicate test definitions produced by automatic merging. Thirty-two bodies matched after comment/indent normalization; the remaining body differed only between an expected `"response"` literal and the fixture's identical `object` value. The first, locally documented body and all assertions were retained.
- Native unknown-field passthrough, precommit embedded-error handling, exact unchanged-body selection, and logical turn identity reside primarily in transport/gateway scopes; no new runtime canonical same-format route or public signature was introduced here.

## Regression evidence for the integration/check phase

New or strengthened:

- `reasoning_replay_removes_only_valid_aether_carriers`
- `official_message_normalization_is_idempotent_and_compact_stays_strict`
- `gemini_provider_state_preserves_signature_only_reasoning_terminal`
- `finalization_strips_invalid_official_openai_input_item_ids`
- `strips_invalid_typed_input_item_ids_without_breaking_tool_pairing`
- `codex_responses_targets_strip_foreign_typed_item_ids`

Retained representative regression names:

- `finalization_preserves_private_responses_item_ids`
- `input_item_id_sanitizer_is_scoped_to_official_openai_responses_targets`
- `gemini_tool_signature_carrier_roundtrips_direction_and_exact_value`
- `gemini_tool_signature_carrier_rejects_nested_and_oversized_values`
- `strips_gemini_signature_carriers_before_openai_replay`
- `streams_gemini_thought_text_to_openai_responses_immediately`
- `gemini_provider_state_emits_terminal_error_for_malformed_function_call`
- `transforms_malformed_gemini_function_call_to_responses_failed`
- `terminal_observer_marks_malformed_gemini_function_call_as_failure`
- `aggregates_authoritative_provider_opaque_reasoning_item_without_mutation`
- `codex_auth_identity_parses_member_claims_and_persisted_fingerprint`
- `codex_client_user_agent_matches_originator_and_version`

## Actual checks

- Ran `rustfmt --edition 2021 --config skip_children=true` only on the 16 owned conflict files.
- Re-ran the same command with `--check`: passed.
- `git -c core.safecrlf=false diff --check -- crates/aether-ai/formats`: passed.
- Scoped conflict-marker search: no remaining marker lines.
- Scoped duplicate test-name scan: zero duplicates in the 16 resolved files.
- Rustfmt parsed the owned files successfully; this is not a Cargo type-check or test execution claim.
- Compilation and focused regression execution are intentionally deferred until shared roots stop changing and the integrator/check phase owns validation.

## Cross-scope requests sent to Root

1. `apps/aether-gateway/src/ai_serving/planner/standard/normalize/tests.rs::local_codex_responses_wrapper_strips_invalid_typed_item_ids`: update only the old missing-message-ID assertion to the stable `openai_responses_message_item_id("item_e19637e60faa53da843e731c", 0)` expectation. Keep function-call ID removal and `call_id` pairing assertions.
2. `apps/aether-gateway/src/ai_serving/planner/passthrough/provider/family/request.rs`: the compatibility-edit detail currently says `stripped ... input item id field(s)`; describe normalization/cleanup because the shared helper now counts both official message rewrites and invalid function-reference removal.

No task-owned debug artifacts or processes were created. This evidence file is the only new non-product file.
