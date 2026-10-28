# Antigravity Account-Scoped Dynamic Thinking Models

## Goal

Keep a stable client model alias while selecting the Antigravity thinking variant requested by each client, including compatible `xhigh` and `max` inputs.

## Background

Current scheduler mapping selects an opaque model ID without request effort. Antigravity envelopes retain that ID, so a default mapping to `-high` is not changed by a client's low thinking configuration. Existing account-discovery and quota isolation rules must remain authoritative.

## Requirements

- R1: Read effort from native Gemini `generationConfig.thinkingConfig.thinkingLevel`, OpenAI Chat `reasoning_effort`, and Responses `reasoning.effort`; preserve existing cross-format conversion and native-format bypass.
- R2: Normalize case/whitespace and the existing effort vocabulary. `high`, `xhigh`, `max`, and `extra` select the high variant. `low` and `medium` retain their corresponding tier. `none` and `minimal` select a discovered minimal/extra-low variant where present, otherwise the low compatibility tier. Do not approximate medium/high with lower tiers.
- R3: A client-specified thinking suffix has precedence over a conflicting body effort. Only a complete exact discovered model identity may bypass variant selection. A default mapped suffix is not an explicit client choice. No effort preserves the configured default mapping.
- R4: Select solely from the actual account's first authoritative Antigravity model map (`quota_by_model`, `models`, `model_quotas`), honor empty maps and disabled entries, and never fabricate or manually supplement model IDs. Keep family/version boundaries exact after removing only recognized terminal effort suffixes.
- R5: Resolve the actual model before ordinary and Pool/sticky discovered-model admission, quota/runtime-quota filtering and frozen windows. Re-resolve on account changes and prevent cached high results from serving low requests or vice versa.
- R6: The final model must agree across candidate/plan, Antigravity envelope, request thinking level, usage attribution and model-scoped quota feedback. Missing requested variants skip the account, not silently fall back to another model/tier.
- R7: Preserve administrator allowlists, unrelated provider behavior, default mapping with no effort, and models without a discovered thinking-variant family. No persistent schema/API changes or new dependency packages.

## Acceptance Criteria

- AC1: One alias mapped to high sends the account's low, medium, or high model when requested through native Gemini, Chat, or Responses; xhigh/max send high without unsupported literal Gemini levels.
- AC2: An explicit low model overrides body high; requests without effort retain the configured default; missing/disabled variants fail account admission rather than changing tiers.
- AC3: Ordinary and Pool/sticky selection skips accounts lacking the requested variant or with that variant exhausted, while another eligible account serves it. High quota exhaustion does not block low.
- AC4: Consecutive mixed-effort requests and account rotations preserve correct selection with warm caches and retain unrelated payload fields.
- AC5: Targeted behavior/regression tests and real HTTP smoke exercise final upstream envelope and client-visible completion. Rust format/Clippy and affected suites pass.

## Out of Scope

- No UI redesign, new user-facing mapping editor, catalog seeding, subscription inference, provider-independent routing rewrite, release tag or deployment.
- No new retry/telemetry framework, unrelated reasoning-budget heuristics, or changes to Claude thinking behavior.
