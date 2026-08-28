# 同格式请求与流式热路径优化设计

## 1. Design Summary

保留现有端到端数据流，只在已经完成所有候选变换之后选择更便宜的 wire representation：

```text
frontdoor bounded bytes
  -> parsed JSON + OriginalRequestPayload
  -> per-candidate final JSON
  -> eligibility check
       unchanged native JSON -> existing body_bytes_b64
       otherwise             -> existing json_body
  -> shared build_request_body
```

流式侧不建立新管线，只删除 observer 无 normalizer 分支中“复制后立即借用”的单次 chunk copy。

## 2. Ownership and Boundaries

- `crates/aether-ai/serving` 继续拥有 `OriginalRequestPayload` 的“最终 JSON 等于原 JSON才返回精确字节”判定。
- `apps/aether-gateway` 的 OpenAI plan builder 负责在候选全部修改完成后，结合格式与编码条件选择已有 `RequestBody` 表示。
- `ExecutionPlan` / `RequestBody`、Tunnel、远程执行协议不变。
- `aether-ai-formats` 的 Canonical、Responses stream compatibility 和 fail-closed 规则不变。

## 3. Request Path

### 3.1 Shared reuse

OpenAI sync/stream plan builder 使用现有 `resolve_ai_passthrough_sync_request_body`，不再各自直接构造 `RequestBody::from_json`。若需要从当前 HTTP request extension 生成精确 base64，逻辑必须由 sync/stream 共用，不能复制四份条件判断。

资格条件：

1. client/provider API format alias 等价；
2. 决策尚未提供精确 base64；
3. `OriginalRequestPayload` 存在且最终候选 JSON 与原 JSON 相等；
4. 没有显式 `content_encoding`；
5. request gzip 未启用，也不存在需要保守视为启用的未知策略。

判定发生在所有候选 body/routing 修改之后、body 被 move 进 `RequestBody` 之前。这样模型映射、Body Rule、PII 或兼容修正只要改变 JSON 就会自然回退。

### 3.2 Transport serialization

`build_request_body` 对 `plan.body.json_body.as_ref()` 直接调用 `serde_json::to_vec`。该函数只读输入，不需要 clone；压缩、base64 与错误类型保持不变。

## 4. Stream Path

`observe_stream_chunk` 的无 private normalizer 分支直接把 `chunk: &[u8]` 交给现有 observer。normalizer 分支仍产生拥有所有权的规范化 bytes。

本轮不允许把 Responses 加入现有 direct passthrough predicate。现有 `FirstClassifiedBody`、bare-error handler、终态兼容 rewriter、usage/terminal observer 与 body capture 均保留。

## 5. Compatibility and Failure Behavior

- 原始字节优先时，`RequestBody.json_body` 必须为 `None`，避免双重来源。
- 编码/压缩请求必须使用 JSON 分支，因为 transport 只会对 JSON variant 执行 gzip/zstd。
- 缺失 `OriginalRequestPayload`（远程决策、序列化回放等）时透明回退 JSON。
- 每个候选独立判定，不缓存跨候选的最终 body/base64。
- native Responses 首段错误、post-output terminal failure 和重试策略完全不变。

## 6. Trade-offs and Deferred Work

- 保留 base64 encode/decode：这是当前可序列化执行合约的成本；移除它需要新 DTO/跨进程契约，收益未量化。
- 保留入口 JSON clone/bytes capture：跨 parser、request extensions、planner 的所有权调整会显著扩大写集。
- 暂缓 Responses classified direct pipeline：只有 profile 显示 frame/base64/rewrite replay 至少造成可复现的 10% 主要成本，并能设计完整错误/终态等价证明时再单独立项。

## 7. Rollback

所有产品改动均为局部选择或借用优化。回滚顺序：先撤销 OpenAI exact-body 选择，再撤销 stream observer 借用，最后撤销 transport clone removal；无需数据迁移或配置恢复。
