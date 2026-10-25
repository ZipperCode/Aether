# Design

Reuse scheduler-core model resolution, not provider-specific suffix rewriting. Pool groups remain logical provider/endpoint/model bindings; actual account hydration resolves selected_provider_model_name and mapping_matched_model from the member row before transport permission checks and request construction. Preserve endpoint/operation-scoped provider mappings and the existing per-format model-directive routing policy; do not unconditionally enable directive stripping.

Explicit allowed-model lists are authoritative, including denial for empty lists. NULL preserves unrestricted existing fallback; no model IDs are invented from a regular expression or hardcoded provider suffix. Score pages, ordinary pages, routing-allowed keys and sticky members must converge on the same member resolution. Quota windows and reserve thresholds must consume the resolved member model, not a representative's model.

The in-memory candidate repository must mirror PostgreSQL's logical pool representative projection while group-key expansion retains every member row; otherwise HTTP regressions accidentally enumerate one logical group per account and hide the production failure.

Remote investigation is read-only SQL and bounded container logs on192.168.2.212; no secrets or business payloads are emitted. Classify candidate failures independently from final usage status. Production deployment is excluded.
