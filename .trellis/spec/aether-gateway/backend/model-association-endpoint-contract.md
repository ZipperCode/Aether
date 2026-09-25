# Provider Model Association Endpoint Evidence Contract

## 1. Scope / Trigger

本契约约束管理员显式创建或导入 Provider Model 时的 Endpoint 证据。Antigravity 额度刷新只更新额度与发现元数据，不负责写入模型目录。

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
- Antigravity 额度刷新（包含 OAuth 更新后、后台探测和手动刷新）只持久化 Key 额度与上游模型元数据，不创建或改写 GlobalModel、ProviderModel 及 Endpoint 绑定；删除的模型不得在下次刷新时恢复。
- 用户显式导入发现模型时，继续传递完整的 `id`、`api_formats`、`endpoint_ids`；发现列表沿用共享的大小写不敏感内部模型过滤，不从额度响应自动发起目录导入。
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
| 额度刷新发现可路由或内部模型 | 保留上游额度元数据，不写模型目录 |
| 删除全局模型后再次刷新额度 | 不恢复全局模型、提供商模型或 Endpoint 绑定 |
| 用户显式导入模型 | 按原导入流程创建目录记录，保留准确 Endpoint 证据 |

## 5. Good / Base / Bad Cases

- Good：用户确认导入 `gemini-3.7-flash` 后，以发现的 Endpoint IDs 创建 Provider Model；仅刷新额度时不创建。
- Base: a Provider with no published upstream list continues through existing backend inference.
- Bad: `gemini-3.8` must not silently choose `gemini-3.8-flash-high`; the user selects that mapping explicitly.

## 6. Tests Required

- Default path: open the dialog, wait for aggregate discovery, select an exact same-name Global Model, save, and assert the complete `createModel` payload plus absence of batch inference.
- Compatibility path: return no upstream models and assert the original batch assignment call.
- Race path: keep discovery pending and assert save is disabled and guarded.
- Session path: resolve an older request after reopening and assert that only current-session models are rendered.
- Contract path: frontend type-checking must include `endpoint_ids` on the provider-query response type.
- 管理端额度路径：通过真实刷新接口验证额度与发现元数据正常持久化、既有模型及手动绑定不变；删除全局模型后再次刷新仍无目录记录。另保留显式导入成功与失败清理的行为测试。

## 7. Wrong vs Correct

### Wrong

Test only a hidden or optional Key-selection path, then assume the normal “select and save” flow has the same Endpoint evidence.

### Correct

Load aggregate upstream evidence on the default path, guard saving until it settles, and keep manual Key selection only as an explicit refresh or disambiguation tool.
