# 技术设计

## 边界

修复位于 `StreamCommitPolicy::for_response` 的共享提交决策。仅当上游与客户端均为 `openai:responses` 且响应为 SSE 时，把提交策略从 `ResponseHeaders` 改为 `FirstClassifiedBody`。

## 数据流

```text
上游响应头与 body stream
→ StreamCommitPolicy::for_response
→ 预取首个完整事件（有界帧数/字节数）
→ inspect_prefetched_stream_body
→ 错误：保留非 2xx/候选重试
→ 合法事件：提交响应并继续原字节流
```

## 契约

- 原生同格式 SSE 的事件名、顺序、字段和未知事件保持原样。
- 首段错误发生在 HTTP 提交前，应优先保留真实错误状态或重试，而不是合成成功 Response。
- 已经出现客户端可见输出后的终止错误继续使用现有流内错误路径；如需合成 Responses 失败事件，必须复用已有标准 `response.failed` 生成能力。

## 取舍

- 同格式 Responses SSE 增加最多一个完整事件的首字节等待。
- 复用现有有界预取和分类函数，不增加新抽象、配置项或缓存。
- Chat Completions、Anthropic 和非 SSE 响应保持当前策略，缩小影响面。

## 回滚

回滚提交策略的 Responses 特例及对应测试即可恢复旧行为，不涉及数据迁移或配置变更。
