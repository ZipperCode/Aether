# Provider Model Association Endpoint Evidence Contract

## 1. Scope / Trigger

This contract applies whenever an admin UI or a trusted admin refresh creates a Provider Model for a Provider that may own multiple Endpoints. Every automatic path must carry authoritative upstream model and Endpoint evidence before it relies on backend inference.

## 2. Signatures

- Aggregate discovery: `fetchModels(providerId, undefined, false) -> { models: UpstreamModel[] }`
- Exact creation: `createModel(providerId, { global_model_id, provider_model_name, endpoint_ids? })`
- Compatibility fallback: `batchAssignModelsToProvider(providerId, globalModelIds)`
- Each discovered model is shaped as `UpstreamModel { id, api_formats, endpoint_ids? }`.

## 3. Contracts

- The provider-query backend aggregates duplicate upstream records by exact model ID and unions their `api_formats` and `endpoint_ids`.
- The Provider detail association dialog loads aggregate upstream models without requiring an extra Key-selection action.
- Exact, case-insensitive Global Model name matches may be selected automatically. Different names require an explicit user selection; prefixes and fuzzy guesses are forbidden.
- Exact creation uses the upstream `id` as `provider_model_name` and forwards its de-duplicated `endpoint_ids`.
- A quota-driven discovery path constructs the complete admin import source and binds each discovered model to the exact Endpoint that produced the quota snapshot. It must not call Provider-level Endpoint inference.
- Internal/non-routable model IDs are filtered through one shared, case-insensitive predicate before any catalog write. Quota success remains successful if the best-effort catalog synchronization fails.
- Saving is unavailable while the initial aggregate query is pending. Async results may update state only when Provider ID, open state, and dialog session still match.
- If discovery returns no usable model, the existing batch inference path remains available for single-Endpoint Providers, explicit metadata, and Providers that do not publish a model list.

## 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| Exact upstream name with Endpoint IDs | Use exact creation; do not invoke batch inference |
| Different upstream and Global Model names | Require explicit upstream selection |
| Aggregate query pending | Disable and guard save |
| Aggregate query empty or failed | Keep the compatibility fallback |
| Response belongs to an old dialog session | Discard it without changing current state |
| Endpoint ID is empty, duplicated, or foreign | Normalize duplicates in the UI; backend validation rejects empty or foreign IDs |
| Quota refresh discovers a routable model | Import with the refresh Endpoint ID as authoritative evidence |
| Quota refresh discovers an internal model | Exclude it before catalog import, case-insensitively |
| Quota succeeds but catalog import fails | Keep quota success and report only a warning |

## 5. Good / Base / Bad Cases

- Good: `gemini-3.7-flash -> gemini-3.7-flash` automatically creates the Provider Model with the discovered Endpoint IDs.
- Base: a Provider with no published upstream list continues through existing backend inference.
- Bad: `gemini-3.8` must not silently choose `gemini-3.8-flash-high`; the user selects that mapping explicitly.

## 6. Tests Required

- Default path: open the dialog, wait for aggregate discovery, select an exact same-name Global Model, save, and assert the complete `createModel` payload plus absence of batch inference.
- Compatibility path: return no upstream models and assert the original batch assignment call.
- Race path: keep discovery pending and assert save is disabled and guarded.
- Session path: resolve an older request after reopening and assert that only current-session models are rendered.
- Contract path: frontend type-checking must include `endpoint_ids` on the provider-query response type.
- Admin quota path: assert the complete import source, exact Endpoint ID, internal-model exclusion, and non-fatal repository failure behavior.

## 7. Wrong vs Correct

### Wrong

Test only a hidden or optional Key-selection path, then assume the normal “select and save” flow has the same Endpoint evidence.

### Correct

Load aggregate upstream evidence on the default path, guard saving until it settles, and keep manual Key selection only as an explicit refresh or disambiguation tool.
