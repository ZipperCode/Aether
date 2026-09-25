# Research: Codex image body construction and replay diagnostics

> Follow-up evidence: `live-evidence.md` now records the recovered 13:06 request
> with `output_format=png`, proving that Codex-specific rejection. The earlier
> unavailable-body discussion below describes the initial 12:58 evidence only.
> `image-missing-usage.md` separately explains why the later 13:09 edits have no
> usage/audit body. Implementation scope follows the updated PRD/design.

- Query: Trace `provider_request_body_missing`, identify all shared Image/Search body callers and field rejection branches, and compare current public Images documentation with pinned official Codex and sub2api implementations.
- Scope: mixed; local source and public primary-source inspection. No code edits, Cargo/test/build commands, production access, or upstream inference calls by this agent.
- Date: 2026-09-25
- Local baseline: parent supplied HEAD `8e765877e`; live release supplied by parent is v0.7.39 / `22124fe59`.

## Findings

### Evidence boundary

The parent independently verified three 154 requests around 12:58 +08:00 for `gpt-image-2.5-flare`, including request `cc588981-e46c-459a-ba3f-2f76b1669545`. The Codex candidate was skipped with `provider_request_body_missing` and `started_at = NULL`; another provider completed with HTTP 200. Capture was disabled and no original body blob exists. The stored diagnostic retained only the generic root path/message and format information.

**This proves local pre-request rejection; it does not identify the actual offending field.** The examples below are source-derived regression candidates, not a replay of the user's unavailable request. Success through another provider does not prove the selected Codex account accepts the same wire payload.

The parent's later discriminating observation is stronger: the successful custom provider `sub.muxing.cfd` used the **same `gpt-image-2.5-flare` target model on the same three requests**. In the native image planner, custom and Codex candidates share normalization and `project_openai_image_api_request_body`; Codex alone adds its stricter projector. Subject to verifying both candidates used this native branch, the same target model, and equivalent provider-dependent options, this argues **against shared quality/size/response_format validation as this incident's cause**. Such values would normally reject the successful custom candidate too. The leading remaining possibilities are Codex-only optional-field rejection, URL-only edit input restrictions, or the 5-image edit cap (native GPT permits 16). Provider-specific upstream stream policy is a remaining discriminator: common validation of partial_images depends on the resolved upstream stream flag. Do not diagnose xhigh/max or custom dimensions as the observed incident merely because separate coverage gaps exist.

### Files found

| File | Responsibility |
| --- | --- |
| `crates/aether-ai/formats/src/formats/openai/image/request.rs` | JSON/multipart normalization, native Images projection, Codex projection and existing image regressions. |
| `apps/aether-gateway/src/ai_serving/planner/specialized/image/request.rs` | Official image-route candidate construction and generic image diagnostics. |
| `apps/aether-gateway/src/handlers/admin/provider/query/models/model_test.rs` | Management probe construction, shared image builder, separate error presentation. |
| `apps/aether-gateway/src/ai_serving/planner/standard/openai/{chat,responses}/decision/request.rs` | Chat/Responses to native Images bridge callers. |
| `apps/aether-gateway/src/ai_serving/planner/standard/family/request.rs` | Gemini-to-Images bridge caller. |
| `crates/aether-ai/serving/src/{failure_diagnostic,request_body_diagnostics}.rs` | Typed candidate diagnostic and text-oriented fallback analysis. |
| `crates/aether-data/contracts/src/repository/candidates/types.rs` | Persistence/admin/public diagnostic projection. |
| `crates/aether-ai/formats/src/formats/openai/{search,request_contract}.rs` | Alpha Search projection and shared final request contract. |
| `crates/aether-ai/formats/src/formats/openai/responses/codex.rs` | Codex endpoint recognition and separate Responses image-tool adaptations. |

### Native image construction and rejection points

