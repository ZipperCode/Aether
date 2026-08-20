# 修复 Responses 流式错误提交

## Goal

避免 Aether 在同格式 OpenAI Responses 流的首段已经表示失败时，先向客户端提交 HTTP 200，导致 Grok 等客户端把裸错误对象按成功 Response 解析并因缺少 `id` 失败。

## Background

- 本机 Grok 1.0.5 使用 `gpt-5.6-sol` 与 `api_backend="responses"`。
- Aether 现场存在同格式 Responses 流式请求：usage 记录为上游 503，但访问日志已记录 200，客户端审计响应顶层仅有 `error`。
- 官方 Responses SSE 失败事件必须使用带完整 Response 的 `response.failed`；成功 Response 必须包含 `id`。
- 当前共享流提交策略对大多数 SSE 仅依据响应头立即提交，跳过已有首段分类逻辑。

## Requirements

- 同格式 `openai:responses` SSE 必须在向客户端提交响应前分类首个完整事件或错误体。
- 首段为可重试错误且尚无客户端可见输出时，必须复用现有候选重试与错误状态逻辑。
- 首段为合法 Responses SSE 时应立即继续流式转发，不得缓冲完整回答。
- 保持原生同格式事件字节、顺序和未知字段不变。
- 不修改 Grok 配置、模型选择、Chat Completions 或 Anthropic 流策略。
- 不通过伪造 `id`、全局关闭流式响应或新增兼容分支掩盖错误。

## Acceptance Criteria

- [x] 同格式 Responses 候选返回 HTTP 200 + SSE，但首段为错误对象时，网关在提交客户端 200 前识别失败。
- [x] 可重试首段错误触发现有候选重试；既有外层候选测试验证该重试信号会执行下一候选。
- [x] 客户端不会收到“HTTP 200 + 顶层仅 `error`、无 `id`”的成功响应形态。
- [x] 首个合法 Responses 事件后仍按现有低延迟路径流式传输。
- [x] 聚焦回归测试与格式检查通过。

## Out of Scope

- 修改 Grok 客户端。
- 为已产生客户端可见输出后的失败重新设计跨提供商 failover。
- 新增 Responses 持久化、资源 API 或全局 SSE 重写器。
