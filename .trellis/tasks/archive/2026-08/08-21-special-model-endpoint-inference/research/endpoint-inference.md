# Endpoint 推断根因与实现边界

## 当前调用链

- Global Model 关联 Provider 时，`build_admin_assign_global_model_to_providers_payload` 创建没有 `provider_model_mappings` 的 Provider Model，并调用 `build_admin_provider_model_create_mutation`。
- `build_admin_provider_model_create_mutation` 在 Provider 已配置 Endpoint 时调用 `infer_unambiguous_admin_model_endpoint_ids`。
- `infer_admin_model_endpoint_binding` 当前按以下顺序推断：
  1. `provider_model_mappings.endpoint_ids`
  2. 上游模型发现缓存中的 `endpoint_ids`
  3. `provider_model_mappings.api_formats`
  4. Provider 仅有一个 Endpoint 时的迁移兜底
- `gpt-image-2` 不出现在发现缓存且关联请求没有 Provider Model 映射；多 Endpoint Provider 因而得到空结果并返回现有歧义错误。

## `/v1/models` 不返回图像模型的原因

- `detect_public_models_auth_signature` 把没有 Claude、Gemini 或 Codex 特征的 `GET /v1/models` 识别为 `openai:chat`。
- `list_models_for_client_format` 读取已启用 Global Model 后调用 `filter_global_models_for_models`。
- `global_model_supports_format` 把 `image_generation` 归为图像族，把 `openai:chat` 归为生成族，因此只声明图像能力的 `gpt-image-2` 被过滤。
- 当前不存在能让 `/v1/models` 使用 `openai:image` 签名的请求形态；Embedding 和 Rerank 等非 Chat 模型同样没有独立公开列表入口。
- 这不是 Endpoint 绑定失败直接造成的，但两者共享同一份模型能力/API 格式族语义，适合在同一任务中统一边界。

## 已确认的产品决策

- 标准 OpenAI `GET /v1/models` 应返回当前 Key 可见的全部已启用 Global Model，不再只发布 Chat/生成模型族。
- 标准 OpenAI `GET /v1/models/{id}` 使用相同可见性边界，避免列表与详情不一致。
- Claude、Gemini、Codex 专用模型目录继续按协议和模型族过滤。
- 模型目录表达配置发布与权限，不替代实际请求的 Endpoint、协议和运行时能力校验。

## 已有语义

- Global Model 的 `supported_capabilities` 与 `config` 已表达模型用途。
- `apps/aether-gateway/src/handlers/public/support/models/shared.rs` 已把 `image_generation`/`image` 归为图像模型族，并把 `openai:image` 归为图像 API 格式族。
- 修复应复用或集中这层语义，避免在管理端推断中维护一份会漂移的独立映射。

## 最小边界

- 保留显式绑定、发现缓存和 Provider Model 映射的现有优先级。
- 在单 Endpoint 迁移兜底之前，使用 Global Model 声明的 API 格式/能力族匹配 Provider Endpoint。
- 仅绑定匹配的 Endpoint；无有效模型元数据时继续报错。
- 不硬编码模型名，不改变公开 API、数据库结构、Endpoint 开关或运行时候选隔离。

## 验证重点

- 图像能力模型在普通生成 Endpoint 与 `openai:image` Endpoint 并存时只匹配图像 Endpoint。
- 没有元数据证据的普通多 Endpoint 模型仍失败。
- 现有显式绑定、发现结果和映射优先级不变。
