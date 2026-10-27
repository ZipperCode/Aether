# Design

## Boundary

Account entitlement is separate from model quota and the user-configured allowlist. Add one borrowed, allocation-free Antigravity discovered-model eligibility helper in `aether-provider-pool`. It reads the selected Key's `upstream_metadata.antigravity` and chooses the first object among `quota_by_model`, `models`, `model_quotas`; even an empty object is authoritative. Require trimmed, case-insensitive exact identity of the selected upstream model; do not infer model families or subscription privileges. Explicitly disabled entries are not available.

The helper only adds discovered-model evidence: existing scheduler allowlist/mapping rules continue to apply independently. Missing model discovery retains existing pre-discovery behavior; other Provider types remain unchanged. A null `allowed_models` cannot override a discovered catalog excluding the requested target. No DTO, persistence, schema, model-fetch, import, or response contract changes are required.

## Integration

- Ordinary candidates: consume existing strong Key reads in `CandidateRuntimeSelectionSnapshot.provider_key_rpm_states` inside `current_candidate_runtime_skip_reason`. Return existing `key_model_disabled` for an account that does not expose the selected upstream model. Do not apply a representative Key's discovery restriction to a logical PoolGroup.
- Actual Pool/sticky members: add the same helper to `schedule_pool_page_candidates` after strong Key hydration, before final window truncation and hot-pool fallback, using each member's resolved model. Preserve existing quota/auth/balance/cooldown diagnostic priority.
- No transport helper import: `aether-provider-pool` already depends on `aether-provider-transport`; a reverse dependency would create a cycle. Current task fixes the actual scheduling path without broad transport refactoring.
- Successful quota/discovery refresh already invalidates candidate caches; strong Key reads make current discovery authoritative without adding another cache.

## Verification

Reuse the existing Antigravity localhost HTTP integration fixture. Reproduce the incident with a high-priority null-allowlist/auto-fetch-disabled account exposing only an unrelated disabled Argon placeholder and a supported sibling exposing the exact mapped upstream name. The unsupported account must receive zero generation requests; the sibling must serve native Gemini and complete HTTP 200. Cover ordinary candidates, Pool membership, sticky rebinding on discovery changes, explicit empty discovery, absent discovery fallback, and exact variants. Do not issue real upstream requests.

## Compatibility and Risk

A present discovered catalog is now a restriction, including when automatic model fetching is disabled. Existing allowed-model restrictions remain intact, and no global model union or tier inference is added. This intentionally stops unsupported Antigravity attempts rather than retrying HTTP 404 or inventing quota exhaustion. Production deployment is separate and not performed by this task.
