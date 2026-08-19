# Technical Design

## Decision Summary

- 固定内部阈值为 `1.0`，不新增配置。
- 余额 eligibility 与订阅 quota exhaustion 分离，避免改变现有开关和 hard-state 语义。
- 余额解析只在 provider-pool 中实现一次，普通候选和 Pool 候选只消费布尔事实。
- stale/unknown 一律 fail-open，现有健康、重试和熔断机制继续承担请求失败兜底。

## Data Flow and Contracts

```text
status_snapshot.quota
  -> provider_pool_key_balance_below_minimum
  -> 普通候选 key_balance_below_minimum map
  -> CandidateRuntimeSelectionSnapshot
  -> CandidateRuntimeSelectabilityInput.balance_below_minimum
  -> key_balance_below_minimum skip reason

status_snapshot.quota
  -> ProviderPoolAdapter::member_signals
  -> PoolMemberSignals.balance_below_minimum
  -> schedule_pool_group
  -> pool_balance_below_minimum skip reason
```

新增 `provider_pool_key_balance_below_minimum(key, provider_type) -> bool`，复用 `provider_pool_member_quota_snapshot`、`provider_pool_json_f64` 和 `provider_pool_json_bool`。函数仅接受 `kind=balance`、`fresh`、非 unlimited、非空且全部有效的余额；任何未知输入返回 `false`。

Workspace 内部公共类型新增余额事实字段和两个诊断 reason；不改变数据库、序列化、HTTP 或前端合同。PoolGroup 外层不继承代表 Key 的余额事实，由展开后的真实 Key 判断。

## Scheduling and Sticky Behavior

- 普通候选在现有统一 selectability gate 中先过滤低余额事实。
- Pool scheduler 在账户阻断/quota 判断旁无条件过滤 `balance_below_minimum`，但订阅 exhaustion 仍受原开关控制。
- sticky 候选构造后以单元素集合复用 `schedule_pool_page_candidates()`，不得复制过滤逻辑。
- sticky 被跳过时保留 `seen_key_ids`、`record_skipped_candidates()`、最终 `pool_key_index` 分配和普通 page 回退。
- `pool_balance_below_minimum` 视为释放 scan budget 及 active-probe unschedulable，避免低余额 Key 阻塞后续候选。

## Refresh and Cache Invalidation

- `ProviderPoolService::quota_serving_policy(provider_type) == ObservationOnly` 的余额提供商自动进入 account self-check；其他 policy 仍遵循现有 enable 开关。
- 复用 `account_self_check_interval_minutes` 和并发配置，未配置时保持 60 分钟/4 并发。
- Key 只需 `is_active` 即可继续刷新，低余额不会停止探测；刷新失败保持 ObservationOnly 行为，不写 cooldown/hard-state。
- 写入前后分别计算余额事实。布尔 eligibility 变化时调用现有 `invalidate_provider_quota_candidate_caches()`；未变化时保持 catalog-only 失效。

## Compatibility and Rollback

- 行为变化只覆盖当前 registry 中产生余额快照的 ObservationOnly provider。
- 订阅 quota、provider-level quota alert、排序权重、重试和持久化 schema 不变。
- 回滚时可整体撤销余额事实字段、统一 gate、自动自检和缓存转换判断；没有数据迁移或外部合同需要回滚。

## Audit Findings Deferred

- 高并发计数截断与非原子 reservation 是独立 correctness 风险，必须在子任务中设计 scoped atomic admission 及所有数据后端的释放语义。
- 秒级 load-balance seed 会形成同秒热点；默认 failover 无限制可能放大错误流量；provider quota block map 串行读取是可选性能优化。

