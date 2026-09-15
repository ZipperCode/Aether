# Research: provider health default gray

- Query: 为什么提供商中新添加、尚未使用的健康度进度条默认置灰，而旧版本显示绿色满格？
- Scope: internal
- Date: 2026-09-11

## Findings

- 行为变更来自提交 `d1b5eb08e`（2026-09-06，`fix(providers): correct endpoint health aggregation and display`），不是 CSS 单独回归。
- 变更前 `apps/aether-gateway/src/handlers/admin/provider/summary/value.rs` 在端点没有关联 key 时把 `health_score` 设为 `1.0`；有 key 但尚无该格式健康记录时也通过 `unwrap_or(1.0)` 计作满分。
- 当前摘要仅对启用端点下、启用 key 的 `provider_key_health_score(key, api_format)` 有效值求平均（`apps/aether-gateway/src/handlers/admin/provider/summary/value.rs:87-120`）。无有效值时返回 JSON `null`；`avg_health_score` 也仅在存在有效端点分数时返回数值。
- 新建密钥初始化 `health_by_format` 为空对象（`apps/aether-gateway/src/handlers/admin/provider/write/keys/create.rs:200-214`），而 `crates/aether-scheduler-core/src/health.rs:241-250` 的 `provider_key_health_score` 对缺少 `health_score` 返回 `None`。因此首次请求/探测前的摘要必然是 `null`。
- 前端 `frontend/src/features/providers/composables/useEndpointStatus.ts:35-70` 把不可用端点、缺失/无效分统一转为 `null`，颜色 `bg-muted-foreground/40`（灰色），标签 `-`，宽度 `100%`；`ProviderTableRow.vue:102-130` 与 `ProviderCard.vue:144-168` 直接使用这些函数。
- 提交同时新增 `docs/api/provider-health-summary.md:10-16,29-37`，明确无健康数据显示灰色占位条和 `-`，不可用端点不再显示 `100%`。测试 `frontend/src/features/providers/components/__tests__/provider-endpoint-health.spec.ts` 与 `frontend/src/features/providers/composables/__tests__/useEndpointStatus.spec.ts` 固化了该契约（`null -> 100% + bg-muted-foreground/40`）。

## Caveats / Not Found

- 当前仓库文档把灰色行为定义为有意语义：未知/未观测健康不等于成功。若恢复旧绿色满格，需要明确修改该契约并同步后端聚合、前端状态及上述测试/文档。
- 不能仅把所有缺失分数无条件填 `1.0`：`docs/api/provider-health-summary.md:20-27` 警告摘要脱敏投影读取失败会造成空 key 列表，简单填满分会掩盖真实数据问题。应区分“端点/密钥确实未使用”与“摘要读取失败”。
- 未发现其他 CSS 或主题规则导致该颜色；颜色来源是 `getHealthScoreColor` 的缺失分支。
