# 修复关联模型 Endpoint 推断复发

## Goal

修复 Provider 详情页“关联模型”仍可能把 `gemini-3.7-flash` 送入旧 Endpoint 自动推断并失败的问题。用户按原有“勾选模型 → 保存”流程操作时，同名真实上游模型应自动携带其 `endpoint_ids`，不再要求先点击“按密钥匹配”。

## Background

- 旧修复 `ce4a40057` 仅在用户先选择 Key 并加载上游模型后，才保存真实 `provider_model_name` 与 `endpoint_ids`。
- `BatchAssignModelsDialog.vue:612-626` 会把没有上游选择的新增项继续提交给 `assign-global-models`；用户看到的原始错误文案正来自该兜底路径。
- 现有 `useUpstreamModelsCache.fetchModels(providerId)` 已支持聚合 Provider 全部 Key 的上游模型，后端保证按模型 ID 合并 `api_formats` 和 `endpoint_ids`，无需新接口或模型名猜测。

## Requirements

1. 关联弹窗打开并完成基础数据加载后，自动通过现有无 Key 查询加载 Provider 的聚合上游模型。
2. 同名 Global Model 与上游模型继续只做忽略大小写的精确匹配；保存时使用真实上游模型 ID 和其 `endpoint_ids`。
3. 不同名模型继续使用现有下拉框自由选择，不增加前缀、模糊或模型名特判。
4. 手动选择某个 Key 刷新上游模型的入口保持可用。
5. 初始上游查询尚未完成时不得提前保存；关闭或切换 Provider 后的旧响应不得覆盖新会话。
6. Provider 无 Key、上游不发布模型或查询失败时，保留现有批量自动推断兜底。
7. 前端响应类型必须声明后端实际返回的 `endpoint_ids`，避免跨层契约继续遗漏该字段。
8. 提交前必须用当前工作区源码构建并启动隔离 Docker 实例，真实验证多 Endpoint 下的旧失败基线、聚合发现结果和精确绑定结果。
9. 用户验收环境必须由标准 Compose 文件直接构建并保持运行，不使用预览容器或模拟数据。

## Acceptance Criteria

- [x] Global Model 与上游模型均为 `gemini-3.7-flash` 时，用户无需点击 Key，勾选后保存即调用 `createModel`，请求包含真实模型名和去重后的 Endpoint ID。
- [x] 上述同名场景不调用 `batchAssignModelsToProvider`，因此不再产生“无法推断 Endpoint”错误。
- [x] `gemini-3.8 → gemini-3.8-flash-high` 的显式选择行为保持通过。
- [x] 没有可用上游模型时仍调用原批量推断接口。
- [x] 初始聚合查询的跨会话陈旧响应会被丢弃。
- [x] 目标 Vitest、前端类型检查和 `git diff --check` 通过。
- [x] 当前源码 Docker 镜像构建成功；隔离实例的健康、登录、首页、生产 JS、旧失败基线、聚合查询和 SQLite 绑定断言全部通过。
- [x] 标准 Compose 已从当前源码构建并启动真实 PostgreSQL、Redis 和 Aether；三项服务健康，真实登录及前端资源访问通过。

## Out of Scope

- 按模型名前缀猜测 Endpoint 或自动选择不同名上游模型。
- 删除模型与 Endpoint 精确绑定。
- 修改数据库、Rust 后端接口或 Global Model → Provider 的另一条关联入口。
