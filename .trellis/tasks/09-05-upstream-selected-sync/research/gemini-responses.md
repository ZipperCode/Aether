# Research: Gemini / Responses category 5 sync

- Query: 分析上游 `66d6c17d2`、`14744abd5` 的最小同步面、依赖与本地契约冲突。
- Scope: mixed
- Date: 2026-09-05

## Findings

### 上游行为与文件

- `66d6c17d2d80ddebf05c682f06793818f3b8158d`（父提交 `18d78dd`）改 4 个文件：
  - `apps/aether-gateway/src/execution_runtime/stream/commit_policy.rs`：非空 `thought:true` 文本成为客户端可见首输出；纯签名/空 thought 仍等待。当前本地仍在 `:429-505,512-537,685-735` 把 thought 全部视为不可见，故会错误等待并把后续错误改成候选重试。
  - `crates/aether-ai/formats/src/formats/gemini/generate_content/stream.rs`：识别 `MALFORMED_FUNCTION_CALL`、`UNEXPECTED_TOOL_CALL`、`TOO_MANY_TOOL_CALLS`、`MISSING_THOUGHT_SIGNATURE`、`MALFORMED_RESPONSE`，输出完整 `response.failed`（`id/model/status/error`，有则带 usage），置 `finished=true`，不再合成成功 Finish。当前本地 `:423-465` 仍发不受支持的 Finish。
  - `crates/aether-ai/formats/src/formats/shared/stream_core/format_matrix.rs`：Responses/Chat 保留上述终态帧；其他目标转成其错误格式；终态观察器从 `/response/usage` 记账。当前本地 `:115-152,350-390,603-642,808-848` 会把它降级为 `unsupported_*`。
  - `apps/aether-gateway/src/execution_runtime/stream/execution.rs:11649-11734` 的回归应从“无响应 + Candidate 重试”改为“HTTP 200 已含 reasoning delta，随后同流 `response.failed`；retry scope 不切到 Candidate”。若错误在任何可见输出前到达，现有提交门仍可返回结构化 502/执行既有 failover。
- `14744abd573cbf3cacaaf5c8fda41f9ebbe46db8` 的父提交就是 `66d6c17d2`，改 2 个文件：
  - `crates/aether-ai/formats/src/formats/openai/responses/mod.rs:121-192`：Gemini thought-signature carrier 虽借 `encrypted_content` 携带，却不是 OpenAI/Codex 密文；回放到 Responses 上游前必须按 `GEMINI_TOOL_SIGNATURE_CARRIER_PREFIX` 删除。保留真正 provider ciphertext、合法 `rs*` reasoning 和非 reasoning item。
  - `crates/aether-ai/formats/src/formats/openai/request_contract.rs:381-414`：扩充最终化回归，证明 carrier 被删。

### 依赖、冲突与最小顺序

- 严格依赖仅为现有 Gemini/Responses carrier、流转换、提交门和错误辅助函数；无新增 crate、Provider Pool/OAuth/模型目录依赖。类别 4 提交在两补丁之后，非编译/语义前置；它可能同改巨型 `execution.rs`，只需人工合并测试段。
- 顺序：先人工适配 `66d6c17d2` 的 4 文件行为，再适配 `14744abd5` 的 2 文件过滤；不要整提交覆盖本地文件。
- 必须保留：原生同格式 Responses 未知字段/事件透传；首个裸错误在提交 2xx 前分类；已输出后的完整 `response.failed` 不拼接第二 Provider 流；跨格式不可表示语义继续 typed fail-closed；不引入 `previous_response_id`、Response 持久化或 Nightly。
- 上游英文/无注释代码不能原样落地。所有新增或修改的函数、方法与测试需实质中文说明，至少覆盖新 helper、`emit_frames`/终态发射方法、replay 判定及改名/新增测试。

### 最小验证

```powershell
cargo test -p aether-ai-formats gemini_provider_state_emits_terminal_error_for_malformed_function_call --lib
cargo test -p aether-ai-formats streams_gemini_thought_text_to_openai_responses_immediately --lib
cargo test -p aether-ai-formats transforms_malformed_gemini_function_call_to_responses_failed --lib
cargo test -p aether-ai-formats strips_gemini_signature_carriers_before_openai_replay --lib
cargo test -p aether-ai-formats finalization_strips_non_replayable_responses_reasoning_history --lib
cargo test -p aether-gateway gemini_gate_commits_on_first_nonempty_thought --lib
cargo test -p aether-gateway malformed_antigravity_function_call_streams_thought_then_fails_in_band --lib
cargo test -p aether-gateway same_format_responses_prefetch_retries_bare_error_before_committing_success --lib
cargo fmt --all --check
```

### External references / related specs

- 上游补丁：[66d6c17d2](https://github.com/fawney19/Aether/commit/66d6c17d2.patch)、[14744abd5](https://github.com/fawney19/Aether/commit/14744abd5.patch)。
- `.trellis/spec/aether-ai-formats/backend/codex-http-responses-contract.md`；`.trellis/spec/aether-gateway-execution/backend/codex-logical-identity-contract.md`。

## Caveats / Not Found

- 未执行 Git、构建或测试；结论来自当前源码、CodeGraph 与上游补丁。`18d78dd` 是合并/夹具提交，不应作为类别 5 依赖整包纳入。
