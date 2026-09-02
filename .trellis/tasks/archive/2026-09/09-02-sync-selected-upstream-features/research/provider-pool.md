# Research: Provider Pool and Key scheduling

- Query: 研究上游 `9631b229b`、`2fe260002`、`ee55f4696`、`57abb2077`、`6c71f8758`，判断 `dd2958a45`/其他补丁是否为 R2 实际功能所需，并与 fork 基线 `50c96d060` 的本地调度、额度和 UI 行为对齐。
- Scope: mixed
- Date: 2026-09-02

## Findings

### 结论

R2 应以五个指定提交的最终行为为来源，但当前 fork 已在相同大文件上叠加余额调度、永久额度阻断、模型/Endpoint 精确绑定和流式错误修复，不能盲目整提交 cherry-pick。最小且完整的集成顺序是：

1. **部分移植** `dd2958a458a582d77803a8e57c9aa3f23672dd7b` 的 Provider Key 保存后即时回显片段；不要带入该提交的套餐结算和端点下拉框改动。
2. **移植 R2 子集** `9631b229b3ac07d5ecccb7990268717a72d086a2`：Key 原子并发 admission、缓存亲和二级模式、Pool 管理载荷，以及 Antigravity OAuth/额度刷新所需片段；不要带入钱包额度语义修复。
3. **移植** `2fe2600021df7a5a971d1e5d6ec94bcf852ff4aa` 的模型额度隔离和 Antigravity 汇总基础。
4. **顺序吸收最终 UI 状态** `ee55f46962c397fade75bd5d7150d2e788f8ca9d`、`57abb20778b19423debe35dde26a3447d5db09cb`、`6c71f87589d9e47124e77359054931afae08dd56`；本地 UI 差异大，按最终状态手工合并比逐个保留中间态更小。
5. **由 R3 集成者串联** `633363e190415943c37792946c4a63acefcf3408` 中仅与 Pool 并发饱和有关的补丁，使所有候选都饱和时保留 skip reason 并返回最终 HTTP 429。

不需要上游 merge wrapper `9b819169d5`、`9210502e77`、`d117a0cd13`、`b538aa2d66`，也不需要其后的纯格式化提交 `3d87bbf230`。通用 Usage API `4dbf98163e`、套餐权益撤销 `144a28f544`、Nightly `ef7caa40e7`/`611c29f1f5` 均无 R2 编译或运行依赖。

### 上游提交与必要子集

| 提交 | 上游职责 | R2 决策 | 依赖/后续 |
|---|---|---|---|
| `dd2958a45` | PR #756 同时修复 Key 保存回显、套餐结算、端点选择器 | **只取前端回显**：`KeyFormDialog.vue`、`OAuthKeyEditDialog.vue`、`ProviderDetailDrawer.vue`、`provider-key-concurrent_limit.spec.ts`。后端 settlement 和端点布局不取 | 可先于 963；不是 `concurrent_limit` 数据库持久化前置 |
| `9631b229b` | Key 并发、cache affinity 二级模式、Antigravity OAuth/Pool 额度展示 | **R2 核心来源，但按子集移植** | 2fe 的直接功能基线；执行层冲突需手工合并 |
| `2fe260002` | Antigravity 按请求模型、Codex Spark/标准额度隔离；汇总 UI 基础 | **取** | 应在 963 后；必须把 `provider_model_name` 接入本地 strong-read Pool context |
| `ee55f4696` | 恢复 Antigravity 汇总的进度条 | **取最终语义** | 依赖 2fe 的 summary utility |
| `57abb2077` | reset time 入载荷/窗口、百分号、倒计时 | **部分已存在，补缺** | 当前 admin parser 已保存 `reset_time`，只需补 catalog RFC3339/别名解析和最终 UI |
| `6c71f8758` | Pool、Provider Drawer、额度弹窗统一 Gemini / Claude & ChatGPT 汇总 | **取最终语义** | 依赖 2fe/ee/57；弹窗测试模型列表仍必须使用 raw items |
| `633363e190` | PR #772 的 Pool 饱和终态 + Gemini malformed-call 修复 | **仅取 R2/R3 交界的饱和子集** | 必须晚于 963；Gemini commit-policy 部分由 R3 处理 |

`9631b229b` 的 R2 文件子集：

