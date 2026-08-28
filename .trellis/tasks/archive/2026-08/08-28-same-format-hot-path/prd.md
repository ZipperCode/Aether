# 同格式请求与流式热路径优化

## Goal

在不改变 Aether 路由、格式兼容、错误提交、候选重试、审计与 Tunnel 合约的前提下，复用现有原始请求字节保真机制，并消除已确认的请求/流式热路径冗余拷贝。

用户价值：同格式请求不为未发生的格式转换支付额外序列化成本，长流不为只读观察支付不必要的逐块复制成本，同时保持现有生产兼容性和失败边界。

## Background

- 基线为已拉取的 `origin/master` 提交 `59ac360501e36a2086d6af01281f719b34a2253d`。
- 当前通用 same-format planner 已通过 `OriginalRequestPayload` 保留前门规范化后的精确 JSON 字节，并在最终候选 JSON 未变化时选择现有 `RequestBody.body_bytes_b64`。
- OpenAI Chat/Responses 专用 plan builder 尚未统一消费现有精确字节候选；普通 JSON egress 仍在序列化前深拷贝整个 `Value`。
- 非 Responses 的安全 SSE 子集已经存在 direct byte path。原生 Responses 必须在提交 HTTP 2xx 前分类首个完整 body/event，并保持终态兼容、usage、错误和 failover 语义。
- `stream_pump::observe_stream_chunk` 在没有 private normalizer 时仍复制整个 chunk 后立即只读观察。

## Requirements

- R1：OpenAI Chat 与 OpenAI Responses 的同步、流式 plan builder 必须复用现有 base64-over-JSON body resolver；当已有精确字节候选时，生成的 `RequestBody` 只携带 `body_bytes_b64`。
- R2：专用 OpenAI 同格式路径只能在上下游格式等价、最终候选 JSON 与原请求 JSON 相等、未请求重新编码/压缩时选择原始字节；任一条件不满足必须沿用 JSON 序列化路径。
- R3：精确字节指前门完成内容解码后的 JSON entity bytes；不承诺保留客户端原始 gzip/zstd 压缩字节。
- R4：`build_request_body` 序列化 JSON 时不得先深拷贝整个 `serde_json::Value`。
- R5：无 private normalizer 的 stream observer 必须直接借用当前 chunk；有 normalizer 的分支保持现状。
- R6：保持每个候选独立完成模型映射、stream 字段、Body Rule、PII、Codex/Responses 兼容和能力校验；不得共享可变候选 body。
- R7：保持 native Responses 首事件分类、裸错误 precommit 处理、终态兼容、usage/terminal observation、body capture、Agent Bridge、未知字段/事件保留和 failover 边界。
- R8：所有本次新增或修改的手写函数/方法须有实质性中文说明；不新增依赖、配置项或公共 DTO/序列化字段。

## Acceptance Criteria

- [x] AC1：OpenAI Chat sync/stream 与 Responses sync/stream 在“已有精确 base64 + JSON”输入时选择 base64，且 `json_body` 为 `None`。
- [x] AC2：至少一个负向回归证明格式不同、最终 JSON 改变或启用内容编码/压缩时继续选择 JSON，不能发送原始字节。
- [x] AC3：普通 JSON `build_request_body` 与 base64 body 的输出保持原行为，同时 JSON 分支不再克隆完整 `Value`。
- [x] AC4：无 normalizer 的 stream observation 不再执行逐 chunk `to_vec()`，相关 terminal/stream-pump 回归通过。
- [x] AC5：native Responses 首段裸错误仍在客户端 HTTP 2xx commit 前进入现有错误/候选重试路径；Responses compatibility 与未知字段/事件测试通过。
- [x] AC6：`cargo fmt --all --check` 通过；所有范围化测试实际匹配非零用例并通过。
- [x] AC7：工作区仅包含本任务产品代码、测试、规范、API 文档和 Trellis 工件改动；无临时服务、缓存或调试产物残留。

## Out of Scope

- 新增 `RequestBody` raw-bytes 字段、修改 Tunnel/远程执行序列化合约，或消除现有 base64 编解码。
- 删除请求体有界缓冲、JSON 解析、候选隔离或任何安全/兼容校验。
- 新增 Responses classified direct-passthrough；当前没有可信 capability 能证明终态兼容重写可安全跳过。
- 修改 Full usage body capture 上限、prefetch 多缓冲结构、WebSocket/Realtime/Live 路径。
- 新增 Criterion 或其他 benchmark 依赖，或在没有可比 A/B 运行证据时宣称固定百分比性能提升。

## Technical Notes

- 请求研究：`research/request-byte-fidelity.md`。
- 流研究：`research/stream-fast-path.md`。
- 验证基线：`research/validation-baseline.md`。
- 阻塞性开放问题：无。当前本机没有已配置的同格式 mock Gateway 运行态，因此本轮以确定性分配消除和范围化正确性回归作为完成证据；不作未经测量的吞吐声明。
