# Technical Design

## State and Persistence

- 在 `status_snapshot.scheduling` 保存可选状态：`quota_suspected` 或 `quota_exhausted`。缺失或 `null` 表示无额度阻断。
- 状态字段包含 `blocked`、`source`、`confidence`、`confirmation_count`、`status_code`、`error_code`、`reason`、`first_observed_at` 和 `last_observed_at`。
- 增加只拥有 `scheduling` 顶层字段的 Key 调度状态 CAS；CAS 同时比较当前调度状态和请求规划时捕获的非敏感凭据指纹。写入前强读当前 Key，并以实际加密凭据字段作为数据库条件，覆盖 memory/SQLite/MySQL/PostgreSQL。
- 原始响应体、请求头和凭据不得落库；原因截断为短文本。凭据指纹仅用于内部 fencing，不进入公开响应或日志。

## Detection and Retry

- 在 `LocalFailoverPolicy` 增加 `quota_exhaustion_patterns`，复用现有 `LocalFailoverRegexRule` 的正则与状态码约束。
- 独立提取额度证据，再与现有 stop/cyber/error-stop 分类组合：显式 stop 仍决定当前请求是否停止，但额度证据仍可持久化。
- 强信号：HTTP 402、精确规范化错误码、人工规则；弱信号：窄范围余额不足文本。普通限流码和明确重置期限优先保留现有暂态路径，人工规则除外。
- 未被显式 stop 的额度信号映射为 `FailureScope::Credential` / `AiAttemptRetryScope::Credential`，并保留最终上游错误作为候选耗尽后的回退响应。

## Effects and Scheduling

- 增加共享额度调度状态 effect，供同步、流式及 HTTP 200 错误包路径调用；强信号直接阻断，弱信号执行 CAS 状态机，非额度 HTTP 终态只清除疑似状态。
- 额度信号仅执行候选失败/亲和失效、调度状态持久化、Pool 硬状态反馈和 active-probe 移除；跳过健康、熔断、自适应限流和 OAuth 失效 effect。
- `aether-provider-pool` 暴露统一的 Key 调度额度阻断读取函数；普通 scheduler 与 Pool scheduler 分别产生 `key_quota_exhausted` 和 `pool_key_quota_exhausted`。
- Pool 分数构建把持久阻断并入 `quota_exhausted` 输入，确保周期重建保持 `QuotaExhausted`。确认/恢复后清理候选页、resolved page 和 scheduler affinity 缓存。

## Admin and Frontend

- 新增 `POST /api/admin/endpoints/keys/{key_id}/clear-quota-exhausted`，返回 `{ key_id, cleared, message }`；无状态时幂等返回 `cleared=false`。
- 恢复接口把 `status_snapshot.scheduling` 设为 `null`，按当前 Key 事实重建对应 Pool score，并刷新调度缓存；不修改 `is_active` 或其他运行态。
- 扩展 `FailoverRulesDialog` 管理 `quota_exhaustion_patterns`，表单和 JSON 模式均复用 `FailoverRuleItem`。
- 扩展状态快照 TypeScript 类型；Provider 详情与 Pool 管理显示疑似警告、确认阻断、原因和时间，并提供独立“恢复调度”操作。

## Compatibility

- 不需要数据库迁移；旧快照缺少 `scheduling` 时保持现有行为。
- 已有 Provider 专用、带 reset deadline 的额度状态继续由原适配器处理，不转为人工阻断。
- 运行时额度阻断不接入自动删除配置；凭据替换保留已有阻断，直到管理员显式恢复。
