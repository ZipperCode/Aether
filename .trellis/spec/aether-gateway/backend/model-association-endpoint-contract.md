# Provider Model Association Endpoint Evidence Contract

## 1. Scope / Trigger

本契约约束管理员显式创建或导入 Provider Model 时的 Endpoint 证据。Antigravity 额度刷新只更新额度与发现元数据，不负责写入模型目录。

## 2. Signatures

- Aggregate discovery: `fetchModels(providerId, undefined, false) -> { models: UpstreamModel[] }`
- Exact creation: `createModel(providerId, { global_model_id, provider_model_name, endpoint_ids? })`
- Compatibility fallback: `batchAssignModelsToProvider(providerId, globalModelIds)`
- Each discovered model is shaped as `UpstreamModel { id, api_formats, endpoint_ids? }`.
- Manual Endpoint choices: `getProviderEndpoints(providerId) -> ProviderEndpoint[]`.

## 3. Contracts

- The provider-query backend aggregates duplicate upstream records by exact model ID and unions their `api_formats` and `endpoint_ids`.
- The Provider detail association dialog loads aggregate upstream models without requiring an extra Key-selection action.
- Exact, case-insensitive Global Model name matches may be selected automatically. Different names require an explicit user selection; prefixes and fuzzy guesses are forbidden.
- Exact creation uses the upstream `id` as `provider_model_name` and forwards its de-duplicated `endpoint_ids`.
- Antigravity 额度刷新（包含 OAuth 更新后、后台探测和手动刷新）只持久化 Key 额度与上游模型元数据，不创建或改写 GlobalModel、ProviderModel 及 Endpoint 绑定；删除的模型不得在下次刷新时恢复。
- 用户显式导入发现模型时，继续传递完整的 `id`、`api_formats`、`endpoint_ids`；发现列表沿用共享的大小写不敏感内部模型过滤，不从额度响应自动发起目录导入。
- Saving is unavailable while the initial aggregate query is pending. Async results may update state only when Provider ID, open state, and dialog session still match.
- If discovery returns no usable model, the existing batch inference path remains available for single-Endpoint Providers, explicit metadata, and Providers that do not publish a model list.
- Every newly selected model exposes manual association, even when discovery is empty or failed. In **手动指定模型与 Endpoint**, prefill the Global Model name, allow editing the real upstream name, and require explicit selection of at least one Provider Endpoint. Never infer support from a model-name prefix or select all Endpoints automatically.
- Manual entries call `createModel` with the trimmed name and selected `endpoint_ids`, never batch inference. Discovery refresh cannot overwrite manual names or bindings. Deselecting a row, closing/reopening, or changing Providers discards its manual state.
- Endpoint loading is session-fenced separately from manual input resets; synchronizing existing associations must not erase freshly loaded Endpoint choices. Manual-mode select values must not collide with real upstream model IDs.
- `models` enforces `UNIQUE(provider_id, provider_model_name)` and one `global_model_id` per row. To preserve two explicitly requested Global Model names without changing cardinality, use distinct configured rows with an unconditional `provider_model_mappings` target equal to the real discovered upstream ID; bind the exact Endpoint explicitly. Never send the configured alias as an invented upstream ID or add it to account model lists. Preserve exact-global precedence; regex mappings do not merge globals or their Endpoint lists.
- Logical pool-group enumeration must not reject a group because its representative's `allowed_models` is empty or incompatible. If normal representative resolution fails, resolve only the scoped configured/provider-mapped target for the group. Actual member hydration still applies its own model list; keep this fallback out of generic resolvers because expanded member rows also carry `provider_pool_enabled=true`.
- Runtime pool expansion must resolve `selected_provider_model_name` and `mapping_matched_model` for each account row through `resolve_provider_model_name_with_model_directives_and_request_operation`, before transport permission checks and payload construction. A representative account's selected target is not a pool-wide fact.
- Reuse the request's existing per-format directive routing model and request operation. Preserve provider mapping priority and Endpoint/operation scope; do not reload directive policy on each pool page or enable directives unconditionally.
- `provider_api_keys.allowed_models=NULL` preserves the row's configured default/provider-mapped target without account restrictions; an explicit empty list denies the account. Account-listed variants must match the configured default or a Provider mapping valid for the current API format, Endpoint and operation. A global regex routes inbound names to a Global Model; it cannot authorize an unconfigured outbound variant. Disabling automatic model fetching on one account must not disable mapped alternatives on other accounts.
- Score, ordinary-page, routing-allowlist and sticky-account hydration share account-local resolution. Model quota windows and reserve thresholds use the resolved account target. Memory candidate listing mirrors PostgreSQL's one-representative-per-pool projection; pool member reads remain expanded.

