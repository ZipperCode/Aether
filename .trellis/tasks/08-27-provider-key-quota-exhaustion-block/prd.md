# 提供商 Key 额度耗尽智能阻断与人工恢复

## Goal

当上游中转站明确返回 Key 或账户额度耗尽时，立即停止在当前请求中继续使用同一 Key，并持久阻断后续调度，直到管理员显式恢复；同时避免把普通限流、可自动重置的订阅窗口或传输故障误判为永久额度耗尽。

## Background

- Aether 已有候选级故障转移、Credential/Endpoint/Provider 重试范围、Key 健康与熔断、Pool 硬状态、额度快照和余额感知过滤。
- 当前 New API、One API、Sub2API 等中转站的运行时额度错误只会继续参与候选重试并逐步进入健康熔断，未统一形成 Key 级持久阻断。
- 已确认采用独立阻断状态，不修改 `is_active`；已确认精确错误码和人工规则一次阻断，内置模糊文本连续两次才阻断。

## Requirements

- 识别 HTTP 402、`insufficient_user_quota`、`insufficient_quota`、`insufficient_balance`、`api_key_quota_exhausted` 等强信号；管理员可在现有故障转移配置中增加 `quota_exhaustion_patterns`。
- 内置弱文本第一次记录疑似状态，同一凭据连续第二次确认；任意非额度 HTTP 终态清除疑似状态，传输错误不改变状态。
- 普通 429、`rate_limit_exceeded`、`usage_limit_exceeded`、普通 `resource_exhausted` 以及带明确重置期限的限流继续使用现有冷却/自动恢复语义。
- 额度信号将当前请求的重试范围提升为 Credential，跳过同一 Key 的其余模型、Endpoint 和协议候选；显式错误终止规则仍决定是否停止当前请求。
- 持久状态写入 `status_snapshot.scheduling`，至少包含状态码、阻断标记、来源、置信度、确认次数、HTTP/上游错误码、脱敏原因和观测时间。
- 运行时状态写入必须凭据隔离并支持多实例 CAS，旧凭据的迟到响应不得阻断替换后的凭据。
- 普通候选、Pool 真实 Key、sticky、active-probe 和 Pool 分数重建消费同一阻断事实，并输出独立 skip reason。
- 已识别的额度错误不得累计健康失败、打开熔断器、训练自适应 429 或误写 OAuth 失效；Pool 可投影为现有 `QuotaExhausted` 硬状态，但不得自动删除 Key。
- Provider Key 列表与 Pool 管理页展示疑似/已阻断状态、原因和时间；已阻断状态提供带确认的人工恢复操作。
- 新增幂等恢复接口，仅清除额度阻断并刷新调度/Pool 状态，不改变 `is_active`，也不清除 OAuth、冷却和其他健康状态。
- 已确认阻断不得因成功请求、额度刷新、服务重启或凭据编辑自动恢复；额度仍不足时，人工恢复后的下一次请求可以再次阻断。

## Out of Scope

- 不修改现有余额下限、订阅窗口自动恢复、Codex 专用额度恢复和 `auto_remove_quota_exhausted_keys` 语义。
- 不新增数据库列或迁移，不自动禁用、删除或启用 Key。
- 不新增单元测试，不运行 `aether-gateway` 大模块编译，不提交代码。

## Acceptance Criteria

- [ ] New API/One API 403 与 Sub2API 403/429 的精确额度错误一次命中即持久阻断，并在当前请求切换其他 Key。
- [ ] 模糊文本第一次仅疑似，连续第二次阻断；中间非额度 HTTP 终态会重置计数。
- [ ] 普通 429、窗口限额和带重置期限的错误只冷却，不进入人工恢复状态。
- [ ] 人工正则可按可选状态码匹配非标准或 HTTP 200 错误包，一次命中即确认。
- [ ] 普通候选、Pool、sticky、缓存命中和重启后均跳过阻断 Key，Pool 分数重建不会误恢复。
- [ ] 额度错误不污染健康、熔断、自适应限流和 OAuth 状态。
- [ ] 并发写入不丢失确认计数，旧凭据迟到响应不能污染新凭据。
- [ ] 两个管理界面可看到阻断证据并人工恢复；恢复不启用人工禁用的 Key，也不清除其他阻断。
- [ ] 轻量格式、相关小 crate 检查、前端类型检查和 `git diff --check` 通过。

## Notes

- 运行时持久阻断优先于正余额快照；只有专用人工操作可以清除已确认状态。
- 管理员自定义额度规则视为强信号；内置模糊文本才使用二次确认。
