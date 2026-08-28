# Research: same-format stream fast path

- Query: 调查当前同格式流的字节直通、首段分类直通候选与重写路径，判断能否安全加入最小 Responses classified passthrough，并给出测试和基准建议。
- Scope: internal
- Date: 2026-08-28

## Findings

### 结论

当前可以安全保留并继续使用的是非 Responses 的既有 direct byte path；把原生同格式 `openai:responses` 仅通过修改 eligibility/predicate 切到该路径并不安全，应暂缓（YAGNI）。原因不是首段分类本身做不到，而是首段通过后仍有不可省略的终态兼容重写、错误/用量观察、正文捕获、候选 failover 以及可选 Agent Bridge 状态处理。当前代码没有“上游保证现代终态字段完整”的可信 capability，因此 classified passthrough 的安全可用集合为空。

建议本轮若需要一个最小、可证明不改语义的流热路径改动，只去掉 `stream_pump::observe_stream_chunk` 在没有 private normalizer 时的整块 `chunk.to_vec()`：当前代码复制后立即以 `&[u8]` 交给 observer（`apps/aether-gateway/src/execution_runtime/stream_pump.rs:1139-1160`），可直接借用原 `chunk`。不要在本轮新增 Responses 直通分支、配置项或依赖。

### 三种路径必须明确区分

1. **Byte passthrough（现有 direct path）**

   - `should_use_direct_sse_passthrough` 只接受 2xx、SSE、上下游格式相同、非 image、非任一 Responses family、客户端不允许代理控制块、无 private normalizer、无 local rewriter，并要求提交策略可在响应头提交（`apps/aether-gateway/src/execution_runtime/stream/execution.rs:1549-1597`）。两个 in-process 调用点满足时直接进入 `execute_stream_from_direct_passthrough`；否则把响应包装成 frame stream 进入通用路径（同文件 `4430-4484`, `4559-4618`）。
   - direct inline body 直接消费 `Bytes`，但并非完全透明：它仍过滤上游 SSE comment/control block，观察 provider/client chunk，并可能生成 terminal error event（同文件 `2433-2650`）。因此准确描述应是“业务 SSE data/event 字节直通，保留控制块过滤与终态副作用”。
   - direct finalizer 仍拥有 usage/terminal observer、provider error inspector、provider/client body buffers、完成跟踪与 telemetry（同文件 `1727-1766`）；每个 provider chunk 会做捕获、usage 观察和流内错误观察，每个 client chunk会做捕获与完成跟踪（同文件 `1871-1953`）。结束时仍提交失败/取消/成功 usage、候选状态、健康效果和 report（同文件 `2091-2429`）。这些职责不能因“直通”而删除。

2. **Classified passthrough（候选，当前不应新增）**

   - `StreamCommitPolicy` 的三态是 `ResponseHeaders`、`FirstClassifiedBody`、`FirstAnthropicSemanticEvent`（`apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs:9-17`）。原生同格式 Responses SSE 被明确设为 `FirstClassifiedBody`（同文件 `20-49`），而普通同格式 SSE 才可 `ResponseHeaders`（同文件 `50-79`）。
   - 通用路径最多预取 5 帧、16 KiB；有 rewriter 时还受 750 ms precommit deadline 约束（`crates/aether-gateway/execution/src/limits.rs:2-3`, `apps/aether-gateway/src/execution_runtime/stream/execution.rs:166`, `6725-6793`）。它同时保留 provider bytes 和 inspection bytes（同文件 `6918-6929`），用 `inspect_prefetched_stream_body` 分类首个有意义 JSON/data 行（`apps/aether-gateway/src/execution_runtime/stream/error.rs:130-191`）。
   - 首段为 bare embedded error 且上下游均为 `openai:responses` 时，代码在下游 2xx commit 前调用既有 provider-error/failover handler（`apps/aether-gateway/src/execution_runtime/stream/execution.rs:6993-7050`）。这一边界来自真实历史故障，不能改成先返回 200 再观察。
   - 仅把 `should_use_direct_sse_passthrough` 的 Responses 排除删除会绕过上述 handler，而且 direct 函数当前没有 `retry_scope_out` / `retry_fallback_out`，无法保持相同候选 failover 合同。因此 predicate-only change 明确不安全。

