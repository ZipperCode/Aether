# 技术设计

## 变更边界

当前存在两个由同一语义缺口引起的行为问题：管理端 Endpoint 推断没有读取 Global Model 的能力族，公开 OpenAI 模型目录又把通用目录错误收窄为 Chat 模型族。修复应统一模型元数据解释，但不改变模型发现、权限契约、Endpoint 开关或运行时调度。

预计修改以下文件：

- `apps/aether-gateway/src/lib.rs`：注册共享模型元数据模块。
- `apps/aether-gateway/src/model_metadata.rs`：集中 API 格式族、能力族和模型声明解析语义。
- `apps/aether-gateway/src/handlers/admin/request/models.rs`：在现有推断优先级之后、单 Endpoint 兜底之前使用 Global Model 元数据匹配 Endpoint。
- `apps/aether-gateway/src/handlers/public/support/models/shared.rs`：改为复用共享模型元数据语义，并提供保留权限/状态过滤但可选模型族过滤的目录过滤入口。
- `apps/aether-gateway/src/handlers/public/support/models/route.rs`：标准 OpenAI 列表和详情使用跨模型族目录；Claude、Gemini、Codex 继续走现有格式过滤路径。

不修改数据库、前端、模型发现缓存、Endpoint/Model 开关和执行候选选择。

## 共享模型元数据语义

共享模块负责：

- 规范化已知 API 格式到 Generation、Image、Embedding、Rerank 模型族。
- 规范化 `supported_capabilities` 中的同义能力名称到相同模型族。
- 从 Global/Provider Model `config` 的 `api_format`、`client_api_format`、`provider_api_format`、`api_formats`、`capabilities` 和 `supported_capabilities` 字段读取声明。
- 区分缺少声明、有效声明和无效声明，保留当前 fail-closed 行为。

公开模型过滤与管理端 Endpoint 推断只能调用这份共享语义，不再分别维护格式/能力映射。

## Endpoint 推断顺序

保持以下优先级：

1. Provider Model 显式 `endpoint_ids`。
2. 上游模型发现缓存返回的 `endpoint_ids`。
3. Provider Model 映射中的显式 `api_formats`。
4. Global Model `config`/`supported_capabilities` 声明的模型族与 Provider Endpoint 格式族匹配。
5. Provider 只有一个 Endpoint 时使用现有迁移兜底。
6. 仍无结果时返回现有歧义错误。

第四步允许绑定同一已声明模型族中的全部匹配 Endpoint；不会跨模型族绑定，也不会覆盖前三类更精确证据。绑定来源继续使用现有自动映射来源。

## 标准 OpenAI 模型目录

标准 OpenAI `GET /v1/models` 的响应格式仍为 OpenAI list，但可见性过滤分成两层：

- 始终保留 Global Model 启用状态、API Key `allowed_models`、Provider 限制和静态关联状态校验。
- 对标准 OpenAI 目录跳过模型族过滤，使图像、Embedding、Rerank 等 Global Model 可见。

Provider 限制必须从所有模型族的有效静态关联中计算允许的 Global Model ID，不能先按 `openai:chat` 收窄。

标准 OpenAI `GET /v1/models/{id}` 使用 Global Model 精确名称读取并应用同一可见性规则；响应构建只依赖模型名，不要求为 OpenAI Chat 找到可路由候选。实际调用能力仍由后续路由与 Endpoint 绑定决定。

Claude、Gemini、Codex 请求继续按各自 API 格式过滤并保留现有响应契约。

## 兼容性与风险

- 标准 OpenAI 模型列表将新增此前被隐藏的非 Chat 模型，这是预期行为变化。
- 调用方可能把列表内模型误用于 Chat API；运行时会按现有协议和 Endpoint 能力拒绝或路由，不在目录层伪造可调用性。
- 共享语义抽取必须保持缺少声明时默认 Generation、无效声明时 fail-closed 的现有行为，避免 Claude/Gemini/Codex 列表回归。
- 回滚只需恢复目录的格式过滤与管理端新增推断分支，不涉及数据迁移。
