# Codex OAuth account model eligibility

## Goal
Only send Codex OAuth text requests to accounts whose own successful remote catalog contains the final upstream model. Preserve basic-model service by free accounts and existing configuration permissions.

## Requirements
- Strict first discovery: missing, malformed or empty account evidence denies text generation with `key_model_disabled`.
- Evidence must contain only raw account-local remote IDs, never aliases, bundled cards, presets, locked models or image enrichment.
- Successful refresh replaces evidence; failure preserves last successful evidence. Existing persistence and routing invalidation paths remain authoritative.
- Cover native Responses, Compact and Search; leave non-OAuth, images and Live unchanged.
- Enforce on real account candidates, ordinary scheduling, pool pages/sticky, admin fixed-account tests and each WebSocket turn.
- No production credentials, external account calls, dependencies, Docker, schema migration or production setting changes.

## Acceptance
Real loopback gateway/upstream scenarios prove paid-only advanced execution, free basic execution, locked/alias isolation, missing/failed-first discovery zero-send denial and recovery, failed-refresh LKG, successful removal/recovery invalidation, ordinary sync/SSE, WebSocket per-turn isolation, fixed-account admin denial/success and versioned catalog bootstrap. Two accounts fetch independently. Existing Codex image/Live and Antigravity regressions remain valid. Run specified transport/model-fetch/Gateway checks and independent curl streaming smoke with observed per-account counts.

## Approved execution source
User approved the complete inline plan; recovery/handoff copy: `local://codex-account-model-eligibility-plan.md`.