3. **Rewrite（当前 Responses 路径）**

   - same-family Responses 无论是否 request conversion，当前 resolver 都选择 `OpenAiResponsesCompat`（`crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs:197-204`）。rewriter 按换行缓冲 chunk（同文件 `250-337`），对 `data:` JSON 做 model directive rewrite，并在 `response.completed` / `response.done` 上补现代 Response 字段（同文件 `788-829`）。
   - 终态兼容不是空操作：缺失或类型错误的 `output` 会被补为数组，并可能补 `created_at`、`completed_at`、`output_text`（`crates/aether-ai/formats/src/formats/openai/responses/response.rs:449-486`）。对应测试明确要求同 family Responses 的非终态 opaque 内容保留、终态字段补齐（`crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs:1248-1306`）。
   - 通用执行在 prefetch 期间已经 normalize / Agent Bridge capture / rewrite（`apps/aether-gateway/src/execution_runtime/stream/execution.rs:7162-7279`）；建立 response 后又重建这些状态并 replay prefetched provider body（同文件 `7458-7725`），随后每个 frame 再经历 base64 decode、provider capture、normalize、provider-error observer、usage observer、Agent Bridge capture、local rewrite、client capture 和 channel send（同文件 `7825-8088`）。这是静态可确认的热路径，但尚无 profile/benchmark 证明哪一项占主导。

### 已确认的性能成本（代码证据，不等于运行时测量）

- in-process 非 direct 流先在 `build_direct_execution_frame_stream` 观察终态，再把每个 chunk base64 编码进 NDJSON `StreamFrame`（`apps/aether-gateway/src/execution_runtime/stream_pump.rs:34-76`, `240-255`, `610-675`）；通用执行随后 LinesCodec 解 frame 并 base64 decode（`apps/aether-gateway/src/execution_runtime/stream/execution.rs:6000-6056`, `6867-6897`, `7863-7885`）。direct path 不承担这轮 frame/base64 往返。
- `stream_pump::observe_stream_chunk` 在没有 private normalizer 的普通同格式流上仍执行 `chunk.to_vec()`，随后只把该 Vec 借给 `observe_normalized_bytes`（`apps/aether-gateway/src/execution_runtime/stream_pump.rs:1139-1160`）。这是可直接消除、且不改变解析/错误/捕获语义的一次每-chunk 分配与拷贝。
- Responses compat rewriter 对换行完成的 `data:` 行做 JSON parse；未修改时返回原 line，修改时重新序列化（`crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs:788-829`）。同时 terminal usage observer 也逐行解析（`crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs:219-313`; gateway 的 bounded line buffer 在 `apps/aether-gateway/src/execution_runtime/stream/execution.rs:946-1009`）。不能假设移除 rewriter 就能移除协议解析。
- body capture 受现有 policy/limit 约束，direct 和通用路径均分别保存 provider/client 视图；直通优化不能把它降成单份或无捕获（direct 初始化见 `apps/aether-gateway/src/execution_runtime/stream/execution.rs:3059-3063`, `3134-3173`，观察见 `1894-1953`）。

### 最小建议与 YAGNI 触发条件

- **本轮推荐**：只改 `stream_pump::observe_stream_chunk` 的无-normalizer 分支，直接调用 `observe_normalized_bytes(..., chunk)`；normalizer 分支保持现状。该改动复用现有 observer，不新增 helper/config/dependency。
- **本轮暂缓**：Responses classified direct passthrough。要做到正确，至少要把首段 classifier + existing failover handler 接入 direct byte stream，并继续支持 terminal compat rewrite、model directive、provider error/usage observers、body capture、private envelope 与 Agent Bridge；这已经不是 predicate-level 最小改动。
- **可量化触发**（建议阈值，不是当前测量结果）：在同样 event 数量/字节数、同样 capture policy 下，Responses 通用路径相对现有 direct 同格式基线出现至少 10% 的 CPU/request 或吞吐差距，并且 profile 将主要成本定位到 frame/base64/rewrite replay 时，再立独立任务设计 classified+rewrite-capable direct pipeline。

### Targeted tests

若只做无-normalizer copy elimination：

- 运行 `cargo test -p aether-gateway stream_pump --lib` 中现有 direct frame stream/terminal summary 覆盖；若过滤名不可用，运行该模块最小相关测试，不需全 workspace。
- 保留 `cargo test -p aether-gateway policy_prefetches_same_format_openai_responses_sse_only --lib`。
- 保留 `cargo test -p aether-gateway same_format_responses_prefetch_retries_bare_error_before_committing_success --lib`。
- 保留 `cargo test -p aether-gateway prefetched_codex_cyber_policy_violation_stops_failover_by_default --lib` 与 enabled-retry 版本，验证 opaque event/bytes 与 failover 边界（现有断言见 `apps/aether-gateway/src/execution_runtime/stream/execution.rs:11493-11537`）。
- 运行 `cargo fmt --all --check`。

若未来实现 classified direct path，还必须新增：