- 原子并发：`crates/aether-runtime/state/src/lib.rs`、`apps/aether-gateway/src/provider_pool_demand.rs`、`execution_runtime/{sync,stream}/execution.rs`、`handlers/proxy/websocket/responses/admission.rs`。
- Pool 配置/载荷：`dispatch/pool_scheduler.rs`、`handlers/admin/provider/pool/config.rs`、`handlers/admin/provider/pool_admin/payloads.rs`、`crates/aether-provider/pool/src/presets.rs`、`frontend/src/api/endpoints/pool.ts`。
- 前端配置/额度：`PoolSchedulingDialog.vue` 及其新增测试、`PoolManagement.vue`、`poolQuotaRefresh.ts`、相关 Pool tests。只移植当前缺少的字段、刷新合并与显示逻辑，保留本地既有 Codex reset-credit 和状态展示。
- Antigravity 额度可持续刷新：`provider_key_auth.rs`、`maintenance/runtime/oauth_token_refresh.rs`、`crates/aether-oauth/.../{antigravity,generic}.rs`、`crates/aether-provider/transport/src/generic_oauth/mod.rs`，以及 `state/oauth.rs` 的持久化回归测试。它们让 legacy `refreshToken` 可识别、授权请求获得 offline refresh token，并验证刷新后的 access/refresh token 与 expiry 持久化。
- 明确排除：`control/auth/gate.rs`、`wallet_runtime/access.rs` 的 unlimited wallet 修复；它们不影响 Provider Key 并发/Pool 调度，属于独立计费语义。

`633363e190` 只需抽取这些容量片段：

- `execution_runtime/{sync,stream}/execution.rs`：把 `provider_key_concurrency_limit_reached` 同步写入本地 runtime-miss 诊断。
- `request_candidate_runtime.rs`：Skipped candidate 的 `skip_reason` 保留该容量原因；合并时扩展本地映射，不能覆盖已存在的永久额度原因。
- `dispatch/pool_scheduler.rs`：Pool 耗尽时记录所有实际 skip reasons，而非只写一个聚合占位原因。
- `handlers/proxy/mod.rs`：仅当所有相关 skip reason 都是容量类时，终态为 429/“上游账号并发已达上限”。不要带入同提交的 Gemini stream commit-policy、format matrix 或 request-body-build 逻辑。

### `concurrent_limit`：持久化与所有 admission 路径

当前 fork **已经完成存储和普通管理 API 往返**，无需 `dd2958a45` 的 Rust settlement 代码，也无需新增 migration：

- 前端表单已读取并序列化 `concurrent_limit`（`frontend/src/features/providers/components/KeyFormDialog.vue:239`、`:717`、`:985`；OAuth 表单 `OAuthKeyEditDialog.vue:78`、`:264`、`:383`）。
- Gateway create/update DTO 与归一化已存在（`apps/aether-gateway/src/handlers/admin/provider/shared/payloads.rs:28`、`:76`；`write/keys/create.rs:193`；`write/keys/update.rs:305`）。
- 公共 Key contract 已有 nullable 字段（`crates/aether-data/contracts/src/repository/provider_catalog/types.rs:455`）。
- SQLite/MySQL/PostgreSQL provider catalog 已在 SELECT/INSERT/UPDATE/row decode 中包含字段，例如 `sqlite/src/provider_catalog.rs:129`、`mysql/src/provider_catalog.rs:68`、`postgres/src/provider_catalog.rs:173`；逻辑 schema 也是 nullable 且无默认值（`crates/aether-data/runtime/schema/logical/002_provider_catalog.toml:388`）。

当前缺口分三层：

1. Pool admin key payload 没有 `concurrent_limit`，但 Pool 前端已试图读取它（`frontend/src/api/endpoints/pool.ts:403`）；963 在 `build_admin_pool_key_payload` 补字段。
2. 调度预过滤虽会从近期 candidate 记录判断 Key 并发（`crates/aether-scheduler-core/src/candidate/selectability.rs:94-108`），但这是非原子的快照，且候选读取量有限，不能阻止两个同时到达的请求都通过。
3. 当前执行 guard 只按 provider 计数（`apps/aether-gateway/src/provider_pool_demand.rs:286-368`），同步、流式、WS 都调用该 provider-only guard（`execution_runtime/sync/execution.rs:2166`、`stream/execution.rs:4057`、`handlers/proxy/websocket/responses/admission.rs:44`）。

963 的正确补法是在 `RuntimeState` 增加以 `provider_key:{key_id}` 分区的 keyed semaphore，在执行前从 catalog 读取正数 `concurrent_limit` 并原子 `try_acquire`；`None`/`0` 仍是不限制。permit 与原 provider-demand guard 放进同一 RAII guard，正常完成、显式 release 和 Drop 都释放。

