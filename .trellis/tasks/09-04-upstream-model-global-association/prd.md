# 修复上游模型自由关联 Global Model

## Goal

修复 Provider 详情页“关联模型”只能依赖同名匹配的问题。管理员应能把任意真实上游模型关联到所选 Global Model，并由上游发现结果自动带入精确 Endpoint 链路，使 `gemini-3.8-flash-high` 可以关联到 `gemini-3.8`，同时继续支持后续自定义模型映射。

## Background

- 当前弹窗只保存 Global Model ID；后端据此把 Global Model 名称同时当作 `provider_model_name`。
- Endpoint 自动推断仅能从精确模型名、显式映射、模型元数据或单 Endpoint 兜底中取得证据。
- 当 Global Model 为 `gemini-3.8`、上游真实模型为 `gemini-3.8-flash-high` 时，精确名称匹配失败，缓存中的 `endpoint_ids` 无法被采用，最终返回“无法推断 Endpoint”。
- `UpstreamModel` 已包含真实 `id` 与 `endpoint_ids`，现有 Provider Model 创建接口也已接受 `global_model_id`、`provider_model_name` 和 `endpoint_ids`，无需新增后端契约。

## Requirements

1. Provider 详情页“关联模型”继续展示全部可选 Global Model，用户可以自由选择目标 Global Model，不要求它与上游模型同名。
2. 选择某个 Key 获取上游模型后，新关联的 Global Model 可以选择一个真实上游模型。
3. 显式选择上游模型时，创建 Provider Model 必须使用：
   - 用户选择的 `global_model_id`；
   - 上游模型真实 `id` 作为 `provider_model_name`；
   - 上游模型携带的 `endpoint_ids` 作为精确 Endpoint 绑定。
4. 同名模型仍自动勾选并自动选择对应上游模型；不得新增前缀或模糊匹配。
5. 未选择上游模型时保留现有批量关联及后端自动推断行为，支持上游不发布模型列表等历史场景。
6. 已有关联的展示、取消选择和删除行为保持不变。
7. 自定义模型映射仍作用于已创建的 Provider Model，本次不得改变映射方向、作用域或运行时调度语义。
8. 单个关联失败不得阻止其他关联或删除操作完成，继续汇总部分失败信息。

## Acceptance Criteria

- [x] 上游列表包含 `gemini-3.8-flash-high` 时，用户可将其关联到任意所选 Global Model `gemini-3.8`。
- [x] 保存时创建请求包含 `global_model_id=gemini-3.8` 对应 ID、`provider_model_name=gemini-3.8-flash-high` 及该上游记录的 `endpoint_ids`。
- [x] 同名 Global/上游模型继续自动匹配，不需要额外选择。
- [x] 未显式选择上游模型的新增项继续走现有 `assign-global-models` 自动推断接口。
- [x] 已有关联的移除与部分成功提示保持可用。
- [x] 不修改数据库、后端公开请求结构或 Endpoint 精确绑定规则。
- [x] 最小前端逻辑测试覆盖“不同名上游模型关联任意 Global Model”的创建请求。

## Out of Scope

- 模糊、前缀或基于模型名称猜测映射。
- 修改 Global Model → 多 Provider 的“关联提供商”入口。
- 修改现有 Provider Model 的主上游模型名。
- 删除 Endpoint 绑定表、调度约束或手动 Endpoint 兜底。
- 重做模型映射页面或导入模型流程。
