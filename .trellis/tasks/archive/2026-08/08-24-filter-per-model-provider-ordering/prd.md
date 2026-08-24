# 按模型筛选可排序 Provider

## Goal

让管理员在“区分模型”调度中只排序与当前模型有关、已启用且至少有一个启用 Key 的 Provider，避免大量无关条目干扰配置。

## Background

- 当前 `RoutingPriorityPolicyEditor` 为每个模型展示全量 Provider。
- Provider 摘要已经提供 `global_model_ids`、`is_active` 和 `active_keys`，无需新增后端接口。
- 现有统一排序和 Key 排序有独立用途，不属于本次筛选范围。

## Requirements

- 按模型的 Provider 排序仅展示同时满足以下条件的条目：当前全局模型 ID 在 `global_model_ids` 中、Provider 已启用、`active_keys > 0`。
- 当前模型无法解析到有效全局模型 ID 时展示空列表，不得回退到全量 Provider。
- 统一 Provider 排序和 Key 排序保持现有行为。
- 隐藏条目的既有 `provider_priority_overrides` 必须保留；编辑可见条目不得隐式删除其配置。
- “可用 Key”仅指 Key 已启用；余额、熔断、健康状态、Key 模型白名单和 Endpoint 状态不参与本次隐藏判断。
- 不新增数据库、HTTP API、后端逻辑或依赖。
- Demo Provider 摘要须镜像生产契约的 `global_model_ids`，以便用模拟数据安全预览筛选效果。

## Acceptance Criteria

- [x] 选择一个模型后，仅显示具备该模型活跃关联、已启用且至少一个 Key 启用的 Provider。
- [x] 错误模型、停用 Provider、全部 Key 停用的 Provider 均不显示。
- [x] 无法解析模型 ID 时出现明确空状态，而不是全量 Provider。
- [x] 切换模型后列表随传入模型 ID 立即重新计算。
- [x] 拖拽、上下移动或手动改优先级不会删除隐藏 Provider 的既有覆盖值。
- [x] 统一排序和 Key 排序行为未改变。
- [x] 聚焦组件测试与前端类型检查通过。
- [x] Demo 模式可从现有模拟 Provider 模型关联生成 `global_model_ids`。