执行覆盖关系如下：

- 普通同步 HTTP 和所有最终走 sync runtime 的特殊执行在 `execute_execution_runtime_sync_impl` admission。
- SSE/流式 HTTP 在 `execute_execution_runtime_stream_inner` admission，饱和时写 Skipped candidate、设 `AiAttemptRetryScope::Candidate` 并让调用方尝试下一候选。
- `ResponsesWebSocketTurnAdmission` 是共享的 per-turn admission；上游 963 的调用方包括 Responses WS、OpenAI Live WS 和 Realtime WS（`9631b229b:handlers/proxy/websocket/{responses/turn.rs:388,live/session.rs:1127,live/http.rs:365,realtime/session.rs:135}`）。连接空闲时不占 permit，每个 `response.create`/turn 单独占用。
- 调度阶段的近期记录过滤必须保留，作为减少无效候选和诊断的快速路径；执行前 semaphore 是解决竞态的唯一 correctness gate，不能二选一。

运行时语义继承 `RuntimeState` backend：memory backend 只在当前进程原子；Redis runtime 才是多实例共享 admission。若部署允许多 Gateway 且不用 Redis，不能声称全局并发上限。

### Cache affinity 语义

当前已有四种数据库排序，勿新增表或另一套亲和实现：`StoredPoolKeyCandidateOrder` 在 `crates/aether-data/contracts/src/repository/candidate_selection/types.rs:50-58`；PostgreSQL 的准确顺序见 `crates/aether-data/adapters/postgres/src/candidate_selection.rs:689-704`，SQLite/MySQL 有等价 NULL 排序。

- sticky 命中：仍先复用当前用户绑定 Key，但必须经过共享 Pool hard filters。
- 首次分配/未命中：`cache_affinity.mode = single_account`（默认）映射 `SingleAccount`，即先 `internal_priority ASC`，同优先级再最近使用优先；`mode = lru` 映射 `Lru`，即最久未用/NULL-first 轮号。
- 缺失或非法 mode 在 config parser 归一为 `single_account`；独立 `single_account` 分配模式仍保留，不能和 cache-affinity sticky 语义混为一个开关。
- mode 存在 provider config JSON 的 scheduling presets 内，无 DB migration。后端 preset payload 必须暴露两项和默认值，前端 `PoolSchedulingDialog` 只在 cache affinity 选中时显示二级选项。

本地必须继续遵守余额 spec：sticky singleton 也走共享 Pool scheduler（`.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md:50`），不能恢复绕过 filter 的捷径。

### 不同模型额度隔离

当前 `ProviderPoolMemberInput` 只有 provider/key/auth config（`crates/aether-provider/pool/src/provider.rs:18-21`），Pool catalog context 调 `member_signals` 时没有请求模型（`apps/aether-gateway/src/dispatch/pool_scheduler.rs:1555-1579`），所以模型窗口只能被折叠成 Key 级结论。

2fe 的最小改动：

- 给 `ProviderPoolMemberInput`/`ProviderPoolService::member_signals` 增加 `provider_model_name: Option<&str>`。
- 从 PoolGroup 的 `candidate.selected_provider_model_name` 一直传到 strong-read 后构造的真实 Key context。
- Antigravity 只匹配 `windows[].model == requested provider model`；该窗口耗尽只跳过同模型，不影响另一个模型。
- Codex 按模型名是否包含 `spark`，分别只看 `spark_*` 或非 `spark_*` 窗口；标准额度和 Spark 额度互不污染。
- 没有匹配的模型窗口时回退原来的 account snapshot 语义，避免未知模型被误放行。

合并时必须保留三类本地 Key-wide 事实：

- fresh balance 下限仍独立、自动跳过/自动恢复（`provider.rs:76-80`；`quota.rs:29-71`）。
- `status_snapshot.scheduling` 的管理员恢复型永久阻断仍无条件 Key-wide，不能因 requested model 而降级（`quota.rs:81-102`；`pool_scheduler.rs:1580-1583`）。
- catalog 读取仍用 `list_provider_catalog_keys_by_ids_strong`，失败/缺项仍以 `pool_key_state_unavailable` fail closed（`pool_scheduler.rs:1495-1537`）；不要采用上游旧基线的普通缓存读。