1. `normalize_openai_image_request_with_options` dispatches by `/v1/images/generations` or `/v1/images/edits`, then JSON versus multipart (`image/request.rs:493`).
2. `normalize_openai_image_json_request` returns `None` for non-object JSON, `n` outside the configured range, unrecognized response/output format, invalid partial image count, invalid boolean-like stream, missing edit images, or failed tool-option normalization (`:1144`). A missing prompt survives this stage because `normalize_prompt` returns `Some(None)` (`:1319`).
3. `build_tool_options` validates known fields (`:1439`). `normalize_openai_image_quality` accepts only low/medium/high/auto plus standard/hd aliases (`:1488`). Consequently `xhigh` or `max` fails before provider projection, regardless of model.
4. `build_codex_openai_image_api_provider_request_body` chooses the mapped model before requested/default model, constructs the body, and invokes the Codex projector (`:563`, `:581`).
5. `project_openai_image_api_request_body` checks prompt, operation/input shape and count, n, quality, formats, background, moderation, fidelity, compression, stream/partial images, style and size (`:630`).
6. `project_codex_openai_image_api_request_body_with_max_generation_count` first invokes that common projection, then applies a second key allowlist and constructs a reduced object (`:1022`). Allowed projected keys are model/prompt/background/n/quality/size/images/response_format/stream. `output_format`, `moderation`, `output_compression`, `input_fidelity`, mask, partial_images and user therefore fail here if not already rejected. Edits additionally require 1–5 URL-backed references (`:1028`, `:1124`).

Important model-version edge: `openai_image_model_family` recognizes `gpt-image-2` and `gpt-image-2-*`, but `gpt-image-2.5-*` falls into the older generic GptImage family (`:919`). `openai_image_size_supported` restricts that family to auto/1024x1024/1536x1024/1024x1536 (`:964`). This is a concrete 2.5 coverage gap independent of the unavailable live body.

The preceding public 2.5 gaps are **separate follow-up findings, not authorization to broaden this incident fix without its actual payload**. Wait for sanitized parameters to choose a field/protocol repair; the diagnostic source-loss issue is independently confirmed.

Changing only the Codex key allowlist is insufficient: the final construction at `:1073-1103` also copies only prompt/background/model/n/quality/size/images. Any extension must retain the intended fields in the emitted body, not silently discard them.

### All production callers affected by the shared projection

| Caller | Current behavior on image failure |
| --- | --- |
| Native image planner, `specialized/image/request.rs:131-156` | Normalization failure -> generic diagnostic with source `openai_image_request_normalize`. |
| Native image planner, `specialized/image/request.rs:197-230` | Codex builder failure -> generic diagnostic with source `codex_openai_images_request_contract`. The same label is also used for non-Codex native projector failures. |
| Management image probe, `model_test.rs:2364-2380`, `:2420-2440` | Calls the same normalizer/builder, but reports a separate English “outside the Codex Images contract” message. |
| Responses-to-image bridge, `standard/openai/responses/decision/request.rs:1468-1481` | Final Codex/native projection uses bare `?`, dropping the candidate without a projection-specific diagnostic. |
| Chat-to-image bridge, `standard/openai/chat/decision/request.rs:1427-1457` | Records `provider_request_body_build_failed`, but diagnoses original Chat input rather than the projected image body. |
| Gemini-to-image bridge, `standard/family/request.rs:1570-1587` | Final projection uses bare `?`, also losing projection-specific failure details. |

The common native projector is also used by non-Codex Images. Model-specific fixes belong in this common owner; private Codex wire restrictions belong in the Codex projector. Fixing only management probes or only the native image route leaves sibling entrypoints inconsistent.

`responses/codex.rs:1896-1905` is a different path: explicitly selected Responses image generation rewrites the Responses host model and image tool defaults. It is not invoked by the native Images projector and cannot explain the observed native `openai:image` generic error by itself.

### Diagnostic propagation holes

