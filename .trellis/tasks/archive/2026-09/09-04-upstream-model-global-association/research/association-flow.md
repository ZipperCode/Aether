# Research: upstream model to Global Model association flow

- Query: Trace the Provider detail “关联模型” flow and define the smallest contract/UI change that lets one real upstream model freely target a Global Model while retaining Endpoint bindings and custom mappings.
- Scope: internal
- Date: 2026-09-04

## Findings

### Current flow and root cause

- `ModelsTab` only renders the Provider models passed by `ProviderDetailDrawer` and emits `batchAssign` when “关联模型” is clicked; it does not load Global Models or upstream models itself (`frontend/src/features/providers/components/provider-tabs/ModelsTab.vue:1-18`, `:307-319`, `:551-554`). The drawer loads `providerModels` with `getProviderModels`, opens `BatchAssignModelsDialog`, and refreshes Provider/endpoints/mapping preview after changes (`frontend/src/features/providers/components/ProviderDetailDrawer.vue:797-822`, `:920-928`, `:3177-3195`, `:4146-4158`).
- `BatchAssignModelsDialog` opens by loading all Global Models, existing Provider Models, and Provider keys in parallel (`frontend/src/features/providers/components/BatchAssignModelsDialog.vue:493-548`). It stores selection only as `Set<global_model_id>` (`:230-239`) and displays only the Global Model catalog (`:67-139`).
- Upstream models are fetched only after the user chooses a key from the “按密钥匹配” menu (`frontend/src/features/providers/components/BatchAssignModelsDialog.vue:22-64`, `:352-407`). The result is immediately reduced to a set of upstream IDs, then compared to `GlobalModel.name` using trim-only exact equality (`:339-340`, `:373-380`). The actual `UpstreamModel` objects, including `endpoint_ids`, are discarded.
- Save submits only newly selected Global Model IDs (`frontend/src/features/providers/components/BatchAssignModelsDialog.vue:428-463`; `frontend/src/api/endpoints/models.ts:101-123`). The backend request contains only `global_model_ids` (`apps/aether-gateway/src/handlers/admin/provider/shared/payloads.rs:341-344`), and the builder creates each Provider Model with `provider_model_name = global_model.name` and no explicit Endpoint IDs (`apps/aether-gateway/src/handlers/admin/request/models.rs:962-1017`). Therefore `gemini-3.8-flash-high -> gemini-3.8` cannot be expressed; Endpoint inference then tries the wrong Provider Model name and can produce “无法推断 Endpoint”.
- Explicit Endpoint binding already works in the shared create path: provided IDs are validated as non-empty, unique, and owned by the Provider, while omitted IDs invoke inference (`apps/aether-gateway/src/handlers/admin/request/models.rs:114-164`, `:181-245`). No data-layer or migration change is needed.
- The upstream contract already has the needed runtime data: `UpstreamModel` contains `id`, `api_formats`, and `endpoint_ids` (`frontend/src/api/endpoints/types/model.ts:321-334`), and model-fetch attaches Endpoint IDs from matching API formats when upstream output lacks them (`apps/aether-gateway/src/model_fetch/runtime.rs:621-649`). The duplicate response declaration in `frontend/src/api/admin.ts:577-589` omits `endpoint_ids`; add that field (or directly reuse `UpstreamModel`) so the frontend boundary is truthful.

### Recommended minimum user interaction

Keep the existing key selector and fetch mechanism, but retain the returned `UpstreamModel[]` and render upstream rows. For each selected upstream row:

1. Show the immutable real upstream ID (for example `gemini-3.8-flash-high`).
2. Provide a `Select` over all loaded Global Models; preselect an exact same-name match only as a convenience, never as a requirement.
3. Submit `provider_model_name = upstream.id`, the freely selected `global_model_id`, and `endpoint_ids = upstream.endpoint_ids`.

Use the upstream model ID as the row/selection identity, not the Global Model ID. Preserve the current one-Provider-Model-per-Global-Model behavior by disabling Global Models already associated or already chosen in the pending batch. Additional upstream names for the same Provider Model remain aliases managed by `ModelMappingDialog`.

This can stay inside the current dialog and existing UI primitives; no new component, endpoint picker, fuzzy matcher, or model-fetch logic is required.

### Minimum API extension

Keep `POST /api/admin/providers/{provider_id}/assign-global-models` and add a structured array while retaining `global_model_ids` as a legacy fallback:

```json
{
  "assignments": [
    {
      "global_model_id": "global-gemini-3.8",
      "provider_model_name": "gemini-3.8-flash-high",
      "endpoint_ids": ["endpoint-gemini"]
    }
  ]
}
```

- Add a small deserializable assignment item beside `AdminBatchAssignGlobalModelsRequest`; make both top-level arrays default empty. Convert legacy IDs to the current behavior (`provider_model_name = GlobalModel.name`, Endpoint inference) so existing callers remain valid.
- Change `build_admin_batch_assign_global_models_payload` to build the already-existing `build_admin_batch_assign_provider_model_record(..., provider_model_name)` and call `build_admin_provider_model_create_mutation(record, endpoint_ids, Some("manual"))`. This reuses existing validation and bound-create storage.
- Return enough identity for partial errors. Recommended success item: `global_model_id`, `global_model_name`, `provider_model_id`, `provider_model_name`, `endpoint_ids`; error item: `global_model_id`, `provider_model_name`, `error`. The current frontend type incorrectly names the backend’s `provider_model_id` as `model_id` (`frontend/src/api/endpoints/models.ts:107-116` versus `apps/aether-gateway/src/handlers/admin/request/models.rs:1033-1038`); correct it while touching the contract.
- `/models/batch` already accepts full `ModelCreate[]`, but switching to it would lose the current per-item `success/errors` contract. Extending the existing assignment endpoint is the smaller behavior-preserving change.