本地 `PoolMemberSignals` 已额外拥有 `balance_below_minimum`、`quota_hard_blocked`、`runtime_quota_hard_blocked`、`catalog_state_unavailable`（`crates/aether-pool-core/src/scheduler.rs:39-52`），2fe 只能增量传入 model，不能用上游结构整体覆盖。

### Antigravity 载荷与三处 UI 的最终状态

目标数据流：

`fetchAvailableModels.models[*].quotaInfo` → `parse_antigravity_usage_response` → `upstream_metadata.antigravity.quota_by_model` → catalog `status_snapshot.quota.windows[]` → 共享 `summarizeAntigravityQuotaItems` → Pool / Provider Drawer / Quota Dialog。

当前 fork 已把 `quotaInfo.resetTime` 保存为 `reset_time`（`crates/aether-admin/src/provider/quota.rs:235-275`），所以 57 的 admin parser hunk已经等价存在。当前缺的是 catalog timestamp parser：它只接受数字/数字字符串（`apps/aether-gateway/src/handlers/shared/catalog.rs:443-455`），应吸收 57 的 RFC3339 解析，并让 model window 查 `reset_at`、`next_reset_at`、`reset_time`、`next_reset_time`（`model_quota_window_snapshot` 当前入口 `catalog.rs:525`）。

最终显示合同不是 2fe 的“无进度条数字网格”中间态，而是 ee/57/6c 合并后的状态：

- 只汇总两组：`Gemini额度` 与 `Claude & ChatGPT`（Claude 和 `gpt-*` 同组）；tab/chat/opaque reset-credit buckets 不进入摘要。
- 每组进度条使用组内最小 remaining percent；文字为相同值 `N%`，不同值 `min%–max%`；reset countdown 跟随最小 remaining 的那个实际窗口。
- Pool Management、Provider Detail Drawer 和 AntigravityQuotaDialog 都复用同一 summary utility，避免三套家族判断。
- Quota Dialog 的下拉“测试模型”仍遍历 raw model items；只能汇总可视额度卡，不能让汇总后的两行取代实际模型 ID。
- `ProviderQuotaProgressRow` 支持显式 `meterText`，因此 Drawer 与 Dialog 能显示范围而不伪造单一百分比。

### Files found

| 路径 | 一句话说明 |
|---|---|
| `.trellis/tasks/09-02-sync-selected-upstream-features/prd.md` | R2/AC2 的权威范围：并发、亲和、模型隔离、Antigravity 全链路（`:23-28`、`:63`）。 |
| `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md` | 本地余额 fail-open、sticky 共享过滤、自检恢复、cache invalidation 合同（`:37-59`）。 |
| `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md` | 永久额度阻断 strong read、HTTP/SSE/WS、管理员恢复合同（`:62-109`）。 |
| `apps/aether-gateway/src/provider_pool_demand.rs` | 当前 provider-demand guard；963 在此组合 provider token 与 Key semaphore。 |
| `crates/aether-runtime/state/src/lib.rs` | 现有 RuntimeSemaphore；963 增加 resource-key namespace 和显式 release。 |
| `apps/aether-gateway/src/execution_runtime/{sync,stream}/execution.rs` | 两条 HTTP candidate 执行入口，存在大量 fork-only 额度/流错误改动。 |
| `apps/aether-gateway/src/handlers/proxy/websocket/responses/admission.rs` | Responses/Live/Realtime 共用 per-turn admission。 |
| `apps/aether-gateway/src/dispatch/pool_scheduler.rs` | Pool 展开、strong read、sticky、余额/永久额度过滤与模型名传递的主要冲突点。 |
| `crates/aether-provider/pool/src/{provider,quota,service,providers/{antigravity,codex}}.rs` | Provider model quota policy 与本地 balance/runtime block 的共享边界。 |
| `crates/aether-provider/pool/src/presets.rs` | 分配模式元数据；963 在 cache affinity 下嵌入首次分配模式。 |
| `crates/aether-data/{contracts,adapters}/...provider_catalog...` | 已有 `concurrent_limit` 四后端持久化，不是本轮新增面。 |
| `crates/aether-data/*/candidate_selection.rs` | 已有 LRU/CacheAffinity/SingleAccount SQL 排序，应直接复用。 |
| `apps/aether-gateway/src/handlers/shared/catalog.rs` | upstream metadata 到统一 quota window；当前缺 Antigravity RFC3339 reset 归一。 |
| `crates/aether-admin/src/provider/quota.rs` | 当前已保存 Antigravity `reset_time`，57 的这部分无需重复。 |
| `frontend/src/features/providers/utils/antigravityQuota.ts` | 现有标签/排序/去重；2fe/57/6c 在此增加唯一 summary owner。 |
| `frontend/src/views/admin/PoolManagement.vue` | Pool 并发字段、quota refresh、summary 的高冲突 UI；保留本地状态/恢复功能。 |
| `frontend/src/features/providers/components/{ProviderDetailDrawer,AntigravityQuotaDialog,ProviderQuotaProgressRow}.vue` | Provider 页面摘要、原始模型测试与范围 meter 三个消费者。 |

