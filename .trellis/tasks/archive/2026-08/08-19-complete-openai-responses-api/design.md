# 技术设计：Codex HTTP Responses 中转

## 1. 设计结论

保留 Aether 现有 create/compact 路由和执行链，只补齐 Codex HTTP 实际使用面的协议保真。原生 `openai:responses` provider 优先走同格式直接转发；其他 provider 仅通过现有 canonical 转换表达其真实能力。Aether 不保存 Response 内容，也不实现资源 ID 的后续管理。

## 2. 客户端契约基线

OpenAI Codex 官方仓库 commit `3929c99a97d1aa0fb8000903a4b57b24fbabe742`：

- HTTP create：`POST /responses`，`store=false`、`stream=true`，请求携带 model、完整 input、tools、reasoning、text 与缓存/服务层参数。
- HTTP request 类型没有 `previous_response_id`；该字段只属于已排除的 Responses WebSocket request。
- Remote compact：`POST /responses/compact`，请求携带 model、完整 input、instructions、tools、parallel_tool_calls、reasoning、service_tier、prompt_cache_key、text，返回 compacted `output`。

官方 Responses 文档用于字段与 SSE schema 证据；Codex 源码决定本产品当前必须消费的端点集合。

## 3. 数据流与所有权

```text
Codex HTTP request
 -> existing explicit create/compact route
 -> control classification + auth + RPM/concurrency admission
 -> model-driven provider selection
 -> native Responses passthrough OR existing canonical conversion
 -> upstream HTTP transport
 -> direct JSON/SSE passthrough OR canonical response emitter
 -> usage/error observer + terminal audit
 -> Codex
```

- Route/control 仍由 gateway 所有，不新增动态资源路径。
- 请求/响应格式语义由 `aether-ai-formats` 所有。
- method、URL、body、header 与同格式透传由 provider transport/execution 所有。
- coverage 文档只审计契约漂移，不成为运行时过滤器。

## 4. 请求保真

重点核对当前 Codex 字段：`model`、`instructions`、`input`、`tools`、`tool_choice`、`parallel_tool_calls`、`reasoning`、`store`、`stream`、`stream_options`、`include`、`service_tier`、`prompt_cache_key`、`text`、`client_metadata`。

同格式路径复制原 JSON 对象，允许未来 Codex 字段自然透传。跨格式路径只映射目标协议能表达的模型语义；传输/观测类字段可按既有策略忽略或消费，但不得把关键输入、工具调用、结构化输出或 reasoning 语义静默删除。

协议相关 headers 复用现有安全转发策略。下游 Authorization/Cookie、hop-by-hop headers 和 Aether 内部凭证不得转发；provider credential 只由 transport 注入。

## 5. 响应与 HTTP SSE

- 原生同格式 JSON/SSE 不经过 canonical 字段白名单。
- direct SSE 保留 `event:`/`data:`、`type`、`sequence_number`、unknown fields 与未来不透明事件。
- 旁路 observer 只提取 usage、request ID、terminal error，不改变原始事件。
- 跨格式 emitter 继续生成 Codex 能消费的 lifecycle、output item、content/text、reasoning、tool argument/result、completed/failed/incomplete/error 事件；不可忠实表达时 fail closed。

## 6. Compact

现有 `/responses/compact` 保持独立格式身份。原生兼容 provider 直接转发当前 Codex payload 与 `output`；跨格式只有在现有实现具备明确等价语义时才转换，不以本地摘要或数据库状态伪装官方 compact。

## 7. 状态、usage 与错误

- HTTP Codex 请求使用 `store=false` 且不使用 `previous_response_id`，因此无需 Response persistence/affinity。
- 每次请求按 body.model 独立选择 provider；现有短期 scheduler affinity 可优化缓存命中，但不承担协议正确性。
- 上游 status、错误 body、request ID 与 terminal SSE 保真；本地认证/准入错误在上游前返回。
- usage 仍按既有请求生命周期记录一次，不从 compact output 或已观察终态重复计费。

## 8. 兼容与回滚

不改变路由集合、数据库或前端。实现应优先增加缺失映射与回归测试；若审计证明现有代码已经满足某项，则只留下最小测试/文档证据。回滚只需撤销本任务的格式映射、测试和审计更新，不影响既有 create/compact 主路径。

## 9. 风险

- 把 coverage matrix 用作 allowlist 会破坏未来 Codex 字段；用 opaque-field fixture 锁定。
- 同格式 SSE observer 若重写 chunk 会破坏未知事件；用原始事件回归锁定。
- 第三方 provider 可能宣称 Responses 兼容但缺字段；能力不足必须暴露真实错误，不做假支持。
- `task.json.package` 仍是 Trellis 创建时的错误默认值；真实 scope 已由 `task.py set-scope` 设为 gateway/formats/transport，不手工猜改 package 字段。
