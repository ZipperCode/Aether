# Endpoint 推断复发证据

- 原始错误只在 `apps/aether-gateway/src/handlers/admin/request/models.rs:228-246` 的无证据推断分支产生。
- Provider 详情关联弹窗在 `frontend/src/features/providers/components/BatchAssignModelsDialog.vue:612-626` 将没有 `selectedUpstreamModelIds` 的新增项送入旧批量接口。
- 上次修复 `ce4a40057` 的测试只覆盖“点击 Key 后手动选择不同名上游模型”，没有覆盖用户原有的直接勾选保存流程。
- `useUpstreamModelsCache.fetchModels(providerId)` 已调用聚合查询；`aggregate_models_for_cache` 按模型 ID 合并重复记录及其 `endpoint_ids`。
- 因此复发属于入口覆盖不完整和测试缺口；最小根因修复是在关联弹窗自动加载聚合上游证据，而不是新增模型名特判。