- `CandidateFailureDiagnostic::provider_request_body_missing` constructs the exact user-visible Chinese message with path `$` (`failure_diagnostic.rs:177-188`). It is a fallback message, not a statement that the caller sent invalid JSON.
- The native image planner bypasses `request_body_build_failure_extra_data`; its normalizer/projectors return `Option`, so no field error survives. The existing generic analyzer only has dedicated Responses and Chat-to-Claude/Gemini inspection (`request_body_diagnostics.rs:169-198`), not image field diagnosis.
- The candidate diagnostic includes kind/source/client_api_format/provider_api_format plus source_format/target_format (`failure_diagnostic.rs:220-233`). `mark_skipped_local_execution_candidate_with_failure_diagnostic` forwards `to_extra_data()` unchanged (`candidate_materialization.rs:2400-2419`).
- **Persistence loses the discriminant:** `sanitize_request_candidate_extra_data_for_persistence` only retains path/field_path/message/type/reason/details/stage/source_format/target_format/safe_to_show from `failure_diagnostic` (`candidates/types.rs:898-919`). It does not retain kind or source. The base public projection does not restore these (`:966`). This matches the parent's live stored result.
- `project_routing_failure_diagnostic` deliberately projects only known kind/path for usage metadata (`usage/metadata_policy.rs:643-670`). This is a separate public/minimal contract; do not widen it merely to repair the administrator's persisted candidate evidence.

Suggested shared fix location: make the existing normalization/projection owner produce a structured field/path error and map that error into the existing `CandidateFailureDiagnostic` in each caller; preserve safe source/kind in persistence/admin projection. Avoid a second hand-written image validator in the UI or management test path. Keep public projection restrictions and capture settings intact.

### External references: public Images versus private Codex

Sources fetched in this research, not inferred from old answers:

