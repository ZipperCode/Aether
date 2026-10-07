# Responses 本地计划诊断修复设计

## Decision

保留 `model_endpoint_bindings`。它是候选查询中区分 URL、协议、认证、Header/Body 规则和 operation 能力的唯一精确边界；删除它会把 Search、Compact、Image、Live 和普通 Responses 混成同一候选池，风险高于当前问题。

## Root cause boundary

154 的 `gpt-6-astra` 有一个 Provider Model 和三个 discovered 绑定：`openai:responses`、`openai:responses:compact`、`openai:search`。普通 `/v1/responses` 候选 SQL 会按 Endpoint `api_format` 精确筛选，因此这些绑定本身不应让 Search/Compact 进入普通 Responses 候选。

同一失败 trace `fcc46626-c074-4346-a28d-e3927ec71831` 能证明的边界是：请求已完成 body buffer，但没有 Provider/Key、没有 request_candidates 行、没有 execution-runtime 日志；相邻相同模型/账号/会话请求可成功并选择 Codex Key。候选统计为 `candidate_count=1, persisted=0, skipped=0`，对应一个 PoolGroup。失败发生在执行前的候选构造阶段，且没有任何具体诊断落库。

该 trace 只能确立 pre-execution failure，不能证明具体失败分支：失败请求正文采集关闭，无法区分 plan-builder `Ok(None)`、号池 Key 被静默运行时隔离（quarantine）或其他 pre-execution 丢弃点；也不能证明 Endpoint 绑定故障。修复覆盖全部已知会造成该签名的静默丢弃点，不把任何单一分支断言为历史根因。

### Verified data paths（代码走查，2026-10-08）

- 直接标记路径 `mark_skipped_local_openai_responses_candidate_with_failure_diagnostic` → `mark_skipped_local_execution_candidate_with_failure_diagnostic` → `persist_skipped_local_execution_candidate` → `request_candidate_runtime::persist_skipped_local_candidate` 只受 `should_persist_request_candidate_status(Skipped)`（request_record_level）与写入器可用性约束，不经过 `should_persist_skipped_local_candidate` 的号池过滤；号池过滤只作用于批量端口路径（`GatewaySkippedCandidatePersistencePort::should_persist_skipped_candidate`）。"per-key skipped 行会被号池过滤丢掉"的说法对直接路径不成立——修复前真正缺失的是 `Ok(None)` 分支没有调用任何标记。
- `LocalExecutionRequestOutcome::Exhausted` 仅在 attempt loop 的 `last_attempted` 为 Some（至少一个候选被真正尝试）时产生（`executor/candidate_loop.rs` 循环尾部）；`NoPath` 不会被新增的 exhaustion 诊断 setter 覆盖。
- 号池游标耗尽（全部 Key 被并发/冷却等运行时条件跳过）已有单条聚合 skipped 行 + `pool_group_exhaustion` extra_data 的直写路径（`persist_pool_group_exhaustion_skipped_candidate`）。
- 普通文本请求 resolver 的 `None` 站点已经标记 skip；不再重复标记。图像桥接的独立静默返回不在本次文本 Responses 修复范围内。

## Proposed change

1. OpenAI Responses stream plan builder 返回类型化分类错误；attempt source 通过既有直写 seam 记录候选身份和安全诊断。已诊断的 payload `None` 不重复记录；不记录认证值、完整 URL 或正文。
2. PoolGroup 中所有具体 Key 都失败后，记录聚合的安全失败原因，不再只报告 `no_local_stream_plans`；PoolGroup 的 available/skipped persistence 规则保持其成本语义，但必须保留请求级诊断。
3. 对池内并发限制、计划构造失败和真正候选耗尽分别测试，避免把运行时失败误报为绑定错误。
4. 保持自动发现只消费上游明确的 `endpoint_ids`，manual 绑定优先。

## Compatibility

- 不打开全局格式转换。
- 不改变普通候选查询的 exact Endpoint format join。
- 不改变 Search/Compact/Image/Live 的专用操作路由。
- 不删除 binding 表、字段或迁移。

## Rollback

代码回滚为单次 commit 回滚；生产配置修复若需要，先记录 binding 快照，再执行可逆的 active 状态或 binding 删除/恢复操作。
