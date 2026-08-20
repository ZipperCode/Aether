# 实施计划

1. 在 `StreamCommitPolicy::for_response` 中让同格式 OpenAI Responses SSE 使用 `FirstClassifiedBody`。
2. 在现有 stream decision 测试附近增加一个聚焦回归：首候选 200/SSE 首段错误，下一候选成功。
3. 验证客户端只收到合法 Responses SSE，且失败候选仍进入既有重试/usage 路径。
4. 运行目标测试；随后运行 `cargo fmt --all --check`。
5. 检查 diff 仅包含任务工件、提交策略与聚焦测试，保留用户已有 `.trellis/workflow.md` 修改。

## 验证结果

- `cargo test -p aether-gateway policy_prefetches_same_format_openai_responses_sse_only --lib`：1 passed。
- `cargo test -p aether-gateway same_format_responses_prefetch_retries_bare_error_before_committing_success --lib`：1 passed。
- `cargo test -p aether-gateway gateway_retries_next_local_openai_chat_stream_candidate_after_retryable_429_execution_runtime_status --lib`：1 passed，复用既有外层候选循环证据。
- `cargo fmt --all --check`：通过。