- bare error 在每个 transport chunk boundary 切分时都在 commit 前识别；不要只测单 chunk。
- 首个合法 Responses event 后 unknown event/unknown field 逐字保持；terminal compat 缺字段仍补齐；有 model directive 时仍重写。
- `response.failed` 出现在 client-visible output 后不切换候选；pre-output 且策略允许时才 failover。
- Full / Basic / None body capture policy 下 provider/client capture、truncated state 与 usage terminal summary 与旧路径一致。
- private envelope、local rewriter、Agent Bridge 任一存在时不进入纯 passthrough。

### Targeted benchmark

仓库已有 `gateway_openai_chat_c_compare`，能采集 `frontdoor_to_stream_first_client_yield`、`stream_first_data`、`stream_first_client_yield`、`stream_total` 等 stage metrics（`crates/aether-testing/integration/src/bin/gateway_openai_chat_c_compare.rs:604-640`），但它只覆盖 OpenAI Chat；`llm_stream_stability_baseline` 更偏传输稳定性。最小基准方案是复用前者的 harness 增加一个 `openai:responses` mock 场景，而不是新建 benchmark 框架。

比较时固定请求数、并发、chunk 数/大小、首字节延迟和 capture policy，分别记录 first-client-yield p50/p95/p99、requests/s、CPU/request、分配次数/bytes，并做 downstream SHA-256/逐字相等检查（terminal compat 需要单独以规范化后的期望体比较）。不要只用 TTFB 判断，因为本优化主要影响持续 chunk 处理和终态。

### Files found

- `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs` — 提交时机、Anthropic gate 与 Responses 首段分类策略。
- `apps/aether-gateway/src/execution_runtime/stream/error.rs` — prefetch body/error 分类器。
- `apps/aether-gateway/src/execution_runtime/stream/execution.rs` — direct predicate/finalizer、通用 prefetch/rewrite/observer/report 主路径及回归测试。
- `apps/aether-gateway/src/execution_runtime/stream_pump.rs` — in-process upstream bytes 到 NDJSON frame 的桥接、首轮 terminal observer 和每 chunk base64 编码。
- `crates/aether-ai/formats/src/formats/shared/stream_rewrite.rs` — Responses compat mode 选择与逐行重写。
- `crates/aether-ai/formats/src/formats/openai/responses/response.rs` — 现代 Responses 终态字段补齐。
- `crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs` — streaming terminal/usage observer。
- `apps/aether-gateway/src/ai_serving/agent_bridge.rs` — Agent Bridge stream capture 是有状态 record 处理，不能被 raw passthrough 绕过（`882-948`）。
- `crates/aether-gateway/execution/src/limits.rs` — 预取帧数与字节上限。
- `crates/aether-testing/integration/src/bin/gateway_openai_chat_c_compare.rs` — 可复用的 gateway stage-metric benchmark harness。

### Related specs and history

- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md:34-47`：native same-format 必须保留 unknown JSON fields。
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md:57-63`：native SSE 保留事件/顺序/未知字段，observer 可观察但不可改写；首完整 body/event 必须在下游 2xx 前分类。
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md:65-77`：upstream status、首段 embedded error、post-output terminal error 与 failover matrix。
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md:89-101`：现有聚焦回归与最小验证要求。
- 历史任务 `f86e9941a` 将 same-format Responses SSE 改成 `FirstClassifiedBody`，并在 check 阶段把过宽的“所有 FirstClassifiedBody 都候选重试”收窄到原生 Responses。该历史结论已由当前代码与 spec 重新验证，不能通过性能改动撤销。

### External references

- 本次未做网络资料查询；没有引入新的外部协议假设。
- 当前项目 contract 记录的 Codex HTTP consumer baseline 是 OpenAI Codex commit `3929c99a97d1aa0fb8000903a4b57b24fbabe742`（`.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md:15`）。扩展或删除字段前需另行刷新该外部基线。

## Caveats / Not Found

- 没有运行 profile 或 load benchmark；“hot spot”是由确定存在的 per-chunk copy、base64/NDJSON 往返、重复 observer/replay 和 JSON parse 静态确认，不能声称它们已按 CPU 占比排序。
- 当前 task 的 `prd.md` 仍是 TBD；本研究以 dispatch 中明确的已批准边界与现有 executable spec 为准。
- `inspect_prefetched_stream_body` 的现有回归只覆盖单 chunk bare error；未来若新建 classified direct path，chunk-boundary completeness 必须由新增测试证明，不能从现有测试推断。
- 没找到能够证明某 provider 的 terminal Response 永远已含 `output`、`created_at`、`completed_at`、`output_text` 的当前 capability/contract；因此不能安全关闭 `OpenAiResponsesCompat`。
