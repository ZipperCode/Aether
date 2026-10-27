# 修复 Antigravity 逐账号模型准入误放行

## Goal

Prevent requests from reaching Antigravity accounts that do not expose the requested upstream model, while preserving successful native serving and rotation through eligible accounts.

## Background

Incident `b92c3b1f-7aab-4f68-9e55-8340499cc1f3` on `192.168.2.212` produced six upstream HTTP 404 `Requested entity was not found.` responses across five Pro accounts. Each had `auto_fetch_models=false`, `allowed_models=null`, and account discovery metadata exposing only `gemini-4-argon-disabled` for Argon. The Provider mapping was `gemini-4-argon` to `gemini-4-argon-high`. An eligible Ultra account timed out; direct Gemini ultimately completed with HTTP 200 and `STOP`. No empty request content or format conversion was involved.

Current `crates/aether-scheduler-core/src/model.rs:272-323` treats a null account allowlist as unrestricted. Actual Pool members reuse that resolver through `apps/aether-gateway/src/dispatch/pool_scheduler.rs:1643-1655`. Model quota and model entitlement are separate facts; missing quota is not exhaustion.

## Requirements

- R1: Resolve admission against each serving account's fetched/discovered upstream model evidence. Never use another account's catalog or infer support from provider-wide mappings or a subscription tier.
- R2: Unsupported Antigravity accounts must be skipped before upstream generation, including ordinary and actual Pool/sticky candidates. Continue to eligible siblings; preserve exact endpoint and native-format handling.
- R3: Disabling automatic model fetching must not turn an unsupported model into unrestricted access, and must not block models other accounts legitimately support.
- R4: Preserve authoritative empty discovery, exact model identity, existing explicit account restrictions, recoverable quota behavior, and all unrelated provider behavior. Do not add hardcoded models, tier fallback lists, schema changes, or broad null-allowlist restrictions for custom API-key providers.
- R5: Keep eligibility diagnostics distinguishable from quota exhaustion, with regression coverage and a real isolated localhost HTTP serving proof.

## Acceptance Criteria

- [x] A failing-before/passing-after scenario reproduces the incident-shaped unsupported-account admission.
- [x] A Pro-shaped account exposing only a disabled placeholder receives zero upstream generation requests for the mapped available-model name, even with a null allowlist and auto-fetch disabled.
- [x] An eligible sibling serves the mapped upstream model natively and the client receives a completed HTTP 200 response; no conversion toggle change is required.
- [x] Ordinary, Pool and sticky decisions use the same account-specific model evidence; no model-list union leaks across accounts.
- [x] Explicit empty/restricted catalogs, exact variant matching, and discovery update behavior retain deterministic semantics; unrelated providers retain null-allowlist behavior.
- [x] Relevant Rust tests/checks and a real localhost HTTP smoke pass; affected contract documentation is updated after smoke proof.

## Out of Scope

- Changing retry, watchdog timeout, subscription ranking, quota exhaustion semantics, request conversion, upstream credentials/projects, deployment configuration, or production data.
- Redeploying `192.168.2.212` or issuing paid/real Google generation requests without separate deployment authorization.

## Verified Evidence

- Failing-before real localhost HTTP regression: unsupported account A received two generation attempts before fallback to B; assertion `(2, 0) != (0, 0)` failed.
- Passing-after ordinary and Pool runs: sync and native SSE both returned HTTP 200 with selected `gemini-4-argon-high`, SSE `STOP`, and zero unsupported generation attempts.
- Successful admin discovery refresh changed the eligible account: stale sticky B was skipped and rebound to A. When both catalogs excluded the model, HTTP 503 returned without further generation attempts.
- Provider-pool suite: 135 passed, none skipped. Allocation-free disabled-flag parsing retained existing boolean-string semantics.
- Final `python3 tools/ci.py preflight`: formatting and Gateway Clippy passed; Gateway lib/bin nextest ran 5,709 tests successfully, with 3 skipped.
- Provider-pool all-targets Clippy passed with `-D warnings`; review found no blocking defects.
- Production deployment and real Google generation were not performed.
