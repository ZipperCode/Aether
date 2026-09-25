# Local Codex OAuth endpoint audit

Research baseline: `1b8c974b0`, 2026-09-25. Read-only source audit; production evidence is in `live-evidence.md` and primary-source semantics in `upstream-capabilities.md`.

## Confirmed findings

- Fixed Codex template (`provider_types.rs:313`) exposes Responses, Compact, Alpha Search, Images, Live. Text model fetching deliberately calls only one catalog endpoint, but the resulting management projection only attaches that chosen transport. New discovered model names therefore never acquire sibling Search/Compact bindings.
- `crates/aether-model-fetch/src/logic.rs`: `MODEL_FETCH_FORMAT_PRIORITY`, `selected_models_fetch_endpoints_for_api_formats` and `endpoint_supports_rust_models_fetch` select one OpenAI sibling. `project_codex_models_for_legacy` projects only returned endpoint formats. Admin cached projection (`models/mod.rs:591`) likewise starts Responses-only. Fix shared management projection, not repeated catalog network requests.
- Image supplement (`logic.rs:379`) injects only the request-default constant; runtime separately pushes that same name (`runtime.rs:669`). Existing API request construction already preserves a supplied real image model. Replace fixed catalog insertion using active configured image models and accurate bindings; ignore dynamic `is_available` when seeding a repair.
- Old migration image bindings include text models. Active image binding alone is insufficient to label a text model as an image generation model; use existing model family/declared capability and mapped actual names.
- `provider_query_key_allows_effective_test_model` treats explicit `allowed_models=[]` as unrestricted; formal scheduler denies it. Keep `None` unrestricted, align explicit empty lists with formal behavior.

## Corrected Search semantics

Earlier inference from the field name `supports_search_tool` was withdrawn after inspecting official Codex code: it controls deferred tool discovery, NOT Alpha Search. Its false/missing value must not deny web search. Actual standalone search checks provider web-search capability and namespace tools plus Responses Lite or a client feature flag. See pinned primary links in `upstream-capabilities.md`.

Project genuinely discovered Codex text models to configured active/permitted Search and Compact as routable candidates. Preserve manual inactive bindings and upstream authorization errors; do not present discovery as guaranteed entitlement. `api_format_permission_covers` (`formats/id.rs:178`) already makes Responses permission cover Search/Compact, so avoid literal-format-only checks.

## Existing protocol implementations

| Endpoint | Existing behavior and verification surface |
| --- | --- |
| Responses | Standard Codex native HTTP/SSE/WS; retain unknown fields, stable logical identity and existing authorization rules. |
| Compact | `/responses/compact`, typed body/header policy in `formats/openai/responses/codex.rs`; tests under `ai_execute/{stream_cli,finalize_local_cli}/compact.rs`. Automatic binding is missing for new catalog models. |
| Search | Native `/alpha/search`, synchronous even if input contains stream=true. Existing `ai_execute/sync/search.rs` asserts plans through a mock runtime; add actual loopback HTTP proof. Shared Search header branch needs minimal JSON headers and removal of Responses-only headers after overrides. |
| Images | Native `/images/generations` and `/images/edits`, real gateway sync/stream tests already exist. Extend their model/plan matrix, do not substitute Responses tool output. |
| Live | Explicit `codex:live` binding; OAuth WebRTC call creation uses `/realtime/calls?intent=quicksilver&architecture=avas`. OAuth direct WS is deliberately rejected (`handlers/proxy/websocket/live/planner.rs:375`); no native card evidence justifies automatic Live binding. Ordinary text model-test adapter has no Live protocol test. |

## Plan and permission seams

- Full Key extraction `aether_provider_pool::derive_oauth_plan_type` preserves status-snapshot > provider metadata/root > auth-config precedence. Share pure normalization rather than duplicating plan names or collapsing raw ProLite to its display label.
- Official Codex excludes known Free images. No verified blanket Search plan table exists. Metadata/auth already exists on the selected transport; lightweight maintenance candidates can carry the auth_type scalar, never a full status snapshot or credentials.
- `candidate_common_transport_skip_reason` is the formal shared transport gate. Pool-group representatives bypass it initially; `dispatch/pool_scheduler.rs` rechecks actual hydrated Key transports. Verify both paths without rejecting an entire pool due to one Free representative.
- Admin image tests must invoke the same capability rule. New diagnostic codes must be registered in `repository/candidates/types.rs` and mapped in existing frontend usage/model-test diagnostics.
- `ModelFetchAssociationStore` owns configuration reads and binding sync. Existing backend sync preserves manual active/inactive bindings. Keep once-per-provider availability reconciliation and bounded lightweight reads.

## Focused acceptance checks

1. A future configured image name enters new/cached management projection and automatic whitelist; inactive/missing endpoint, explicit exclusions and pure-text legacy bindings do not expand support.
2. Known Free image requests send zero upstream calls in formal sync/stream and admin; Plus, Pro, ProLite, future/unknown plans retain existing permission and quota gates.
3. New real native text models acquire Search/Compact on active permitted endpoints; `supports_search_tool=false` does not block Alpha Search; no automatic Live binding.
4. Manual inactive bindings, `None` versus explicit empty allowlists, and original native cards remain correct.
5. Actual loopback Images and Alpha Search assert final URL, selected model, body and headers. Existing mocked plan assertions alone are insufficient.
6. Relevant Responses/Compact/Live protocol, quota and architecture regressions remain passing.