## 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| Exact upstream name with Endpoint IDs | Use exact creation; do not invoke batch inference |
| Different upstream and Global Model names | Require explicit upstream selection |
| Aggregate query pending | Disable and guard save |
| Aggregate query empty or failed | Keep the compatibility fallback |
| Response belongs to an old dialog session | Discard it without changing current state |
| Endpoint ID is empty, duplicated, or foreign | Normalize duplicates in the UI; backend validation rejects empty or foreign IDs |
| Manual name blank/whitespace or no Endpoint selected | Show inline guidance; disable Save and guard all writes, including pending deletions |
| Endpoint list failed or empty | Show the loading failure or missing-Endpoint guidance; manual association remains unavailable |
| Manual row followed by discovery refresh | Preserve manual name and chosen Endpoints |
| 额度刷新发现可路由或内部模型 | 保留上游额度元数据，不写模型目录 |
| 删除全局模型后再次刷新额度 | 不恢复全局模型、提供商模型或 Endpoint 绑定 |
| 用户显式导入模型 | 按原导入流程创建目录记录，保留准确 Endpoint 证据 |
| First account has no model list; later account lists an explicitly scoped Provider-mapped variant | Later account resolves its own actual variant and remains eligible after first-account failure |
| Account variant matches only a global regex, not a scoped Provider model | Reject the account; do not synthesize an upstream target from its model list |
| Pool member has an explicit empty model list | No upstream send for that member; preserve model-disabled semantics |
| Pool account uses an operation-scoped provider mapping | Apply the original request operation and directive policy snapshot before permission/quota checks |
| Pool representative list excludes the requested model or is empty | Keep the logical group; evaluate actual member lists before any upstream send |
| All pool members exclude the requested model | Exhaust the pool with zero upstream sends; never weaken member permissions |
| Two Global Models need the same real upstream target | Preserve distinct configured identities and explicit real-target mappings; do not reassign the only route away from the existing global |

## 5. Good / Base / Bad Cases

- Good：用户确认导入 `gemini-3.7-flash` 后，以发现的 Endpoint IDs 创建 Provider Model；仅刷新额度时不创建。
- Base: a Provider with no published upstream list continues through existing backend inference.
- Bad: `gemini-3.8` must not silently choose `gemini-3.8-flash-high`; the user selects that mapping explicitly.
- Good: `gpt-6.1-sol` absent from discovery is explicitly associated with the known-working Responses Endpoint; its absence is not treated as proof that it cannot be called.

## 6. Tests Required

- Default path: open the dialog, wait for aggregate discovery, select an exact same-name Global Model, save, and assert the complete `createModel` payload plus absence of batch inference.
- Compatibility path: return no upstream models and assert the original batch assignment call.
- Race path: keep discovery pending and assert save is disabled and guarded.
- Session path: resolve an older request after reopening and assert that only current-session models are rendered.
- Contract path: frontend type-checking must include `endpoint_ids` on the provider-query response type.
- Manual boundaries: empty/whitespace names and missing Endpoints block every write; completing the row saves only the selected binding.
- Manual precedence/session isolation: Key refresh cannot replace manual edits; deselection, reopen, Provider switches, and stale Endpoint responses cannot leak bindings.
- Select collision: a discovered model literally named `__manual__` remains a discovered model, not the manual-mode control.
- 管理端额度路径：通过真实刷新接口验证额度与发现元数据正常持久化、既有模型及手动绑定不变；删除全局模型后再次刷新仍无目录记录。另保留显式导入成功与失败清理的行为测试。
- `responses_pool_mixed_account_model_failover_sends_account_mapped_suffix`: real loopback HTTP, first account receives its bare configured target twice and returns404; second receives exactly its account-listed mapped target and returns200. Assert concrete candidate identities/statuses and final content.
- `responses_pool_empty_account_model_list_stays_denied`: explicit empty account list prevents any upstream send despite a preceding unrestricted member.
- `pool_member_model_resolution_tracks_account_scope_and_denials` and directive-policy coverage preserve mapping/operation scope, no-intersection denials and policy-disabled behavior. Memory projection parity must preserve all group-member rows while listing one logical pool representative.
- `pool_cursor_raw_routing_model_keeps_restricted_custom_suffix_member_schedulable`: exercise `next_key` with a restricted raw custom-suffix model; callers that disable directive routing retain the eligible account, while an explicitly supplied base-routing context observes its own restrictions.
- `pool_representative_model_list_denial_does_not_reject_logical_group`, `non_pool_account_model_list_denial_stays_strict`, `pool_representative_mapping_scope_mismatch_stays_denied`, and `generic_model_resolution_stays_strict_for_pool_member_rows` lock the group/member boundary, explicit-empty semantics and Endpoint/operation scope.
- `responses_pool_incompatible_representative_never_sends_and_sibling_serves_actual_model` and `responses_pool_all_incompatible_account_models_send_nothing` exercise real loopback HTTP: only the supporting sibling may receive the actual mapped model, and all-incompatible pools send nothing.
- Model-resolution fixtures must configure scoped Provider mappings for positive account-variant cases and retain a regex-only rejection case. Keep core `global_regex_does_not_replace_unmatched_requested_model` intact when migrating caller tests.

## 7. Wrong vs Correct

### Wrong

Test only a hidden or optional Key-selection path, then assume the normal “select and save” flow has the same Endpoint evidence.

### Correct

Load aggregate upstream evidence on the default path, guard saving until it settles, and keep manual Key selection only as an explicit refresh or disambiguation tool.

Do not ask users to select an Endpoint without exposing that action in the normal association dialog. Keep both discovered evidence and explicit manual association reachable.

Wrong: clone a pool representative, replace only its Key fields, then reject other accounts against the representative's upstream model name.

Correct: re-resolve the account row with the request's mapping context, then check permissions, quotas and construct that account's request.

Wrong: apply a representative account's model list to the whole logical pool, or bypass generic account checks whenever a pool flag is present.

Correct: defer only representative-list denial during group enumeration, then strictly resolve each actual account before sending.
