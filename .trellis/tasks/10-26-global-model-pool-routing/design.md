# Design

## Association repair
Existing UNIQUE(provider_id, provider_model_name) plus single models.global_model_id means a row cannot belong to multiple globals. Reuse existing provider_model_mappings rather than alter cardinality or exact-global precedence. Create a distinct configured bare-name row under global claude-opus-4-6 with an unconditional mapping to discovered claude-opus-4-6-thinking and explicit Gemini endpoint. Then retarget original real thinking-name row to the thinking global. Preserve existing overrides, active flags and Gemini binding. Both routes send the real thinking model even for unrestricted accounts. No model IDs are added to account allowed_models. Do not use batch inference that omits the explicit mapping. Production changes require a supported authorized mutation path and cache coherence; otherwise retain a precise blocker and do not claim live repair.

## Pool boundary
Only logical group enumeration may ignore the representative's model-list denial. Keep the generic account resolver strict because expanded account rows retain provider_pool_enabled=true. Resolve against the representative normally; when resolution fails for a pool representative, reuse scoped configured/default model resolution without account permission filtering. Preserve endpoint/operation scope. Actual member hydration remains authoritative, including empty-list denial. Non-pool enumeration unchanged. No SQL/schema change needed.

## Verification
Regression must fail before the patch. Real loopback gateway HTTP observes zero upstream sends from incompatible representative, success through supporting sibling and actual model ID; all-incompatible pools send nothing. Exercise two exact globals using separate rows and shared real target, verify Gemini routing payload/bindings. No real-provider traffic, broad conversion enablement, or production restart/deploy in this task.