### 风险最高的重叠文件/符号

| 文件/符号 | 本地行为必须保留 | 集成动作 |
|---|---|---|
| `execution_runtime/stream/execution.rs` | Responses 流错误预提交、永久额度证据、Gemini/Anthropic 本地修复 | 手工放置 Key admission；再串联 633 的 capacity diagnostic，不能整文件取上游 |
| `execution_runtime/sync/execution.rs` | 本地 quota evidence 和模型/Endpoint 精确绑定 | 同上；饱和发生在 Pending/usage 写入前 |
| `dispatch/pool_scheduler.rs` | `3d277c56d` balance、`81c841be4` strong-read/runtime block、`60377958c` 精确模型/Endpoint | 只增 mode 映射与 model 参数；保留所有 skip reason、seen set、scan budget、active-probe eviction、`pool_key_index` |
| `provider-pool/{provider,quota}.rs` | balance helper、hard block wrapper | 扩展 input；不得删除本地字段或把永久 block 变为 model-scoped |
| `request_candidate_runtime.rs` + `handlers/proxy/mod.rs` | 本地永久额度和其他 skip reasons | 合并 633 时做“添加容量原因”，不能把 mapping 收窄成只有 concurrency |
| `PoolManagement.vue` | 本地额度回退、Codex reset credits、永久额度恢复 UI | 按功能块移植，避免上游大文件覆盖 |
| `ProviderDetailDrawer.vue` | 本地分页、选择同步、额度/恢复、模型精确绑定 | 先合 dd 的 API-return snapshot，再合 6c summary；保存回调参数保持 optional 以兼容其他 emitters |
| `catalog.rs` | 多 provider quota window 和永久状态投影 | 仅增强 timestamp/model reset aliases，不替换整个 builder |

### 最小验证

先跑每组最小检查；只有这些暴露公共契约问题时再扩大：

```powershell
# Key 原子 admission、三类执行路径和最终容量终态
cargo test -p aether-runtime-state memory_keyed_semaphores_isolate_resource_capacity
cargo test -p aether-gateway provider_key_limit_rejects_concurrent_guard_until_release
cargo test -p aether-gateway provider_key_concurrency
cargo test -p aether-gateway provider_key_capacity
cargo test -p aether-gateway saturated_provider_key_snapshot_persists_capacity_skip_reason

# Cache affinity + requested-model quota isolation
cargo test -p aether-provider-pool cache_affinity_preset_exposes_secondary_distribution_modes
cargo test -p aether-gateway cache_affinity_secondary_modes_select_distinct_candidate_orders
cargo test -p aether-provider-pool antigravity_model_quota_exhaustion_does_not_block_other_models
cargo test -p aether-provider-pool codex_standard_and_spark_quota_families_are_independent
cargo test -p aether-gateway pool_catalog_context_scopes_antigravity_exhaustion_to_requested_model

# Antigravity refresh + reset payload
cargo test -p aether-oauth antigravity_authorize_requests_offline_refresh_token
cargo test -p aether-provider-transport antigravity_expired_legacy_credential_refreshes_and_normalizes_refresh_token
cargo test -p aether-gateway legacy_antigravity_refresh_token_is_refreshable
cargo test -p aether-admin parses_antigravity_usage_response_labels_opaque_reset_credit_keys
cargo test -p aether-gateway sync_provider_key_quota_status_snapshot_labels_antigravity_models_by_model_id

# 前端；在 frontend 目录执行
npm run test:run -- src/features/providers/components/__tests__/provider-key-concurrent_limit.spec.ts src/features/pool/components/__tests__/PoolSchedulingDialog.cache-affinity.spec.ts src/features/providers/utils/__tests__/antigravityQuotaSummary.spec.ts src/features/pool/components/__tests__/PoolKeyDisplayPanels.spec.ts src/features/providers/components/__tests__/AntigravityQuotaDialog.spec.ts
npm run type-check

# 收口
cargo fmt --all --check
```

