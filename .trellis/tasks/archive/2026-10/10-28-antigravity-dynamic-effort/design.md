# Design

## Boundaries

- `aether-provider-transport::antigravity` owns a small Copy `AntigravityThinkingLevel` (Minimal, Low, Medium, High), effort decoding, recognized Gemini effort-suffix parsing, and private-envelope level normalization. Existing ReasoningEffort semantics are reused where useful; no public Gemini or Claude conversion rewrite.
- `aether-provider-pool` owns `resolve_provider_pool_key_thinking_model`: borrowed input/output and authoritative account-map selection. It reuses the current first-map precedence and disabled-entry policy rather than maintaining a second catalog.
- Gateway request planning carries the optional normalized level together with the original client model. Ordinary candidates are resolved before runtime snapshots; Pool placeholders wait until the actual member Key is known, including sticky reserve-quota checks. Account changes always re-run the same resolver.

## Shared API

Transport exports from its existing `antigravity` module:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityThinkingLevel { Minimal, Low, Medium, High }
impl AntigravityThinkingLevel {
    pub fn parse(value: &str) -> Option<Self>;
    pub fn as_str(self) -> &'static str;
}
pub fn antigravity_request_thinking_level(api_format: &str, body: &serde_json::Value)
    -> Option<AntigravityThinkingLevel>;
pub fn antigravity_model_thinking_variant(model: &str)
    -> Option<(&str, AntigravityThinkingLevel)>;
```

Only `gemini-` model IDs have recognized terminal suffixes. Check `-extra-low` before `-low`; also support `-none`, `-minimal`, `-low`, `-medium`, `-high`, `-xhigh`, `-max`, `-extra`. Extended high spellings normalize to High; none/minimal/extra-low normalize to Minimal. Do not strip arbitrary suffixes or infer another version. The request reader uses only the matching format's native field (Gemini also accepts an existing private envelope's inner request).

Pool exports:

```rust
pub fn resolve_provider_pool_key_thinking_model<'a>(
    key: &'a StoredProviderCatalogKey,
    provider_type: &str,
    requested_model: &str,
    selected_provider_model: &'a str,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> Option<&'a str>;
```

For unrelated providers/models return the original selected name. With an authoritative map, an empty map rejects. Recognized client suffix wins over body effort. An exact discovered client variant in the selected family retains its exact identity and disabled status. Otherwise select the requested tier within the selected model's exact family and return the discovered map key. Minimal prefers a present minimal variant (disabled means rejection), using low only when no minimal variant exists. Missing medium/high/low never falls back. No effort retains the configured default. Models with neither a mapped suffix nor discovered siblings retain existing base-model behavior. Absent discovery retains existing behavior without inventing a target.

## Request and Cache Flow

Decode after client-request routing mutations so routing rules remain authoritative. Use original/effective client model for suffix precedence, not the suffix on the default mapping. Propagate through ordinary selection and lazy Pool cursor construction. Keep source-row caches request-independent; resolved-candidate/page caches must distinguish effort and explicit model identity or be conservatively bypassed for effort-sensitive requests. Scheduler affinity may retain a preferred account but must revalidate its actual model and quota on every request; it must never carry a cached selected model.

## Envelope Invariant

At the shared private-envelope boundary, Gemini suffix level determines the canonical `generationConfig.thinkingConfig.thinkingLevel`. A base-only Gemini model canonicalizes an existing recognized input level. Preserve unrelated fields and numeric budgets. Claude payloads remain unchanged. This shared point covers native Gemini, converted Chat/Responses, and already enveloped requests.

## Compatibility and Rollback

No persistence or external API schema changes, no discovered catalog additions, no new package dependencies. Existing static mappings remain the no-effort default. Changes are code-only and reversible in one logical commit. Tests cover authoritative maps, account isolation, suffix precedence, mixed-effort cached requests and final wire envelopes.
