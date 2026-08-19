# 实施计划：Codex HTTP Responses 中转

## 0. 已通过门禁

- [x] 用户批准 Codex HTTP 中转范围：create + compact + HTTP SSE，无持久化、无资源管理端点。
- [x] 任务已处于 `in_progress`，scope 为 `aether-gateway,aether-ai-formats,aether-provider-transport`。
- [x] 进入代码修改前重读 Trellis specs、本文和实时工作区，并先用 CodeGraph 定位所有调用方。

## 1. 建立 Codex HTTP 契约差距矩阵

- 用当前 Codex request/compact structs、官方 Responses 字段与 SSE reference 对照 Aether request parser、canonical model、same-format transport、response emitter 和测试。
- 将每项标为 already-preserved、mapped、intentional transport-only、fail-closed gap；不因字段存在于文档就盲目新增运行时模型。
- 验收：得到可定位到 symbol/file:line 的实际改动清单，无 5 个资源端点或 affinity 遗留。

## 2. 完善 create 请求路径

- 优先复用同格式 JSON 对象透传；只修真实字段丢失或错误改写。
- 跨格式补齐 Codex 必需且目标协议可表达的 input/tool/reasoning/text/cache/service-tier 语义；不可表达的关键能力返回结构化错误。
- 核对协议 header 与 credential 隔离，不扩大敏感 header 转发。
- 最小测试：当前 Codex payload、opaque future field、tool result、reasoning/text controls、错误转换。

## 3. 完善响应与 HTTP SSE

- 锁定同格式同步 unknown response field 和 opaque SSE event/field 直通。
- 补齐跨格式 Codex 消费的 lifecycle、text、reasoning、tool、terminal event 缺口。
- 验证 usage/error observer 不重写事件，不重复终态或计费。
- 最小测试：标准流、未知事件、terminal error、代表性 upstream status/body/request ID。

## 4. 完善 compact

- 对照当前 Codex `CompactionInput` 核对 model/input/instructions/tools/parallel_tool_calls/reasoning/service_tier/prompt_cache_key/text。
- 原生 provider 保真返回 `output`；跨格式仅保留已有真实等价能力。
- 最小测试：完整当前 payload、unknown field、成功 output、400/5xx 错误。

## 5. 更新审计文档

- 更新 `docs/api/provider-interface-definitions.md` 与相关 coverage/audit，使 create/compact 的当前字段和事件可追踪。
- 生成 matrix 只能通过项目生成器更新，并明确其非 runtime allowlist。
- 不新增 retrieve/delete/cancel/input_items/input_tokens 条目为“已支持”。

## 6. 最小验证

先按实际修改符号运行最窄测试；预期候选：

```powershell
cargo test -p aether-provider-transport same_format_responses
cargo test -p aether-ai-formats openai_responses
cargo test -p aether-gateway ai_serving::finalize
python docs/api/generate_format_field_coverage.py --check
cargo fmt --all --check
```

若过滤名与实际测试不符，先列出/定位对应测试再运行，不用空过滤结果冒充通过。只有最小验证暴露共享契约风险时扩大到对应 crate；不运行前端验证或默认 workspace 全量测试。

## 7. 检查与回滚

- `trellis-check` 复核 PRD、同格式 unknown-field/SSE、跨格式 fail-closed、credential/header、usage/error 与 compact。
- 高风险文件是 Responses request/stream converter、same-format transport、execution stream observer 和 coverage generator。
- 任一步失败只回滚本任务变更，保留现有 create/compact 路径；禁止以数据库、兼容 shim 或静默降级收口。

## 8. 实施结果与证据

- 代码调查未发现需要修改的产品逻辑；同格式 JSON/SSE、header 隔离、compact 和跨格式 fail-closed 已由共享实现提供。
- 本任务只扩展三处既有测试模块，锁定当前 Codex create payload、opaque request/response/SSE、credential 隔离、compact output/error 和 terminal failover 边界。
- 新增或扩展的 6 个精确过滤器均实际运行 1 项并通过；另复用 5 个现有精确测试验证 compact 投影/响应、不可表达工具结果 fail-closed 及 Claude/Gemini SSE 转换。
- 通用 HTTP 边界的 upstream 429 status/body 与 terminal request-id 记录各有 1 个精确测试通过。
- `cargo fmt --all --check`、coverage generator `--check`、`git diff --check` 与 Trellis context validation 均通过。
- 没有产品源码、路由、数据库、前端、依赖或生成矩阵改动。