Gateway 测试在本机可能触发 `boring-sys2`/NASM 构建；既有可行环境是 NASM 在 `C:\Users\Zipper\AppData\Local\bin\NASM`、设置 VS2026 的 `CMAKE`，但不要设置 `CMAKE_GENERATOR`。这是环境前置，不是本轮产品依赖。

### Integration requests

- R3 owner：在 963 Key semaphore 落地后，负责 `633363e190` 的 capacity-only 片段；若 R3 已整体移植 633，则 R2 不重复修改这些文件。
- R4/Antigravity owner：`57abb2077` 的 catalog RFC3339 reset parser 由唯一 owner 合并；R2 UI 只消费统一 `reset_at/reset_seconds`。
- Integrator：最终检查 `git diff` 不包含 `dd2958a45` 的 settlement/endpoint-layout、963 的 wallet 修复、633 的 Gemini commit-policy（若不属于 R3 分配）、以及任何 Usage API/套餐撤销/Nightly 文件。
- Integrator：对 `PoolManagement.vue`、`ProviderDetailDrawer.vue`、`stream/execution.rs`、`sync/execution.rs` 指定唯一写者，避免共享大文件并行覆盖。

## External references

- [9631b229b commit](https://github.com/fawney19/Aether/commit/9631b229b3ac07d5ecccb7990268717a72d086a2) / [PR #767](https://github.com/fawney19/Aether/pull/767) — Key concurrency、quota refresh、cache affinity 的上游来源。
- [2fe260002 commit](https://github.com/fawney19/Aether/commit/2fe2600021df7a5a971d1e5d6ec94bcf852ff4aa) / [PR #768](https://github.com/fawney19/Aether/pull/768) — model-scoped quota 与 compact summary。
- [ee55f4696 commit](https://github.com/fawney19/Aether/commit/ee55f46962c397fade75bd5d7150d2e788f8ca9d) — 恢复 summary progress bars。
- [57abb2077 commit](https://github.com/fawney19/Aether/commit/57abb20778b19423debe35dde26a3447d5db09cb) / [PR #770](https://github.com/fawney19/Aether/pull/770) — reset time normalization/countdown。
- [6c71f8758 commit](https://github.com/fawney19/Aether/commit/6c71f87589d9e47124e77359054931afae08dd56) — 三个前端 surface 最终 summary 对齐。
- [dd2958a45 commit](https://github.com/fawney19/Aether/commit/dd2958a458a582d77803a8e57c9aa3f23672dd7b) / [PR #756](https://github.com/fawney19/Aether/pull/756) — 仅采用 Key save-result snapshot 小片段。
- [633363e190 commit](https://github.com/fawney19/Aether/commit/633363e190415943c37792946c4a63acefcf3408) / [PR #772](https://github.com/fawney19/Aether/pull/772) — capacity saturation 终态与 Gemini 修复的混合提交。

## Related specs

- `.trellis/spec/aether-provider-pool/backend/balance-scheduling-contract.md`
- `.trellis/spec/aether-provider-pool/backend/runtime-quota-block-contract.md`
- `.trellis/spec/aether-provider-pool/backend/index.md`
- `.trellis/spec/aether-scheduler-core/backend/index.md`
- `.trellis/spec/aether-data-contracts/backend/index.md`
- `.trellis/spec/aether-gateway-execution/backend/index.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `.codegraph/` 不存在，调查使用源码、当前 refs 和指定上游提交/PR；未建立索引。
- 未执行实现、cherry-pick 或测试；只运行了 read-only diff/ancestry/patch-apply checks。`git apply --check` 已确认 963 的 sync/stream、2fe 的 Pool/provider quota、以及所有 Antigravity UI 中间提交在当前 HEAD 上存在冲突，结论因此要求手工合并最终行为。
- `dd2958a45` 不是 Key 数据库存储前置；若只看最终刷新后的 UI，可以不移植其前端片段，但 PRD 要求设置/载荷/UI 一致时，保留 API 返回快照是最小且确定的即时一致性修复。
- 内存 runtime 的 Key semaphore 不跨进程；多实例的严格上限依赖共享 Redis runtime。
- 上游提交不可变，但当前 fork 工作树后续若先合入 R1/R3/R4，行号和冲突集合会漂移；应复核最终 staged diff，不应复做功能设计。
