# Research: Gemini / Responses / Antigravity protocol compatibility

- Query: Research R4 upstream non-merge patches, duplicate/branch relationships, dependency order, fork equivalents, conflicts, request/response/stream and same-format contracts, tests, prerequisites, and local behavior to preserve.
- Scope: mixed (local fork worktree plus upstream commit/API evidence and official protocol documentation)
- Date: 2026-09-02
- Baselines supplied by dispatch: fork `50c96d060442fb1b612a27c587b91dec4f79a613`; upstream/main `cae9aa4134b6bfd4b21dab0c535186232002ed34`.

## Findings

### Conclusion

R4 is not already present in the fork. The fork has partial equivalents for Gemini ID-less call pairing, text-part thought signatures, Codex `additional_tools`, Responses compaction operation detection, and cross-format finalization, but it lacks the complete upstream behaviors. Replay the following behavior chain in order, resolving it into the fork rather than replacing fork-specific code wholesale:

1. `4da8c57fe332d65fd1c01979f205de424238cc87`
2. `8cdfa338e5fa132a21beef5f770b041bfa7f403c`
3. `c4b4dfa996dcd8ce8ff92cb425c6883a492aa90f`
4. `5687dad17717adc7c326650eb01dfd87681574ed`
5. `fa8e443f7b3ce3bc860586c67f05e97810a7a27a` (choose this one; do not also replay duplicate `5b6fce1a77d8260b151b4f08e3fef8b177a5c1ec`)
6. `64e57253310ae21bc470dea3ee69a5a42e1a0c37`
7. `f0b0064f3de581bf46dda15fcc487e0968dbc815`
8. `83098f98b6a4b5d2de751cd8c2bda879089ad7f0`
9. `5bcdcca7849598844262b39083bdbc89c1918cac`
10. `1bc2287baa0019f4968468b276336c17a1414976`
11. `9837ce119702934da1785d5c7d1fc83e69922258`
12. `36daba7a34d68c4dca27aa993ffca39bedbd321a`
13. `b35364d7fd9f31e8e95667f2a29f3f8c47972784`
14. `56395945c025c03331bb863b6095644180d62e5f`

This ordering preserves the actual topic-branch dependencies. In particular, apply `f0b0064f3` immediately followed by its Gemini-3 correction `83098f98b`; apply `9837ce119` immediately followed by corrective `36daba7a3`; and never stop on either intermediate behavior.

### Duplicate and branch relationships

- `4da8c57fe -> 8cdfa338e -> c4b4dfa99 -> dd2958a45 -> 5687dad17 -> fa8e443f7` is the original feature-branch line. `dd2958a458a582d77803a8e57c9aa3f23672dd7b` is unrelated admin/provider-settings work and is not a semantic prerequisite; exclude it.
- Upstream main separately merged through `8cdfa338e`, then through `5687dad17`, and created `5b6fce1a7` on the main-side parent as the same reasoning-summary change as `fa8e443f7`.
- The normalized `filename + patch` payloads for `fa8e443f7` and `5b6fce1a7` have the identical SHA-256 `08713c88495bd8e9440bb97c82b5fc78d5434f29fb10f74b6f23835793e393cd` (5,034 characters). They are patch duplicates, not complementary commits. `fa8e443f7` is preferred because it directly follows `5687dad17` in the selected feature line.
- `64e572533 -> f0b0064f3 -> 83098f98b -> 5bcdcca78` is the schema/mixed-tools/additional-tools topic sequence after main-side merges.
- `1bc2287ba -> 9837ce119 -> 36daba7a3` follows the merge that combined the earlier lines. `b35364d7f` is based on a main merge containing `36daba7a3`; `56395945c` is directly based on `b35364d7f`.
- Do not replay upstream merge commits. They combine unrelated features, including excluded scope.

### Exact behavior and dependency matrix

