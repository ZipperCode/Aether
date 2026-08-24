# Technical Design

## Data Flow

`RoutingProfiles.vue` 已持有全局模型列表，通过当前模型名称解析 `GlobalModelResponse.id`，并将该 ID 作为内部组件属性传给 `RoutingPriorityPolicyEditor.vue`。

排序编辑器继续一次性读取现有 Provider 摘要；按模型模式只在 `providerRows` 投影处根据 `global_model_ids`、`is_active`、`active_keys` 过滤。内部保留全量 Provider 数据，统一排序和 Key/Pool 元数据解析不受影响。

## Configuration Semantics

可见 Provider 重排时，把新的可见顺序合并进当前模型已有的 `provider_priority_overrides`，而不是替换整个对象。这样停用、失去模型关联或暂时没有启用 Key 的 Provider 重新满足条件后仍恢复原有覆盖值。

## Interfaces and Compatibility

- 新增内部 Vue 属性 `globalModelId?: string`，只用于按模型 Provider 投影。
- 缺失 ID 在按模型模式代表模型不在当前有效目录，结果为空。
- 不改变路由配置 JSON、后端响应、数据库或公开 API。

## Demo Contract Parity

Demo 的 Provider 摘要响应从既有 `generateMockModelsForProvider` 结果投影活跃 `global_model_id`，避免维护第二份模型关联表，并保持与生产摘要字段一致。
