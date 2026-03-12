# WebDAV 独立签到与旧配置移除 设计文档

## 背景
当前站点管理模块已支持多 WebDAV 源，但签到调度仍然使用全局配置（`site_account_checkin_time`），无法按 WebDAV 源独立设置执行时间；同时签到日志缺少按源维度的关联字段，前端难以按源过滤查看定时任务执行结果。

另外，系统设置中仍保留旧的 all-api-hub WebDAV 配置（`all_api_hub_*`），与当前多 WebDAV 架构重复且无实际使用价值。

## 目标
1. 支持每个 WebDAV 源独立配置签到开关与签到时间。
2. 支持 WebDAV 定时签到日志按源查看与筛选，并展示账号维度明细。
3. 前后端彻底移除旧 all-api-hub WebDAV 配置逻辑，不保留兼容入口。

## 非目标
1. 不重构 Provider 签到（`enable_provider_checkin`）任务。
2. 不重做站点管理日志体系为统一任务中心。
3. 不删除既有 `site_checkin_items` 的 provider 维度字段（保留兼容）。

## 方案选择
采用方案 B（结构化完善版）：
- 数据层补齐 source/account 维度字段。
- 调度层按 WebDAV 源独立注册签到任务。
- API 与前端改为按源配置和按源查询。
- 系统设置全量删除旧 all-api-hub WebDAV 配置链路。

## 架构设计

### 数据职责
- `webdav_sources`：存储源级签到调度配置（`checkin_enabled`、`checkin_time`）。
- `site_checkin_runs`：存储一次签到运行，新增 `webdav_source_id` 用于源级过滤。
- `site_checkin_items`：存储运行明细，新增账号维度字段（`account_id`、`account_domain`、`account_site_url`）。

### 调度职责
- `SiteManagementScheduler` 启动时读取可用 WebDAV 源，为每个源注册独立 cron job。
- job id 规范：`site_account_checkin:<source_id>`。
- WebDAV 源配置发生变更（开关/时间）时，动态移除并重建对应 job。

### 日志职责
- 定时签到执行完成后，写入 source 维度 run。
- run 下写入 account 维度 item（成功/失败/跳过与消息）。
- 保留 provider 字段，兼容已有 `provider_ops` 手动签到写入路径。

## 数据模型变更

### 表变更
1. `webdav_sources`
- 新增 `checkin_enabled`（Boolean，`nullable=False`，默认 `True`）
- 新增 `checkin_time`（String(5)，`nullable=False`，默认 `04:00`）

2. `site_checkin_runs`
- 新增 `webdav_source_id`（String(36)，`nullable=True`）

3. `site_checkin_items`
- 新增 `account_id`（String(36)，`nullable=True`）
- 新增 `account_domain`（String(255)，`nullable=True`）
- 新增 `account_site_url`（String(500)，`nullable=True`）

### 索引
- `webdav_sources`：`idx_webdav_sources_checkin_enabled`（`is_active`, `checkin_enabled`）
- `site_checkin_runs`：`idx_site_checkin_runs_source_created`（`webdav_source_id`, `created_at`）
- `site_checkin_items`：`idx_site_checkin_items_run_account`（`run_id`, `account_id`）

## 迁移与回填

### 回填规则
- `webdav_sources.checkin_time`：优先取系统配置 `site_account_checkin_time`，无值则 `04:00`。
- `webdav_sources.checkin_enabled`：若系统配置 `enable_site_account_checkin=False`，回填为 `False`；否则 `True`。

### 旧配置清理
在迁移中删除以下 `system_configs.key`：
- `enable_all_api_hub_sync`
- `all_api_hub_sync_time`
- `all_api_hub_webdav_url`
- `all_api_hub_webdav_username`
- `all_api_hub_webdav_password`
- `enable_all_api_hub_auto_create_provider_ops`

## 后端接口变更

### Site Management API
1. `GET /api/admin/site-management/sources`
- 新增返回：`checkin_enabled`、`checkin_time`

2. `POST /api/admin/site-management/sources`
- 新增可选入参：`checkin_enabled`、`checkin_time`

3. `PUT /api/admin/site-management/sources/{source_id}`
- 新增可更新字段：`checkin_enabled`、`checkin_time`
- `checkin_time` 做 `HH:MM` 校验
- 成功后触发该源签到 job 刷新

4. `GET /api/admin/site-management/checkin-runs`
- 新增查询参数：`source_id`

5. `GET /api/admin/site-management/checkin-runs/{run_id}/items`
- 新增返回字段：`account_id`、`account_domain`、`account_site_url`

## 调度执行逻辑

### 行为
- 仅处理当前 `source_id` 下 `is_active=True` 且 `checkin_enabled=True` 的账号。
- 每个账号通过 `AccountOpsService.checkin(account_id)` 执行。
- 聚合成功/失败/跳过计数，落库 run + items。

### 异常策略
- 单账号失败不影响整源任务继续执行。
- 单源任务失败不影响其他源任务。
- job 重建失败仅记录日志，不影响调度器其他任务。

## 前端变更

### 站点管理
1. 源编辑弹窗 `WebDavSourceFormDialog.vue`
- 新增签到开关和时间选择输入。

2. 源卡片 `WebDavSourceCard.vue`
- 展示签到开关和时间摘要。

3. 签到历史 `SiteCheckinHistory.vue` + `CheckinHistoryTable.vue`
- 增加按源筛选。
- 明细展示账号域名与站点 URL。

4. 类型与 API
- `features/site-management/types.ts`、`api.ts` 增加字段和筛选参数。

### 系统设置移除旧配置
删除以下前端链路中的 all-api-hub WebDAV 逻辑：
- `SystemSettings.vue`
- `useSystemConfig.ts`
- `useScheduledTasks.ts`
- `ScheduledTasksSection.vue`

## 兼容性
1. 历史签到记录（无 `webdav_source_id`）仍可显示，归入“未知来源/历史记录”。
2. `provider_ops` 手动签到继续使用 `SiteManagementLogService.record_checkin_run`，不受新增字段影响。
3. 不破坏已有 `site_checkin_items` provider 维度字段。

## 测试策略

### 后端
- `tests/modules/site_management/test_webdav_source_service.py`
  - 覆盖新字段创建/更新与时间格式校验。
- `tests/modules/site_management/test_scheduler.py`
  - 覆盖按源调度注册、源配置变更后重建、执行仅限源内账号、日志落库。
- 新增/调整 routes 测试
  - 覆盖 `checkin-runs` 的 `source_id` 过滤。
- `tests/modules/site_management/test_models.py`
  - 覆盖新增字段存在性。
- 移除或改造旧 all-api-hub 配置相关测试。

### 前端
- `site-management` 组件测试
  - 覆盖新表单字段、保存 payload、筛选参数传递。

### 迁移验证
- `alembic upgrade head` 成功。
- 验证新增列、索引和回填值正确。
- 验证旧 all-api-hub 配置项被清理。

## 验收标准
1. 每个 WebDAV 源可独立配置签到开关与时间，并按配置执行。
2. 签到历史可按源筛选，展开可查看账号级明细。
3. 系统设置不再出现 all-api-hub WebDAV 配置项。
4. 后端 `SystemConfigService` 与 API 不再依赖 all-api-hub 旧配置。