| Commit | Required behavior | Dependency / supersession | Tests added or materially changed upstream |
| --- | --- | --- | --- |
| [`4da8c57fe`](https://github.com/fawney19/Aether/commit/4da8c57fe332d65fd1c01979f205de424238cc87) | Preserve JSON-looking tool outputs as strings; merge adjacent Gemini-role messages and align parallel tool results before text; allow Responses `include` to degrade when targeting Chat; use production `cloudcode-pa` for Antigravity `loadCodeAssist`. | Assumes namespace-tool/cross-format infrastructure described below. Foundation for later Gemini request changes. | `pure_openai_chat_to_gemini_preserves_json_tool_output_as_string`; `pure_openai_responses_to_gemini_aligns_parallel_tool_outputs`; `pure_claude_to_gemini_aligns_parallel_tool_results_before_text`; pure/runtime `*_drops_include`; Antigravity production load test. |
| [`8cdfa338e`](https://github.com/fawney19/Aether/commit/8cdfa338e5fa132a21beef5f770b041bfa7f403c) | Generate collision-free IDs for ID-less Gemini function calls; pair responses by explicit `id`/`call_id`/`callId`, then name, then FIFO; preserve pairings through Chat, Responses, and Claude conversion. | Direct child of `4da8c57fe`. Fork has only a partial per-name implementation. | `pure_gemini_idless_parallel_tool_history_stays_paired_for_standard_targets`; four `gemini_request_*` pairing/collision tests. |
| [`c4b4dfa99`](https://github.com/fawney19/Aether/commit/c4b4dfa996dcd8ce8ff92cb425c6883a492aa90f) | Preserve Gemini function-call `thoughtSignature` through sync and stream conversions. Adds `ToolCallSignature`, Gemini ToolUse extension storage, and a bounded/directional Responses reasoning carrier so Responses history can round-trip back to Gemini. Synthetic Gemini history gets the upstream sentinel only on the first call part. | Direct child of `8cdfa338e`; requires existing Responses synthetic-reasoning and namespace infrastructure. | Gemini provider signature ordering; Responses emitter early/late signature ordering; carrier round-trip/oversize/nesting rejection; Responses↔Gemini signature round-trip and previous/next carrier tests. |
| [`5687dad17`](https://github.com/fawney19/Aether/commit/5687dad17717adc7c326650eb01dfd87681574ed) | Moves `encode_gemini_tool_signature_carrier` import into tests. | Compile/lint cleanup for `c4b4dfa99`; no runtime behavior. | No new test; makes the production import test-only. |
| [`fa8e443f7`](https://github.com/fawney19/Aether/commit/fa8e443f7b3ce3bc860586c67f05e97810a7a27a) | Degrade Responses `reasoning.summary` when targeting Chat while preserving `reasoning.effort`; continue to reject `reasoning.budget_tokens`. | Select instead of identical `5b6fce1a7`. | Pure/runtime summary drop; budget-token fail-closed regression. |
| [`64e572533`](https://github.com/fawney19/Aether/commit/64e57253310ae21bc470dea3ee69a5a42e1a0c37) | Convert general JSON Schema into Gemini's supported subset: resolve local `$ref`, map `oneOf` to `anyOf`, handle nullable unions, filter invalid enum values/unsupported keys, normalize int64 count limits, recurse through properties/items, and ensure object properties. | Foundation for mixed/custom tool conversion. Raw native Gemini parameters remain the raw path. | `canonical_tool_declaration_sanitizes_json_schema_for_gemini`. |
| [`f0b0064f3`](https://github.com/fawney19/Aether/commit/f0b0064f3de581bf46dda15fcc487e0968dbc815) | When cross-format conversion produces built-in plus function tools, set `toolConfig.includeServerSideToolInvocations=true`. | Intermediate implementation; must be immediately corrected by `83098f98b` because this version enables Gemini 2.5. | `openai_responses_builtin_and_function_tools_enable_gemini_server_invocations`. |
| [`83098f98b`](https://github.com/fawney19/Aether/commit/83098f98b6a4b5d2de751cd8c2bda879089ad7f0) | Gate mixed-tool conversion to Gemini 3, fail before transport for unsupported target models, keep camelCase on wire, and verify the direct execution body. | Direct child/superseding correction of `f0b0064f3`. | `mixed_builtin_and_function_tools_require_gemini_three`; `runtime_responses_to_gemini_rejects_mixed_tools_for_gemini_two`; `direct_sync_execution_runtime_preserves_gemini_tool_config_on_wire`; matrix/Antigravity assertions. |
| [`5bcdcca78`](https://github.com/fawney19/Aether/commit/5bcdcca7849598844262b39083bdbc89c1918cac) | For Responses→Chat only, consume a leading exact-shape developer `additional_tools` prefix, prepend its function/custom tools to request tools, and remove the pseudo-message. Unknown fields, later occurrences, or unrepresentable tools remain fail-closed. | Direct child of `83098f98b`; relies on existing custom/namespace tool conversion. | Four registry normalization/fail-closed tests plus gateway `maps_openai_responses_additional_tools_without_message_name`. |
| [`1bc2287ba`](https://github.com/fawney19/Aether/commit/1bc2287baa0019f4968468b276336c17a1414976) | Apply the mixed-tool flag on native Gemini same-format provider requests too and record a provider-compatibility edit. | Requires mixed-tool helper from `f0b0064f3/83098f98b`; follows the merge containing `5bcdcca78`. | `same_format_gemini_body_enables_mixed_tool_invocations`. |
| [`9837ce119`](https://github.com/fawney19/Aether/commit/9837ce119702934da1785d5c7d1fc83e69922258) | Intermediate Antigravity schema rewrite from `parameters` to `parametersJsonSchema`. | Direct child of `1bc2287ba`, but wire direction is immediately reversed by `36daba7a3`; never ship alone. | Extends existing real-agent envelope assertions. |
| [`36daba7a3`](https://github.com/fawney19/Aether/commit/36daba7a34d68c4dca27aa993ffca39bedbd321a) | Correct private Antigravity wire format to `parameters`; accept and normalize incoming camel/snake `parametersJsonSchema` aliases to it. | Syntactically depends on helper introduced by `9837ce119` and supersedes its direction. | `antigravity_envelope_normalizes_json_schema_parameter_spellings` plus corrected real-agent assertion. |
| [`b35364d7f`](https://github.com/fawney19/Aether/commit/b35364d7fd9f31e8e95667f2a29f3f8c47972784) | At the Antigravity private envelope boundary only, normalize public `googleSearch` / `google_search` to legacy `googleSearchRetrieval`. | Requires final Antigravity helper state from `36daba7a3`. Public Gemini requests must retain native spelling. | Existing real-agent and existing-envelope tests assert removal of public spellings and preservation of payload under the private spelling. |
| [`56395945c`](https://github.com/fawney19/Aether/commit/56395945c025c03331bb863b6095644180d62e5f) | Restrict a standard `openai:responses` request whose semantic operation is `compact` to Responses-format candidates; reject `compaction_trigger` conversion to Gemini. Preserve ordinary Responses cross-format candidates and the legacy compact format. | Requires current operation detection and candidate-source plumbing, already present in the fork. | `compaction_operation_excludes_non_responses_provider_formats`; `openai_responses_compaction_trigger_is_not_converted_to_gemini`. |

### Upstream prerequisites outside the selected set

No additional upstream patch needs replay, but two ancestry prerequisites must remain intact because later patches are written against them:

- [`e2b003af24581a3b07a69826a91b22071c7d8d12`](https://github.com/fawney19/Aether/commit/e2b003af24581a3b07a69826a91b22071c7d8d12), Responses namespace tools through Chat. The fork already has the equivalent `NamespaceToolAliases` flow and its request/stream/sync tests: `formats/openai/chat/request.rs:776`, `formats/registry.rs:4342`, `formats/shared/stream_core/format_matrix.rs:975`, and `formats/shared/sync_to_stream.rs:1450`. Do not replay it.
- [`9d9892be6a5cdfcbcb9f8f8fc0815b9d92afc6eb`](https://github.com/fawney19/Aether/commit/9d9892be6a5cdfcbcb9f8f8fc0815b9d92afc6eb), cross-format sync JSON finalization. The fork has the generalized finalization functions at `formats/shared/sync_products.rs:55-554` and gateway regressions at `ai_serving/finalize/tests_sync.rs:1402` and `:1710`. Do not replay it.
- `c4b4dfa99` needs Base64, but `crates/aether-ai/formats/Cargo.toml:11` already uses the workspace dependency and root `Cargo.toml:107` pins `base64 = "0.22"`; no dependency commit is needed.
- `56395945c` needs `openai_responses_request_operation` and `OPENAI_RESPONSES_OPERATION_COMPACT`; both already exist at `formats/openai/responses/mod.rs:197-222`.
- Merge commits and `dd2958a458a582d77803a8e57c9aa3f23672dd7b` are ancestry only, not prerequisites. Exclude them.

### Fork equivalents and gaps

- **ID-less Gemini history is partial, not complete.** `protocol/canonical.rs:1139-1215` already assigns per-message/part IDs and pairs ID-less results by per-name FIFO; `:1218-1254` consumes explicit IDs. It recognizes only `id`, does not reserve all explicit IDs before generation, and has no global FIFO fallback for nameless results. Integrate `8cdfa338e` semantics into this fork implementation rather than keeping both algorithms.
- **Thought signatures are only preserved for thought/text parts.** `protocol/canonical.rs:1369-1388` and Gemini stream parsing at `generate_content/stream.rs:216-267` retain reasoning signatures, but function-call parts lose them. `CanonicalStreamEvent` has no tool-call signature variant (`protocol/stream.rs:42-78`). `c4b4dfa99` is still required.
- **Tool outputs still parse JSON-looking strings.** OpenAI Chat tool messages parse string content at `protocol/canonical.rs:2405-2426`; Responses function outputs do the same at `:2751-2788` and `:3200-3231`. This violates `4da8c57fe`'s string-preservation requirement.
- **Responses→Chat still rejects `include` and `reasoning.summary`.** See `formats/registry.rs:3209-3253`. Both `4da8c57fe` and the selected reasoning-summary patch are required.
- **Gemini schema cleaning is minimal.** `generate_content/request.rs:865-883` only recurses and inserts empty `properties`; it does not sanitize/resolve JSON Schema. `64e572533` is required.
- **Mixed tools are absent.** Gemini conversion returns immediately after applying extensions (`generate_content/request.rs:183-190`), and same-format body handling only strips function-response IDs after body rules (`same_format_provider/mod.rs:459-495`). Apply the cross-format, model gate, and same-format trio.
- **Codex `additional_tools` exists but only in specialized Codex/WS code.** Examples are `formats/openai/responses/codex.rs:1257-1452` and gateway WS request handling at `handlers/proxy/websocket/responses/request.rs:120-215`. Generic conversion entry points at `formats/registry.rs:117-178` do not normalize it for Responses→Chat, so `5bcdcca78` is required.
- **Antigravity envelope has no tool-wire normalization.** `aether-provider/transport/src/antigravity/request.rs:76-109` currently removes model/safety fields and wraps the body only. Apply the schema alias and private search-name chain.
- **Compaction candidate restriction is absent.** The format matrix isolates only the legacy `openai:responses:compact` format (`formats/matrix.rs:110-155`), while standard Responses preselection expands all compatible formats at `candidate_source.rs:202-240`. `56395945c` is needed for `compaction_trigger` on normal Responses.
- **Antigravity load-code-assist host needs a scoped merge.** The fork currently uses the daily host for `build_antigravity_load_code_assist_plan` (`aether-model-fetch/src/transport.rs:263-304`) and separately uses the production host for Gemini CLI (`:306-349`). Apply `4da8c57fe` only to the Antigravity path and its matching tests; do not collapse the two fork-specific provider flows.

### Request, response, stream, and same-format contracts

```text
Responses/Chat/Claude/Gemini request
  -> registry normalization (only leading exact `additional_tools` for Responses->Chat)
  -> canonical parse + fail-closed validation
  -> Gemini request emission
       tool-result ordering + raw string preservation
       function-call thoughtSignature replay
       JSON Schema sanitization
       Gemini-3 mixed-tool flag
  -> provider transport
       native Gemini same-format compatibility edit, or
       Antigravity private envelope field/name normalization

Gemini sync/stream response
  -> canonical ToolUse extension / ToolCallSignature event
  -> Responses sync/SSE emitter writes directional synthetic reasoning carrier
  -> later Responses history conversion decodes carrier
  -> original Gemini functionCall receives exact thoughtSignature
```

- Normalization must occur before parsing/validation, but in the runtime path it must use the body after the fork's `expand_previous_response_for_chat` step (`formats/registry.rs:146-163`). Unknown `additional_tools` fields, unsupported tool kinds, and non-leading occurrences must still fail closed.
- Keep `ReasoningSignature` for thought/text blocks separate from the new `ToolCallSignature`; do not overload one event. The fork has additional canonical variants (`ContentPart`, `ImageGenerationCall`, `OpenAiResponsesOutputItem`, `ReasoningSummaryDone`) at `protocol/stream.rs:42-78`; every exhaustive emitter/aggregator must retain those arms while adding the new one.
- Same-format OpenAI Responses is not canonical conversion. It must continue copying the JSON object and preserving unknown fields. The new same-format mutation is Gemini-only and must be recorded in `SameFormatProviderCompatibilityEdit` so exact-body reuse is impossible after mutation (`same_format_provider/mod.rs:318-423`, `:459-495`).
- Native same-format Responses SSE remains byte/event passthrough. Tool-signature carrier logic belongs only to cross-format Gemini↔Responses conversion; it must not rewrite native Responses events.
- The fork's Responses replay sanitizer (`formats/openai/responses/mod.rs:41-118`) preserves Aether synthetic reasoning items with non-empty encrypted content. Add a focused regression showing that a Gemini carrier is decoded on the Gemini route and does not break the local OpenAI/DeepSeek replay-policy behavior.
- Antigravity normalization belongs inside `build_antigravity_safe_v1internal_request`, for both already-wrapped and unwrapped inputs. Public Gemini/Vertex bodies must not be renamed to private `googleSearchRetrieval` globally.
- Compaction restriction belongs at candidate-source preselection, including caller-supplied candidate format lists, not only at body conversion. Conversion rejection is the second fail-closed guard.

### Risky overlapping files and symbols

| Risk | File / symbols | Why integration must be manual or serialized |
| --- | --- | --- |
| Critical | `crates/aether-ai/formats/src/formats/registry.rs` — `convert_request*`, extension validators, `validate_openai_responses_to_chat` | Seven selected patches touch this hub. The fork has namespace tools, custom tools, previous-response expansion, model capabilities, and stricter fail-closed logic not present in the original hunks. |
| Critical | `crates/aether-ai/formats/src/protocol/canonical.rs` — Gemini history, OpenAI tool results, Responses input/output | Fork partial ID pairing conflicts structurally with `8cdfa338e`; c4 adds carrier decoding while local custom/hosted tool variants and DeepSeek handling must survive. |
| Critical | `crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs` | Six patches stack here; local raw tool/extension behavior must survive schema, signature, ordering, and mixed-tool changes. |
| Critical | `crates/aether-ai/formats/src/formats/openai/chat/stream.rs`, `protocol/stream.rs`, Gemini/Claude stream emitters, `shared/sync_products.rs` | `ToolCallSignature` makes existing matches exhaustive and intersects fork-only stream variants/error handling. Apply c4 behavior across every compiler-reported consumer, not just its original four files. |
| Critical | `crates/aether-provider/transport/src/same_format_provider/mod.rs` | The fork owns exact-body fidelity, body rules, function-response ID stripping, reasoning replay, and credential isolation here. Insert the Gemini compatibility edit without weakening any of them. |
| High | `crates/aether-provider/transport/src/antigravity/request.rs` | `9837ce119`, `36daba7a3`, and `b35364d7f` are a correction chain; final wire field is `parameters`, and search renaming is private-boundary-only. |
| High/shared | `apps/aether-gateway/src/ai_serving/planner/candidate_source.rs` | `56395945c` overlaps R1/R2 routing and cache-affinity work. One integrator must merge the operation filter after those changes. |
| High/shared | `apps/aether-gateway/src/execution_runtime/transport.rs` | `83098f98b` adds a wire test in a file likely touched by R2 concurrency work. Serialize ownership; behavior change itself remains in format/provider code. |
| Medium | `crates/aether-ai/formats/src/formats/shared/standard_matrix.rs`, `shared/model_directives.rs`, `lib.rs`, gateway `pure/mod.rs` | Test expectations and exports must follow the final stacked behavior; avoid duplicate helpers/re-exports. |
| Medium | `crates/aether-model-fetch/src/{strategy,transport}.rs`, `apps/aether-gateway/src/tests/control/admin/provider_query.rs` | `4da8c57fe` changes only Antigravity production bootstrap host; preserve fork's separate Gemini CLI and daily/sandbox coverage. |

### Validation matrix

The following upstream tests cover each requirement and should remain present after conflict resolution:

- Tool-result/string/order and `include`: the five `4da8c57fe` registry tests plus Antigravity production load-code-assist test.
- ID-less history: all five `8cdfa338e` pairing/collision tests.
- Thought signatures: all c4 carrier, sync, stream-order, round-trip, and previous/next tests; ensure one regression crosses actual Gemini stream -> Responses SSE -> Responses history -> Gemini request.
- Reasoning summary: all three selected `fa8e443f7` tests; budget tokens must still fail closed.
- Gemini schema: `canonical_tool_declaration_sanitizes_json_schema_for_gemini`.
- Mixed tools: the f0 matrix test, the 830 Gemini-2 rejection/Gemini-3/wire tests, and 1bc same-format compatibility-report test.
- Additional tools: all four registry cases and gateway planner normalization test from `5bcdcca78`.
- Antigravity wire: real-agent envelope, JSON-schema alias normalization, and both public search spellings mapped only inside the private envelope.
- Compaction: both `56395945c` tests, including ordinary Responses retaining cross-format candidates and legacy compact remaining isolated.

Run targeted filters serially and verify every command reports a non-zero test count:

```powershell
cargo test -p aether-ai-formats
cargo test -p aether-provider-transport same_format_gemini_body_enables_mixed_tool_invocations -- --nocapture
cargo test -p aether-provider-transport antigravity -- --nocapture
cargo test -p aether-gateway direct_sync_execution_runtime_preserves_gemini_tool_config_on_wire -- --nocapture
cargo test -p aether-gateway maps_openai_responses_additional_tools_without_message_name -- --nocapture
cargo test -p aether-gateway compaction_operation_excludes_non_responses_provider_formats -- --nocapture
```

Also rerun the fork contracts that must not regress:

```powershell
cargo test -p aether-provider-transport same_format_responses_body_preserves_opaque_extension_fields -- --nocapture
cargo test -p aether-provider-transport same_format_headers_cannot_restore_credentials_or_internal_headers -- --nocapture
cargo test -p aether-ai-formats falls_back_to_body_json_for_openai_responses_same_family_sync_payload -- --nocapture
cargo test -p aether-ai-formats rejects_openai_responses_same_family_error_body_json -- --nocapture
cargo test -p aether-gateway openai_sync_and_stream_builders_prefer_prevalidated_exact_body -- --nocapture
cargo test -p aether-gateway local_exact_body_requires_unchanged_same_format_unencoded_json -- --nocapture
cargo test -p aether-gateway prefetched_codex_cyber_policy_violation_stops_failover_by_default -- --nocapture
cargo test -p aether-gateway prefetched_codex_cyber_policy_violation_retries_when_system_setting_is_enabled -- --nocapture
cargo test -p aether-gateway same_format_responses_prefetch_retries_bare_error_before_committing_success -- --nocapture
cargo fmt --all --check
python docs/api/generate_format_field_coverage.py --check
```

No validation command was run during this research task because the researcher write boundary permits output only under this task's `research/` directory, while Cargo and generators create artifacts elsewhere.

### Integration requests

1. Give `candidate_source.rs` and `execution_runtime/transport.rs` to the final serial integrator after R1/R2 changes; do not let R4 cherry-picks race those writers.
2. Use `fa8e443f7`, not both duplicate reasoning-summary commits.
3. Apply `9837ce119` and `36daba7a3` adjacently; final Antigravity function declarations must carry `parameters`, never `parametersJsonSchema`.
4. Merge `8cdfa338e` into the fork's existing ID-pairing algorithm and delete the superseded partial path; do not retain two pairing state machines.
5. For c4, follow every compiler-reported `CanonicalStreamEvent` match and preserve fork-only event/error branches. Do not modify native Responses same-format SSE.
6. Insert 1bc's Gemini compatibility edit after body rules and before existing function-response-ID stripping, preserving the compatibility report and exact-body invalidation.
7. Apply 5bcd normalization after runtime previous-response expansion. Preserve exact-shape/leading-prefix guards and fail closed otherwise.
8. Restrict 4da's host change to Antigravity bootstrap; preserve separate Gemini CLI production and daily/sandbox model-fetch paths.
9. Do not import VSCodex, generic Usage API, entitlement revocation, nightly/release, or merge-only changes while resolving context.

## Files found

- `crates/aether-ai/formats/src/formats/registry.rs` — central pure/runtime conversion, loss audit, request normalization, and most protocol regressions.
- `crates/aether-ai/formats/src/protocol/canonical.rs` — canonical Gemini/OpenAI request and response parsing, tool pairing, and extension ownership.
- `crates/aether-ai/formats/src/protocol/stream.rs` — canonical stream event contract.
- `crates/aether-ai/formats/src/formats/gemini/generate_content/request.rs` — Gemini request emission, tool schemas, tool history, signatures, and mixed tools.
- `crates/aether-ai/formats/src/formats/gemini/generate_content/stream.rs` — Gemini provider stream parsing and client stream emission.
- `crates/aether-ai/formats/src/formats/openai/chat/stream.rs` — Chat/Responses stream emitters and Responses output-item ordering.
- `crates/aether-ai/formats/src/formats/openai/responses/{mod,response}.rs` — Responses operation/replay policy, signature carrier, and sync response projection.
- `crates/aether-ai/formats/src/formats/shared/{standard_matrix,sync_products,model_directives}.rs` — end-to-end conversion matrix, stream-to-sync aggregation, and model gates.
- `crates/aether-provider/transport/src/same_format_provider/mod.rs` — native body fidelity and provider compatibility edits.
- `crates/aether-provider/transport/src/antigravity/request.rs` — private v1internal request envelope and final wire normalization.
- `apps/aether-gateway/src/ai_serving/planner/candidate_source.rs` — candidate-format preselection and compaction restriction point.
- `apps/aether-gateway/src/execution_runtime/transport.rs` — final on-wire request regression.
- `crates/aether-model-fetch/src/{strategy,transport}.rs` and `apps/aether-gateway/src/tests/control/admin/provider_query.rs` — Antigravity project hydration/bootstrap host behavior.

## Code patterns

- Same-format request fidelity is copy-first, then explicit candidate/provider edits (`same_format_provider/mod.rs:348-423`); unknown native fields must not pass through a canonical allowlist.
- Cross-format conversion validates before emit and uses extensions for semantics that can round-trip (`formats/registry.rs:117-210`, `:1930-2028`). Unknown material semantics are errors.
- Gemini request history currently compacts adjacent roles after emission (`generate_content/request.rs:808-842`); 4da moves alignment before part generation so tool results can be ordered without losing surrounding text.
- Provider-private spelling changes belong in the envelope builder (`antigravity/request.rs:49-110`), not in the public Gemini converter.
- Responses compaction is a semantic request operation carried on the normal wire format (`formats/openai/responses/mod.rs:197-222`), so routing needs operation-aware format filtering rather than another public format alias.

## External references

- [Gemini GenerateContent thought signatures](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures) — Gemini 3 function-call signatures must be returned exactly; for parallel calls the first call part carries the signature.
- [Gemini GenerateContent mixed built-in and function tools](https://ai.google.dev/gemini-api/docs/generate-content/tool-combination) — documents mixed tools and `toolConfig.includeServerSideToolInvocations` for GenerateContent.
- [Google FunctionDeclaration wire schema](https://docs.cloud.google.com/gemini-enterprise-agent-platform/reference/rest/Shared.Types/FunctionDeclaration) — public schema documents mutually exclusive `parameters` and `parametersJsonSchema`; it does not document Antigravity's private v1internal preference.
- [OpenAI Chat API reference](https://platform.openai.com/docs/api-reference/chat) — Chat exposes `reasoning_effort`, not the Responses `reasoning.summary` control.
- Upstream commit links in the matrix are the authoritative implementation/test evidence for the private Antigravity spellings and selected patch versions (2026-08-27 through 2026-08-29).

## Related specs

- `.trellis/tasks/09-02-sync-selected-upstream-features/prd.md` — R4 and preservation acceptance criteria.
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md` — native unknown-field/event preservation, compact surface, fail-closed cross-format behavior, exact-body constraints, and SSE commit boundary.
- `.trellis/spec/aether-ai-formats/backend/index.md` and `.trellis/spec/aether-provider-transport/backend/index.md` — package ownership links for the Responses relay contract.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` — trace each format boundary and keep payload ownership centralized.

## Caveats / Not Found

- Antigravity `v1internal` is private. No public Google document was found that establishes its final `parameters` and `googleSearchRetrieval` spellings; confidence comes from the upstream corrective sequence and its tests. Preserve those tests as executable evidence.
- Official public Google docs distinguish surfaces: the mixed-tool flag is documented for GenerateContent, while endpoint behavior can differ. Validate both Generative Language and Vertex/provider-private paths supported by this fork; do not broaden the private Antigravity rewrite into public Vertex/Gemini code.
- The signature carrier is an Aether convention, not an OpenAI standard field. It intentionally uses a synthetic Responses reasoning item. Its interaction with the fork's OpenAI-vs-DeepSeek replay policy needs the focused regression requested above.
- Commit ancestry and patches were inspected through GitHub's public commit API/pages because this research role forbids repository Git operations. Baseline identity is therefore the value supplied by dispatch; local source findings were verified directly in the worktree.
- No product files, task state, refs, index, or worktree state were changed.