### Custom mapping semantics remain separate

`ModelMappingDialog` chooses an existing Provider Model as the client-visible target, then adds one or more upstream/custom names into that model’s `provider_model_mappings`; optional Endpoint and operation scopes are preserved and saved through `updateModel` (`frontend/src/features/providers/components/ModelMappingDialog.vue:11-38`, `:227-248`, `:349-359`, `:794-883`). `ProviderModelMapping` remains `{name, priority, api_formats?, endpoint_ids?, operations?}` (`frontend/src/api/endpoints/types/provider.ts:961-970`).

The association fix must set only the base `provider_model_name`, `global_model_id`, and model Endpoint binding. Do not rewrite `provider_model_mappings`, `ModelMappingDialog`, or the model-name matcher. Existing matcher tests already prove exact aliases and Global Model regex mappings remain available (`crates/aether-model-fetch/src/association_sync.rs:684-748`).

### Recommended minimum write set

- `frontend/src/features/providers/components/BatchAssignModelsDialog.vue` — retain upstream results, select a Global Model per upstream row, and submit structured assignments.
- `frontend/src/api/endpoints/models.ts` — type/send structured assignments and align the success field with `provider_model_id`.
- `frontend/src/api/admin.ts` — expose `endpoint_ids` on fetched upstream model entries (or reuse the existing `UpstreamModel` type).
- `apps/aether-gateway/src/handlers/admin/provider/shared/payloads.rs` — deserialize structured assignments plus legacy IDs.
- `apps/aether-gateway/src/handlers/admin/provider/models/assign_global.rs` — pass structured input to the builder.
- `apps/aether-gateway/src/handlers/admin/request/models.rs` — use the selected Provider Model name and explicit Endpoint IDs, preserving partial success/errors.
- Tests only in the two existing files below. No change is needed in `aether-admin`, `aether-model-fetch`, repositories, schema, or `ModelMappingDialog`.

### Minimum related tests

- Extend `frontend/src/features/providers/components/__tests__/BatchAssignModelsDialog.loading.spec.ts` (currently only verifies Global Models/existing models/keys load at `:87-109`) with one behavior case: fetched upstream `gemini-3.8-flash-high` carries `endpoint-gemini`, user selects Global `gemini-3.8`, and `batchAssignModelsToProvider` receives all three fields. Same-name auto-selection may be a second assertion in that case, not a separate suite.
- Add one focused integration test in `apps/aether-gateway/src/tests/control/admin/models/provider.rs`: post a structured mismatched-name assignment to a multi-Endpoint Provider, assert the stored Provider Model name and exactly one manual binding. Existing nearby tests show the required repository assertions for explicit binding (`:1104-1177`), while the existing assignment test covers only `global_model_ids` (`:1345-1441`) and should remain as backward-compatibility coverage.
- Existing `frontend/src/features/providers/components/__tests__/ModelMappingDialog.spec.ts` covers upstream loading and scoped mapping edits; run it unchanged as the regression check. The Provider form Endpoint tests (`frontend/src/features/providers/components/__tests__/ProviderModelFormDialog.endpoint-bindings.spec.ts:110-156`) are useful precedent but do not cover the batch path.

## Files Found

- `frontend/src/features/providers/components/provider-tabs/ModelsTab.vue` — association entry point and Provider Model list.
- `frontend/src/features/providers/components/BatchAssignModelsDialog.vue` — current Global-ID-only association UI.
- `frontend/src/features/providers/components/ModelMappingDialog.vue` — separate alias/custom mapping editor.
- `frontend/src/features/providers/composables/useUpstreamModelsCache.ts` — existing upstream query facade used by the dialog.
- `frontend/src/api/endpoints/models.ts` — current assignment client contract.
- `frontend/src/api/endpoints/types/model.ts` — canonical `UpstreamModel` and Provider Model create types.
- `apps/aether-gateway/src/handlers/admin/provider/shared/payloads.rs` — request DTO ownership.
- `apps/aether-gateway/src/handlers/admin/provider/models/assign_global.rs` — assignment HTTP handler.
- `apps/aether-gateway/src/handlers/admin/request/models.rs` — assignment construction and Endpoint validation/inference.
- `apps/aether-gateway/src/tests/control/admin/models/provider.rs` — closest end-to-end admin model tests.

## External References

None; this is an internal contract/UI defect.

## Related Specs

- `.trellis/spec/guides/cross-layer-thinking-guide.md` — map and type the full frontend/API/service/storage round trip.
- `.trellis/spec/guides/code-reuse-thinking-guide.md` — reuse the existing Provider Model create/binding path and existing UI primitives.
- `.trellis/spec/aether-gateway/backend/index.md` — gateway package index; package-specific files are otherwise mostly placeholders for this concern.

## Caveats / Not Found

- No existing test exercises `BatchAssignModelsDialog` save behavior or a mismatched upstream/global name through `assign-global-models`.
- Current batch association assumes one Provider Model per Global Model; supporting multiple base Provider Models for one Global Model would be a separate product/cardinality change. Existing custom aliases already cover additional upstream names without that expansion.
