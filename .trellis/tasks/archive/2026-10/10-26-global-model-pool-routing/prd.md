# Global model routing and pool preselection repair

## Goal
Restore the Antigravity Gemini route for `claude-opus-4-6-thinking` while preserving `claude-opus-4-6`, and prevent one pool representative's model list from rejecting other eligible accounts.

## Confirmed evidence
- Remote `models` has `uq_provider_model UNIQUE(provider_id, provider_model_name)` and one `global_model_id` per row.
- Before repair, Antigravity upstream `claude-opus-4-6-thinking` was bound to global `claude-opus-4-6` on `gemini:generate_content`; the exact thinking global had only Maas routes.
- PostgreSQL selects a single pool representative by key priority/id. Scheduler enumeration then applies that representative's `allowed_models` before the pool group exists.
- Actual scheduler reproduction: representative supporting only Opus5.5 yields zero Opus4.6 candidates; representative supporting Opus4.6 yields an Antigravity candidate.
- Actual account permissions must remain account-scoped, including explicit empty-list denial and NULL unrestricted behavior.

## Requirements
1. Explain current one-global association semantics and implement an evidence-backed route for both names without stealing a global name or inventing upstream models.
2. Do not weaken non-pool account permissions or per-member pool validation.
3. Preserve model mapping priority, endpoint/operation scopes and request directive policy.
4. Restrict production mutations to the requested model associations; no broad schema change, deploy, or real-provider billable request without explicit authorization.

## Acceptance
- Both global names retain the intended Gemini path to real upstream `claude-opus-4-6-thinking`.
- A pool whose representative lacks a model can still use another supporting account.
- A pool with no eligible account sends no request; non-pool forbidden models remain denied.
- Real loopback HTTP smoke observes correct account/model selection and exact-global routing.
- Relevant regression checks and affected contract documentation are updated.

## Design decisions
- Keep the single-global schema and exact-global priority. Preserve bare routing with a separate configured bare-name row whose unconditional provider mapping selects the real thinking upstream; associate the existing thinking-name row with the exact thinking global. Explicitly bind both rows to the existing Gemini Endpoint.
- Change only logical pool-group enumeration. Generic model resolution and actual account validation remain strict even when account rows carry the pool-enabled flag.
