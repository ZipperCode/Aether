# Research: Codex OAuth plans and upstream capabilities

- Query: How do current sub2api and official Codex sources distinguish subscription plans, Images, Alpha Search, Responses/Compact, and Live?
- Scope: mixed; read-only source/documentation research, no production access or live account calls.
- Date: 2026-09-25
- Versions: sub2api main `a3eb7ef302961cba716dc78b39b93b60c467db0e`; official openai/codex main `75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf`. Both main SHAs were resolved during this research.

## Findings

### 1. Plan evidence: gate known Free images, preserve other plan identities

Official documentation explicitly excludes Free from Codex image generation and says images consume the same general usage pool as ordinary Codex work; heavier consumption or a 429 does not mean an account lacks the feature. Source: [official pricing, image-generation limits](https://learn.chatgpt.com/docs/pricing#image-generation-usage-limits), fetched successfully through `https://developers.openai.com/codex/pricing.md` (redirects to the official Learn site).

This is corroborated by executable official code, rather than a marketing table: [`image_generation_available`, spec_plan.rs:727-763](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L727) rejects exactly `Some(PlanType::Free)` at lines 737-745. Other prerequisites are the enabled image feature, provider image/namespace capabilities, image input modality, and Codex-compatible auth. It does **not** enumerate Plus/Pro as the only allowed plans.

Official [auth.rs:66-95](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/protocol/src/auth.rs#L66) preserves raw unknown plans and distinguishes `free`, `go`, `plus`, `pro`, `prolite`, `promax`, and organization/education SKUs. [auth.rs:130-171](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/protocol/src/auth.rs#L130) displays ProLite as “Pro” and Pro as “Pro (More)”; display labels are unsuitable entitlement keys.

| Raw plan | Evidence-grounded interpretation |
| --- | --- |
| Free | Official Codex explicitly disables image generation. Do not supplement image eligibility for known Free. |
| Plus / Pro / ProLite / ProMax | No distinction in the official image exclusion guard. Keep raw identity; actual model access and quota still belong to upstream. |
| Unknown / missing / future SKU | Not equivalent to Free, and not proof of support. Preserve unknown state; do not invent a paid-plan allowlist or deny a future SKU merely because it is new. |
| Any plan with 429 | Record the actual quota scope/reset evidence, separately from capability or unsupported-model decisions. |

sub2api does **not** supply a complete entitlement matrix: [account.go:2015-2028](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L2015) selects Images by credential type. [account.go:1328-1337](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L1328) excludes empty/Free/abnormal from a *subscription-priority* category, consumed by [openai_account_scheduler.go:1546](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_account_scheduler.go#L1546). That is scheduling preference, not a universal feature permission rule.

### 2. Images: family recognition and protocol choice are separate

- [openai_images.go:483-503](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_images.go#L483) recognizes `gpt-image-*`; the default model is distinct from the accepted family.
- [account.go:843-903](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L843) retains explicit model mappings/allowlists with exact and wildcard matching. Empty mappings do not treat missing discovery records as denial. Its explicit passthrough mode is a separate policy; do not copy its whitelist bypass into Aether.
- [openai_images_direct.go:30-49](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_images_direct.go#L30) still enumerates direct-call models: 1.5, 2, 2.5-flare/sunburst and dated 2.5 snapshots. Other recognized image models use its Responses image-tool bridge. Thus sub2api is not automatically compatible with all future direct-image protocols.
- Official [ImagesClient, endpoint/images.rs:63-102](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/endpoint/images.rs#L63) directly POSTs provider-relative `images/generations` and `images/edits`; [images.rs:5-30](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/images.rs#L5) represents `model` as a string and edits as JSON image references.

Minimum Aether implication: derive eligible configured image model IDs from active image endpoint bindings, apply the known-Free restriction and existing key filters, and preserve the chosen real model ID. Do not grow a fixed-name catalog or change explicit whitelist semantics. A new protocol still requires adaptation; a new name using the existing protocol should not.

### 3. Alpha Search: independent native JSON protocol

Official [SearchClient, endpoint/search.rs:14-45](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/endpoint/search.rs#L14) POSTs `alpha/search`. [SearchRequest, search.rs:8-65](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/search.rs#L8) contains `id/model/reasoning/input/commands/settings/max_output_tokens`; commands include search/image search/open/click/find/screenshot/finance/weather/sports/time. [SearchResponse, search.rs:295-305](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/codex-api/src/search.rs#L295) preserves `encrypted_output`, `output`, and opaque structured `results`.

sub2api's native path provides concrete wire rules:

| Concern | Implemented behavior and source |
| --- | --- |
| OAuth URL | `https://chatgpt.com/backend-api/codex/alpha/search`; account-type dispatch in [openai_alpha_search.go:669-687](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L669). |
| Request headers | Provider credentials/account identity; JSON Content-Type and Accept; preserve turn metadata and Codex version/originator/User-Agent. Dedicated builder at [355-429](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L355). |
| Remove Responses-only headers | `OpenAI-Beta`, `Session_ID`, `Conversation_ID`, `X-Codex-Beta-Features`, `X-Codex-Turn-State`, and its Responses Lite header, even after account overrides: [432-454](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L432). |
| Known contaminating body fields | Remove `prompt_cache_key`, `prompt_cache_retention`, `store`; otherwise preserve the alpha body: [464-495](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L464). |
| Tool endpoint errors | Search 401/404/405 do not permanently invalidate the whole account: [92-113](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L92), [543-551](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L543). |

The PAT branch is **not ordinary OAuth evidence**: [66-71](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L66) documents PAT `no_matching_rule` and routes PAT to a Responses hosted-search bridge. That bridge serializes alpha commands into a prompt and synthesizes output/citations ([290-345](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L290), [555-565](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_alpha_search.go#L555)); it does not prove full native alpha semantics. Do not silently apply this fallback to all Aether OAuth requests.

Official [spec_plan.rs:1038-1046](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L1038) activates standalone search using provider web-search capability and Responses Lite/feature settings, with no plan table in that gate. Also, `supports_search_tool` means deferred **tool discovery**, not proof of Alpha Search entitlement: compare [370-404](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L370) and [653-655](https://github.com/openai/codex/blob/75e0e0aad97a86138b8b1ec87d9b544b4a35ecbf/codex-rs/core/src/tools/spec_plan.rs#L653).

### 4. Responses, Compact, Live and failure scope

- Compact: sub2api has explicit supported/unsupported/unknown capability plus manual mode; unknown remains eligible until observed/configured otherwise ([account.go:906-951](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L906)). This is not plan gating.
- Live: sub2api's gate permits OpenAI OAuth and excludes PAT/Agent Identity ([account.go:1859-1863](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L1859)). That is credential-shape eligibility only. No verified Free/Plus/Pro entitlement matrix or Aether live-account success was obtained. **Unknown Live support must remain unknown; text discovery or a Responses success cannot create positive Live evidence.**
- Embeddings: the same source permits only API-key accounts ([account.go:1883-1885](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/account.go#L1883)). “All capabilities” does not imply all OpenAI API endpoints are served by a Codex subscription.
- Images: actual upstream structured `image_generation_unavailable` can affect image capability; a model's plain-text refusal is not durable account evidence ([openai_images_responses.go:2022-2058](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/openai_images_responses.go#L2022)). sub2api tests distinguish image-only rate limits from whole-account blocks ([ratelimit_service_openai_image_test.go:64-75](https://github.com/Wei-Shaw/sub2api/blob/a3eb7ef302961cba716dc78b39b93b60c467db0e/backend/internal/service/ratelimit_service_openai_image_test.go#L64)). Reuse Aether's existing scoped error/quota machinery rather than introducing a separate policy store solely for these fixes.

## Files found and related specs

| Path | Role |
| --- | --- |
| `.trellis/spec/aether-model-fetch/backend/quality-guidelines.md` | Existing image supplement projection, active endpoint/format filters, manual whitelist and native catalog isolation contracts; fixed default-name supplement needs revision by the owning implementation/spec agent. |
| `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md` | Native unknown-field/event preservation, credential isolation, compact and non-2xx behavior. |
| `.trellis/spec/aether-provider-transport/backend/index.md` | Transport ownership and links to the Responses/quota contracts. |
| sub2api `account.go`, `openai_account_scheduler.go` | Model/endpoint eligibility versus subscription scheduling preference. |
| sub2api `openai_images*.go`, `openai_alpha_search.go` | Separate native/bridge protocols and scoped failures. |
| official `protocol/src/auth.rs`, `core/src/tools/spec_plan.rs`, `codex-api/src/{endpoint/,}search.rs`, `codex-api/src/{endpoint/,}images.rs` | Raw plan semantics, actual Free restriction and current wire contracts. |

## Caveats / Not Found

- Parent cross-check of official `standalone_web_search_enabled` (same pinned SHA, lines 1038-1046) confirms only provider `web_search`, namespace tools, and `use_responses_lite` **or** the standalone-search feature switch. It has no Free/Plus/Pro table and no `supports_search_tool` condition. Aether may project configured active/permitted native Search to genuinely discovered Codex text models as routable candidates; this is not a claim of guaranteed upstream entitlement. Do not block Alpha Search using the unrelated deferred-tool-discovery flag.

- No production account was accessed or exercised; Plus/Pro/model-specific outcomes and Live availability remain unverified.
- No complete public subscription-to-endpoint/model matrix was found. Do not infer one from subscription marketing, a display badge, quota exhaustion, or sub2api's priority category.
- Official docs were fetched directly because the web search tool returned HTTP 503. GitHub source links are pinned to resolved immutable commits. `gh api` large aggregate source output omitted part of one file; final cited function locations were cross-checked against direct raw GitHub retrieval.
- Historical memory was used only to preserve the prior “avoid hard-coded special model names” constraint (MEMORY.md:3202); current local specs and live upstream source were checked for the report.
