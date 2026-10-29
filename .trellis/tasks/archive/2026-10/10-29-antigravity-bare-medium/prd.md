# Antigravity Bare Gemini Medium Default

## Goal

Serve a bare Gemini model name through Antigravity at medium thinking when the client supplies no thinking setting, instead of preserving a default-high mapping or rejecting the bare family.

## Requirements

- R1: A bare Gemini request with no explicit thinking level, budget, or recognized model suffix defaults to medium. Apply before account discovery, permission and quota admission in ordinary and Pool/sticky paths.
- R2: Explicit model suffix, native `thinkingLevel`, numeric `thinkingBudget`, Chat `reasoning_effort`, and Responses `reasoning.effort` retain their existing precedence and compatibility normalization.
- R3: Select the medium model only from the serving account's authoritative discovered model map. Missing/disabled/exhausted medium rejects that account and permits an eligible sibling; never substitute high/low or fabricate model IDs.
- R4: The final upstream model, thinking configuration and client-visible completion agree. Preserve unrelated payload fields, native-format bypass, administrator allowlists and account-local quota isolation.
- R5: Preserve non-Gemini and non-Antigravity behavior. No production deployment, new persistent data, fallback catalog, or unrelated retry/policy changes.

## Acceptance Criteria

- AC1: Bare `gemini-3.8-flash` without thinking settings completes on discovered medium in ordinary and Pool modes, including warm requests and native SSE.
- AC2: The default works when the provider mapping is bare or carries a default suffix; explicit low/high/suffix/budget still wins.
- AC3: An account missing, disabling or exhausting medium is skipped; an eligible account serves medium, and all-ineligible returns 503 with zero unsupported generation attempts.
- AC4: Focused regressions fail before the fix and pass after; actual localhost HTTP smoke observes medium envelopes and client-visible completion.
- AC5 (user-authorized completion): repair the existing Codex OAuth refresh-failure test's missing account discovery evidence without relaxing production admission or its assertions; pass the full Gateway gate, then commit and push the Codex fixture and Antigravity changes.

## Context

Prior-turn uncommitted changes add numeric-budget parsing in transport and an existing HTTP regression. They are part of the same compatibility path and must be preserved. The current repository includes committed Codex/Claude changes; do not revert or rework them.
