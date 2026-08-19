# 完善 Codex HTTP Responses 中转兼容

## Goal

以 OpenAI Codex 当前 HTTP 客户端和官方 Responses 协议为基线，完善 Aether 对 `POST /v1/responses`、`POST /v1/responses/compact` 及 HTTP SSE 的无状态中转能力，使下游 Codex 能稳定使用 OpenAI 官方或兼容第三方 provider，同时不引入 Aether 自有的 Response 内容存储。

## Background

- 产品定位：Aether 是下游 Codex 与上游 OpenAI 官方/第三方 provider 之间的多租户中转网关，不是 OpenAI Application State 的副本。
- OpenAI Codex 官方仓库 commit `3929c99a97d1aa0fb8000903a4b57b24fbabe742` 中，HTTP 请求固定 `store=false`、`stream=true`，HTTP request 类型不含 `previous_response_id`；远程压缩只调用 `POST /responses/compact` 并携带 model 与完整 input。
- Codex HTTP 当前不调用 retrieve、delete、cancel、input_items、input_tokens；`previous_response_id` 增量链路属于已排除的 WebSocket 模式。
- Aether 已显式登记 `POST /v1/responses` 与 `POST /v1/responses/compact`，本任务聚焦字段、header、SSE、错误、unknown-field 保真和跨格式转换缺口。
- 2026-08-19 用户确认按 Codex HTTP 中转方案继续：不保存响应正文，不建 response affinity 数据库，不实现通用资源管理端点。

## Requirements

- R1：保持并完善 `POST /v1/responses` 与 `POST /v1/responses/compact`；两条路径都必须经过既有认证、准入、provider 选择、transport、usage/audit 与安全日志边界。
- R2：同格式 `openai:responses` 请求按 JSON 值保留 Codex 使用的已知字段及未知扩展字段；不得用 coverage matrix 作为运行时 allowlist，也不得静默改写 `store=false`。
- R3：保留协议相关 header 与请求语义，同时继续隔离下游凭证、hop-by-hop header 和不应泄露给第三方的内部 header。
- R4：同格式 HTTP SSE 直接保留事件类型、顺序、未知字段和不透明事件；usage/error observer 只能旁路观察，不得重写客户端事件语义。
- R5：跨格式 provider 继续使用现有 canonical 边界，覆盖 Codex 所需的 input items、tools/tool results、reasoning、text controls、service tier、prompt cache 与终态；不可忠实表达的关键语义必须 fail closed，禁止伪造完整支持。
- R6：`/responses/compact` 保留 model、input、instructions、tools、parallel_tool_calls、reasoning、service_tier、prompt_cache_key、text 等当前 Codex payload，并返回 provider 的 compacted `output`。
- R7：上游成功与错误保留 HTTP status、必要响应头、request ID、错误 body 和 SSE terminal error；usage/billing 不重复记录或凭空生成 token。
- R8：以当前 Codex HTTP fixture 和官方文档更新 schema inventory/coverage 审计；审计文档不参与运行时过滤。
- R9：不新增依赖、不改 UI、不做无关重构；新增或修改的手写函数、方法、具名回调、类型及业务字段提供实质性中文说明。

## Acceptance Criteria

- [x] AC1：真实 Codex HTTP create payload 可通过 Aether 到达原生 Responses provider，method/path/body/header/SSE 语义保持一致。
- [x] AC2：同格式请求未知字段、同步响应未知字段、SSE 未知字段及不透明事件有端到端回归，均不被 inventory/canonical 过滤。
- [x] AC3：Codex 使用的 input/tool/reasoning/text/stream/include/cache/service-tier 字段在同格式路径保真；跨格式路径能表达的字段正确映射，不能表达的关键字段明确失败。
- [x] AC4：`POST /v1/responses/compact` 接受当前 Codex compaction payload 并返回 `output`，OpenAI 官方和兼容第三方原生 Responses provider 均可透传。
- [x] AC5：代表性 400/401/404/429/5xx、request ID 与 SSE terminal error 保真，认证与准入失败不会调用上游。
- [x] AC6：HTTP 路径不依赖 `previous_response_id`、Response 内容存储或 affinity 数据库；不同请求仍按请求中的 model 正常路由。
- [x] AC7：相关最小 Rust 测试、格式检查和 API coverage 检查通过；只有发现共享契约风险才扩大验证。
- [x] AC8：无 WebSocket、资源管理端点、数据库 schema、前端或新依赖改动，任务自有临时产物已清理。

## Out of Scope

- Responses WebSocket、HTTP Upgrade、连接内 `previous_response_id` 增量链路。
- `GET/DELETE /responses/{id}`、cancel、input_items、input_tokens 等通用资源管理端点。
- Aether 自有 Response 正文存储、30 天 Application State、response affinity 数据库或跨 provider 资源查询。
- 面向任意 OpenAI SDK 的完整 Responses 平台兼容承诺。
- 与 Codex HTTP 中转无关的管理后台、计费策略、调度策略或前端重构。
