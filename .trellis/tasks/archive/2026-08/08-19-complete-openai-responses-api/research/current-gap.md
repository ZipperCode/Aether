# Codex HTTP Responses 契约与 Aether 现状

调查日期：2026-08-19。官方产品事实来自 `developers.openai.com`；Codex 客户端事实来自 OpenAI 官方仓库；Aether 事实来自当前工作区。字段 inventory 只用于审计，不是运行时 allowlist。

## 官方资料

- Responses 总览：<https://developers.openai.com/api/reference/responses/overview>
- Conversation state：<https://developers.openai.com/api/docs/guides/conversation-state>
- HTTP streaming：<https://platform.openai.com/docs/api-reference/streaming>
- Compact：<https://developers.openai.com/api/reference/python/resources/responses/methods/compact>

官方通用 Responses 资源面比 Codex HTTP 消费面更大；本任务以实际下游消费端为产品范围，以官方 schema 为协议字段/事件证据。

## Codex HTTP 实证

核对 OpenAI Codex commit `3929c99a97d1aa0fb8000903a4b57b24fbabe742`：

- `codex-rs/codex-api/src/common.rs:252-276`：HTTP `ResponsesApiRequest` 有 model、instructions、input、tools、tool choice、reasoning、store、stream、include、service tier、prompt cache、text、client metadata；没有 `previous_response_id`。
- `codex-rs/core/src/client.rs:920-934`：HTTP 请求固定 `store=false`、`stream=true`。
- `codex-rs/codex-api/src/endpoint/responses.rs:70-145`：HTTP inference 只调用 `POST responses` 并接受 SSE。
- `codex-rs/codex-api/src/common.rs:28-43` 与 `endpoint/compact.rs:35-55`：remote compact 使用 model + 完整 input 调用 `POST responses/compact`，返回 compacted output。
- `previous_response_id` 只出现在 Responses WebSocket request；当前源码未发现 Codex HTTP 调用 retrieve/delete/cancel/input_items/input_tokens。

## Aether 当前入口

- `apps/aether-gateway/src/api/ai/registry.rs:15-20` 显式登记 `POST /v1/responses` 与 `POST /v1/responses/compact`。
- `apps/aether-gateway/src/api/ai/openai.rs:3-22` 为两条路径提供独立格式 identity/signature。
- `crates/aether-provider/transport/src/same_format_provider/mod.rs:330-346` 在 client/provider 格式一致时复制请求 JSON 对象；现有 opaque request field 回归位于同文件相关 Responses 测试。
- 同格式 SSE 使用 direct passthrough；跨格式 Responses 通过 canonical rewriter/emitter。
- 当前 docs inventory 只列 create/compact，端点集合与 Codex HTTP 目标一致；需核对字段和事件版本，而不是增加 5 个未消费资源端点。

## 已排除的错误方向

- 不实现 7 个通用资源端点；它们不是当前 Codex HTTP 依赖。
- 不保存 Response 正文，不建 `response_id -> provider/key` affinity；HTTP 请求自带 model 和完整 input。
- 不用 scheduler affinity 或 continuation history 冒充 OpenAI 30 天 Application State。
- 不实现 WebSocket；若未来需要，应单独规划连接内 previous-response 语义。

## 实施调查重点

1. 当前 Codex create 字段在同格式、跨格式和测试中的真实覆盖。
2. protocol-related header 与 credential/hop-by-hop/internal header 的现有边界。
3. 同格式 unknown response field、opaque SSE event、terminal error 是否有端到端锁定。
4. 跨格式 Codex 所需 reasoning/tool/text/lifecycle 事件是否存在静默丢失。
5. compact 当前 payload/response 与错误保真是否覆盖最新 Codex struct。

## 用户决策

- 2026-08-19：仅做 Codex HTTP 中转兼容；范围为 create、compact、HTTP SSE、错误与第三方格式转换；排除持久化、资源管理端点和 WebSocket。
