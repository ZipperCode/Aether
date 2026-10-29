# Design

## Request selection

Extend the existing exported `antigravity_request_thinking_level` reader with the requested model name. Move the existing client-suffix precedence into that owner, then decode only the format-native body setting. A bare Gemini name with no explicit setting yields Medium. An explicit malformed setting is not replaced by the default. Preserve the numeric budget compatibility added in the preceding turn.

Both routed and static gateway request contexts call this reader before preselection. Remove the gateway-only decoder wrapper so suffix/body/default semantics have one owner. Existing Some(level) selection gates then defer static Key model permission, bypass effort-sensitive resolved caches, and re-resolve each actual account before discovery/quota filtering.

## Account and wire boundaries

Reuse `resolve_provider_pool_key_thinking_model` unchanged: only account-discovered variants may be selected, missing/disabled medium rejects the account and no alternate tier is fabricated. Its None input remains the low-level explicit no-selection contract; gateway bare Gemini requests now carry Some(Medium).

At the final private-envelope boundary, suffix-derived levels still win. For bare Gemini 3 wire models, apply the same reader so a base-only discovered model receives medium without changing its identity; keep legacy budget-only Gemini models and non-Gemini bodies unchanged. Do not inject a numeric budget alongside a newly selected level: medium is a thinkingLevel, not a string-valued thinkingBudget.

The same pre-rule projection handles native suffixed aliases that resolve to discovered bare Gemini 3 identities. Defer endpoint rules only for that narrow case, apply the shared in-place selection, and then execute the original rules once before configured directive patches. Native requests still bypass canonical conversion. Enabled Antigravity endpoint rules retain the current unsupported-transport classification; this task does not add live rule support.

OpenAI nullable effort fields are semantically absent and use the same medium default. Before endpoint body rules and directive patches, converted requests reuse the already-decoded source selection: recover omitted `extra-low` and apply explicit client suffixes even over valid conflicting body effort. This in-place recovery adds no payload clone. At the envelope boundary, unresolved explicit effort without an effective target tier fails rather than silently defaulting to medium; configured effective target fields remain authoritative. Keep pure conversion and OpenAI-to-OpenAI custom-effort passthrough unchanged. Converted completions retain the established suffixed client display-model contract even when the actual discovered upstream identity is bare.

## Compatibility

This intentionally replaces the prior no-effort default-high behavior for bare Gemini client names. Explicit client suffixes/levels/budgets remain authoritative. Other provider types ignore this Antigravity-only selection state. No persistent/configuration migration or production mutation is involved.
