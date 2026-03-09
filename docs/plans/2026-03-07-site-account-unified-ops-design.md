# Site Account Unified Ops Design (Option B)

## 1. 结论

采用方案B：新增独立 `SiteAccount` 域模型，站点管理与 Provider 共享同一套执行引擎（connector/action），不再依赖“进入 Provider 页面”触发签到。  
目标是同时支持：
- 站点管理定时签到
- 站点管理定时余额同步
- WebDAV 缓存 + 可配置同步策略
- 未匹配 Provider 的站点可独立执行签到/余额

## 2. 现状问题

- Provider 页会触发批量余额查询，部分架构把签到耦合在余额查询里，造成“访问页面像是在触发签到”。
- 站点管理账号列表每次都实时拉 WebDAV，缺乏缓存、差量策略和回放能力。
- 未匹配 Provider 的站点只记录 `unmatched_provider`，无法签到与余额同步。
- 签到执行逻辑分散在 `ProviderOpsService` 与 `MaintenanceScheduler`，存在分支重复。

## 3. 目标与边界

### 3.1 目标

- 构建“目标无关”的统一执行器：Provider 与 SiteAccount 共用。
- SiteAccount 独立保存凭据与执行状态，不污染 Provider 主数据。
- 提供同步策略：`manual` / `interval` / `cron` / `on_startup`。
- 支持 WebDAV 快照缓存（hash/etag/last-modified）与强制刷新。

### 3.2 非目标

- 本期不改动请求路由主链路（推理请求仍由 Provider/Endpoint 体系处理）。
- 本期不把 SiteAccount 自动提升为 Provider。

## 4. 核心设计

### 4.1 数据模型

新增：
- `site_accounts`：站点账号主表（domain/base_url/architecture/auth_type/credentials/provider_id 可空/checkin_enabled/balance_sync_enabled/last status）。
- `site_source_snapshots`：WebDAV 拉取快照（raw payload/hash/etag/last_modified/source_url/fetched_at）。
- `site_account_balance_runs` / `site_account_balance_items`：站点余额同步运行日志（可与现有 checkin/sync logs 风格一致）。

复用：
- `site_sync_runs/site_sync_items`
- `site_checkin_runs/site_checkin_items`

### 4.2 统一执行引擎

抽出 `OpsExecutionService`（或等价命名）：
- 输入：`ExecutionTarget`（ProviderTarget / SiteAccountTarget）
- 逻辑：解析 ops config -> 创建 connector -> 执行 action -> 标准化 `ActionResult`
- 输出：统一 status/message/data，供调度器与 API 共用

Provider 兼容层：
- `ProviderOpsService` 保留 API，不直接处理底层分支，改为调用统一执行引擎。

SiteAccount 新能力：
- `SiteAccountOpsService` 调统一执行引擎执行 `checkin/query_balance`。

### 4.3 同步与缓存策略

同步分两阶段：
1. `fetch_snapshot`：拉 WebDAV，写入快照缓存（支持条件请求）。
2. `apply_snapshot`：按策略落地到 `site_accounts` 与（可选）Provider 凭据。

策略键（系统配置）：
- `site_sync_mode`: `manual|interval|cron|on_startup`
- `site_sync_interval_minutes`
- `site_sync_cron`
- `site_sync_apply_policy`: `matched_only|matched_and_unmatched`
- `site_sync_use_cache_ttl_seconds`
- `site_sync_force_refresh_on_manual`

### 4.4 调度统一

维护调度器新增或重构三个任务：
- `site_snapshot_sync_job`
- `site_account_checkin_job`
- `site_account_balance_sync_job`

Provider 原任务保留但可配置；后续可切换为同一调度编排器（按 target_type 分组执行）。

### 4.5 前端行为调整

- Site Management 默认读取后端缓存账号列表，提供“强制刷新 WebDAV”按钮。
- 新增“同步策略”配置区。
- 未匹配站点支持配置架构/凭据并直接手动签到、查询余额。
- Provider 页不再隐式触发签到（保留余额刷新能力，签到改为显式或定时）。

## 5. 风险与缓解

- 风险：新增 SiteAccount 凭据存储导致安全面扩大。  
  缓解：沿用 `provider_ops` 敏感字段加密/脱敏逻辑，统一凭据处理器。

- 风险：双体系并行期行为不一致。  
  缓解：统一执行引擎 + 契约测试（同架构同凭据时结果一致）。

- 风险：调度任务增多影响连接池。  
  缓解：分任务并发阈值、全局信号量、失败退避。

## 6. 验收标准

- 不访问 Provider 页面也能按计划执行站点签到与余额同步。
- 未匹配 Provider 的站点可在站点管理完成签到与余额查询。
- WebDAV 拉取频率受策略控制，手动可强制刷新。
- Provider 与 SiteAccount 对同架构行为一致（status/message/checkin_success 语义一致）。