1. [OpenAI Image generation guide](https://developers.openai.com/api/docs/guides/image-generation), current page fetched 2026-09-25. It documents 2.5 Flare/Sunburst custom dimensions, multiples of 16, aspect ratio up to 3:1, maximum edge 3840, and total pixels 655360–8294400. Sizes above 2560x1440 are experimental. It also documents transparent output with PNG/WebP. These are public API capabilities, not account-specific Codex acceptance evidence.
2. [OpenAI Create image reference](https://developers.openai.com/api/reference/cli/resources/images/methods/generate), current page fetched 2026-09-25, lines 449–487 in the fetched rendering. It lists `xhigh` and `max` for the two 2.5 models and their 2026-09-08 snapshots. It documents output_format png/jpeg/webp, moderation low/auto, compression for JPEG/WebP, and custom 2.5 size strings. Exact short excerpt on response_format: “This parameter isn’t supported for the GPT image models, which always return base64-encoded images.”
3. [Official Codex images.rs, pinned 75e0e0aa](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/images.rs#L5-L49), raw file re-fetched. Its typed generation/edit DTO has prompt/background/model/n/quality/size, plus edit images. Quality enum is low/medium/high/auto; size/model are strings. This is the CLI client DTO, not an exhaustive server rejection schema. The omitted output_format/moderation fields are not affirmative proof the private server rejects them.
4. [sub2api direct Images, pinned a3eb7ef3](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_images_direct.go#L30-L126), raw file re-fetched. Direct models explicitly include 1.5, 2, 2.5 Flare/Sunburst and dated 2.5 snapshots. The shared OAuth payload builder forwards parsed size/quality/background/output_format/moderation/input_fidelity/style, n, output_compression, partial_images and stream. Edits send plural image_url references and an optional URL-backed mask. Unlisted image models use its Responses bridge, not automatically the direct API.
5. [sub2api parser and model validation](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_images.go#L192-L244), raw file re-fetched. Parse -> defaults -> resolved upstream model -> compatible image-model validation -> billing size tier -> required capability. JSON extraction reads quality/size as strings (`:247-280`); it does not impose Aether's old four-value quality enum in that extraction. GPT family recognition uses `gpt-image-` (`:483`), while compatible Gemini image mappings require API-key capability (`:521-527`). This does not authorize bypassing Aether's OAuth plan, whitelist, or quota checks.

#### response_format=url is compatibility, not native Codex support

The public API reference explicitly excludes this wire parameter for GPT image models. sub2api's direct payload does not send response_format. Its response handler locally converts returned base64 bytes to a `data:<mime>;base64,...` URL for a client requesting url (`openai_images_direct.go:166-170`, `:240-249`). This is not an OpenAI-hosted image URL or evidence the private endpoint accepts response_format=url.

Aether currently accepts the normalizer's url enum but rejects it in the common GPT-image projector (`image/request.rs:762-774`), before the Codex-specific check (`:1066-1070`). Retaining that intentional boundary with an accurate `$.response_format` diagnostic is safer than claiming upstream URL support. Implementing a data-URL compatibility response is separate behavior and should not be fabricated merely to make request construction pass.

### Source-derived minimal regression candidates (not executed)

Use `/v1/images/generations`, mapped model `gpt-image-2.5-flare`, and base JSON `{ "model": "gpt-image-2.5-flare", "prompt": "a blue square" }`. Keep upstream stream policy explicit in the test.

| Add/change | Current source-derived rejection | Appropriate boundary |
| --- | --- | --- |
| `"quality": "xhigh"` or `"max"` | Normalizer rejects via `normalize_openai_image_quality`. | Confirmed public 2.5 gap; private acceptance still subject to upstream. |
| `"size": "1536x864"` | Normalization passes; common projector treats 2.5 as old family. | Confirmed public 2.5 size gap; do not merely broaden every old model. |
| `"output_format": "png"` | Common projector accepts; Codex projector rejects key. | sub2api direct forwards it; retain its value when adapting. |
| `"moderation": "auto"` | Same Codex key rejection after native validation. | sub2api direct forwards it; public Images documents it. |
| `"response_format": "url"` | Common GPT-image projector rejects. | Precise field error; URL conversion is separate behavior. |
| `"n": 0` or `11` | Normalization rejects count. | Preserve real validation, report `$.n`. |
| Missing prompt | Normalization succeeds, common projector fails required prompt. | Report `$.prompt`. |
| Edit route without images | Normalization rejects. | Report edit image requirement, not generic JSON failure. |

Existing tests `builds_codex_image_generation_with_the_typed_images_contract` (`image/request.rs:2551`) and `rejects_fields_outside_the_codex_images_contract` (`:2621`) intentionally encode the narrow DTO. They must be revised only for evidenced field support; generic existing PASS does not cover modern optional fields or real client bodies.

### Search scope

Alpha Search does not run through the native Images builder. It uses same-format body construction (`passthrough/provider/family/request.rs:203-239`), then shared OpenAI finalization; typed finalization failures already map to `openai_provider_request_contract_failure_extra_data` (`:315-329`). Search projection is `apply_openai_search_request_projection` (`openai/search.rs:81`), called from `request_contract.rs:165`. It retains the dedicated Search fields and has no `Option` failure of its own. Search tests synthesize id/input/commands/max_output_tokens (`model_test.rs:756`, `:849`). No captured Search failure was supplied in this follow-up; the observed error is not evidence of a new Search-specific regression.

### Related specs and constraints

- `.trellis/spec/aether-model-fetch/backend/quality-guidelines.md`: dynamic configured image names, separate OAuth known-Free restriction, no invented paid-plan matrix, existing filters/whitelists/quota remain authoritative.
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md`: native/cross-format fidelity and fail-closed semantics; Search is its own JSON protocol.
- `crates/aether-ai/formats/AGENTS.md`: pure conversion errors must not silently drop material semantics or be encoded only as None.
- `.trellis/tasks/archive/2026-09/09-25-codex-oauth-capabilities/research/upstream-capabilities.md`: previous pinned versions and distinction between tool discovery, Search, and plan eligibility.

## Caveats / Not Found

- No exact live JSON, multipart content, original per-field value, or client stream setting is available. Parent has requested sanitized parameters; a synthetic candidate must not be labeled the actual replay.
- No live Codex 2.5 image call was performed. Public API docs, official CLI DTO, and sub2api forwarding are different strengths of evidence; none guarantees this account's private endpoint acceptance.
- No code, specs, platform config, production state, or Git state was changed by this research. No tests were run.
- Historical memory was used only for the existing “no model-name special-case catalog repair” constraint (MEMORY.md:3202; rollout `01a06ca4-366f-7a03-b9a5-3ad15c42ec62`). Current relevant specs/source were inspected independently.
