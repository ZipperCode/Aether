# Antigravity 签名失败恢复

## Goal
Implement approved error-triggered recovery for Antigravity Gemini instead of repeated key failures hidden by a successful fallback provider.

## Requirements
- First send preserves all original signatures. Gate on explicit Antigravity Gemini adaptation, actual HTTP 400, error.status=INVALID_ARGUMENT, and Corrupted thought signature (ignoring surrounding whitespace/trailing periods).
- Only then clone candidate body, replacing existing thoughtSignature / thought_signature on historical role=model message parts with skip_thought_signature_validator. Preserve every message, thought text, tool call/result, parameter and order; do not change same-named keys in tool/user data.
- Retry only if body changes, exactly one compatibility retry on same provider/endpoint/key/auth/URL/model without rerouting or refreshing credentials for signature recovery.
- No rewrite or repeated signature rejection selects request-local Provider scope if existing failover allows; StopLocalFailover stops. Other errors follow existing policy.
- Cover direct/tunnel stream/sync plus fixed-target admin model tests. Reuse effects and execution owners; no generic retry framework/config/cache/heuristic.
- Retain remaining timeout and one logical candidate lifecycle, unique terminal accounting/lease effects. Trace first error and recovery result; capture final actual upstream body with original client body unchanged.
- No public API/schema/frontend changes, production mutations/deployment/restarts/live inference.

## Acceptance
- [x] Gate exclusions, scope-only rewrite, aliases, native/public body, immutability and no-op covered.
- [x] Original first send / compatible same-account second send / one retry on real sync/stream direct/tunnel and admin paths.
- [x] Repeated rejection skips provider only for current request; stop and unrelated errors unchanged.
- [x] Actual sent-body capture, initial error/recovery trace, shared deadline and unique terminal accounting verified.
- [x] Relevant existing Antigravity schema/Gemini signature tests and scoped Rust checks pass with nonzero selected counts: Docker during implementation and exact-SHA GitHub CI for the release (see `research/release-v0.7.35.md`).
- [x] Independent check and honest local-vs-live handoff.

## Confirmed tradeoff
User selected error-triggered compatibility recovery and explicitly requested the complete plan. Rejected historical reasoning state is not reused; visible chat/tools remain and further reasoning may consume more tokens. Normal successful requests never degrade. This supersedes the earlier lossless-only research gate, not the rejection of unconditional signature deletion.
