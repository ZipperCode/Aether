# 同格式请求与流式热路径优化实施计划

## 1. Pre-development

- [x] 加载 `trellis-before-dev` 与 aether-gateway / aether-ai-serving 相关规范。
- [x] 用 CodeGraph/调用者搜索复核最新 `59ac36050` 的 OpenAI plan builder、`OriginalRequestPayload`、`build_request_body` 和 `observe_stream_chunk`。
- [x] 记录实施前工作区状态，保护无关改动。

## 2. Request optimization

- [x] 在 OpenAI plan builder 共享边界复用现有 exact-body/equality 机制，避免 sync/stream 条件漂移。
- [x] Chat sync/stream 与 Responses sync/stream 改用现有 base64-over-JSON resolver。
- [x] 添加正向回归：已有精确 base64 时四条 builder 路径选择 `body_bytes_b64`。
- [x] 添加负向回归：跨格式、最终 body 变化或编码/压缩时回退 JSON。
- [x] 在 `build_request_body` 中借用 JSON，删除序列化前深拷贝。

## 3. Stream optimization

- [x] 无 private normalizer 的 observer 分支直接借用 chunk，normalizer 分支不变。
- [x] 不修改 Responses direct-passthrough eligibility、prefetch、commit policy 或 compatibility rewriter。

## 4. Focused validation

- [x] 运行新增 exact-body / OpenAI plan-builder 聚焦测试，并确认非零匹配。
- [x] 运行 `cargo test -p aether-ai-serving payload_fidelity -- --nocapture`。
- [x] 运行 `cargo test -p aether-gateway --lib policy_prefetches_same_format_openai_responses_sse_only`。
- [x] 运行 `cargo test -p aether-gateway --lib same_format_responses_prefetch_retries_bare_error_before_committing_success`。
- [x] 运行与 stream-pump observer 改动对应的最小模块测试。
- [x] 运行 `cargo fmt --all --check` 与 `git diff --check`。

## 5. Quality review and closeout

- [x] 由 `trellis-check` 独立核对规格、调用链、测试和无越界改动。
- [x] 若检查发现共享契约风险，仅修复本任务引入的问题并重跑失败项。
- [x] 更新任务实施/检查证据；不在无 service-backed A/B 的情况下声明固定性能百分比。

## Validation Evidence

- `trellis-check`: PASS；44 次通过执行，39 个唯一用例。
- exact-body builder/门禁：4/4；AI serving payload fidelity：2/2；resolver：2/2。
- stream pump：10/10；Responses commit policy：1/1；bare-error precommit：1/1；Codex failover：2/2。
- provider opaque fields：1/1；same-family stream/terminal：7/7；same-family sync/error：8/8；JSON compression transport：1/1。
- `cargo fmt --all --check` 与 `git diff --check` 均通过。
- 未运行 service-backed A/B；因此只声明已消除确定性 clone/copy，不声明量化吞吐或延迟提升。

## Risky files / rollback points

- `apps/aether-gateway/src/ai_serving/planner/standard/openai/plan_builders/**`：错误选择 raw body 可能绕过候选修改或压缩。
- `apps/aether-gateway/src/execution_runtime/stream_pump.rs`：必须保持 observer 输入生命周期和终态语义。
- `apps/aether-gateway/src/execution_runtime/transport.rs`：共享于 direct/tunnel sync/stream，clone removal 只能是等价借用。
