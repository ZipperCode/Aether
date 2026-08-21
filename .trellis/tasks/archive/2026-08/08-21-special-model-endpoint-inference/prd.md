# 修复特殊模型 Endpoint 推断与公开模型目录

## Goal

修复模型与 Endpoint 精确绑定功能对特殊 API Endpoint 模型的关联回归，并让标准 OpenAI 模型目录正确发布当前 Key 可见的全部已启用 Global Model。

用户应能够把 `gpt-image-2` 这类不会出现在上游模型发现结果中的图像模型关联到 `openai:image` Endpoint；关联完成后，标准 `GET /v1/models` 和 `GET /v1/models/{id}` 也应能看到该模型。

## Background

- Endpoint 推断入口位于 `apps/aether-gateway/src/handlers/admin/request/models.rs:248` 的 `infer_admin_model_endpoint_binding`。
- 当前推断只覆盖显式 `endpoint_ids`、模型发现缓存、`provider_model_mappings.api_formats` 和单 Endpoint 迁移兜底。
- Global Model 关联 Provider 时没有 `provider_model_mappings`；`gpt-image-2` 也可能不出现在模型发现结果中，因此多 Endpoint Provider 返回“无法推断 Endpoint”。
- 普通 `GET /v1/models` 在 `apps/aether-gateway/src/control/route/mod.rs:232` 的 `detect_public_models_auth_signature` 中回落为 `openai:chat`，随后在 `apps/aether-gateway/src/handlers/public/support/models/shared.rs:242` 按模型族过滤 Global Model。
- `gpt-image-2` 仅声明 `image_generation`，不属于 Chat/生成模型族，因此从标准 OpenAI 模型目录中被过滤；当前也没有能把 `/v1/models` 识别为 `openai:image` 的公开请求形态。
- Global Model 的 `supported_capabilities` 与 `config` 已表达模型用途；现有代码已把 `image_generation`/`image` 与 `openai:image` 归为同一模型族。

## Requirements

### Endpoint 自动推断

- 保持显式 Endpoint 绑定、模型发现结果和 `provider_model_mappings` 的现有优先级不变。
- 当上述证据不存在时，使用 Global Model 已声明的 API 格式或能力族匹配当前 Provider 的 Endpoint。
- `image_generation`/图像模型族必须匹配 `openai:image` Endpoint，覆盖 `gpt-image-2` 场景。
- 自动推断必须基于模型元数据，不得硬编码具体模型名。
- 只绑定语义匹配的 Endpoint；没有有效证据时继续返回现有歧义错误。
- 自动绑定继续使用现有来源语义，不新增 API 字段或数据库枚举。

### 标准 OpenAI 模型目录

- 标准 `GET /v1/models` 返回当前 Key 可见的全部已启用 Global Model，不再把普通 OpenAI 模型目录等同于 `openai:chat` 模型族目录。
- 标准 `GET /v1/models/{id}` 对同一可见模型返回详情，避免列表可见但详情为 404。
- API Key 的 `allowed_models` 和 Provider 限制继续生效；Provider 限制按任意 API 模型族中的静态有效关联判断，不因请求被识别为 Chat 而排除图像模型。
- Claude、Gemini 和带 `client_version` 的 Codex 模型目录继续保留现有协议识别、响应结构和模型族过滤语义。
- 模型目录只表达配置发布与 Key 权限；具体 API 是否可调用仍由 Endpoint 绑定、Endpoint/Model 开关和运行时调度校验。

### 一致性与约束

- Endpoint 推断与公开模型目录必须复用同一份模型能力/API 格式族语义，避免两套常量表漂移。
- 所有新增或调整的代码注释必须完整、规范，并解释模型目录与调用能力校验的边界。
- 不进行 Git Commit，不运行大模块编译，不新增单元测试。

## Acceptance Criteria

- [x] 多 Endpoint Provider 同时包含普通生成 Endpoint 和 `openai:image` Endpoint 时，声明图像生成能力的 Global Model 可以成功关联并只自动绑定图像 Endpoint。
- [x] Endpoint 推断不依赖模型是否出现在上游模型发现缓存中，且不包含 `gpt-image-2` 模型名特判。
- [x] 普通多 Endpoint 模型在没有显式绑定、发现结果或模型元数据证据时，仍返回“无法推断 Endpoint”。
- [x] 显式绑定、发现结果和 Provider Model 映射的现有优先级与行为保持不变。
- [x] 标准 OpenAI `GET /v1/models` 能返回当前 Key 可见且已启用的 `gpt-image-2`。
- [x] 标准 OpenAI `GET /v1/models/gpt-image-2` 能返回与列表一致的模型详情。
- [x] `allowed_models` 与 Provider 限制仍约束标准 OpenAI 模型目录，Claude/Gemini/Codex 模型目录行为不变。
- [x] 不新增前端字段、公开后端字段、数据库迁移或具体模型名特判。
- [x] 修改通过目标文件 Rust 格式检查、`git diff --check` 和局部静态审查；不运行大模块编译。

## Out of Scope

- 在关联弹窗中新增 Endpoint 手工选择 UI。
- 修改模型发现协议或要求图像 Endpoint 实现 `/models`。
- 修改 Endpoint/Model 开关、运行时候选隔离或实际请求调度规则。
- 为历史错误绑定执行批量数据库迁移。
- 改变 Claude、Gemini、Codex 专用模型目录的响应契约。
