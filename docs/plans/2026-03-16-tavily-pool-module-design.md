# Tavily Pool Module Design

## 1. 背景与目标

基于 `skernelx/tavily-key-generator` 的能力，在 Aether 中新增一个独立的 Tavily 账号池管理模块，形态与现有站点管理模块一致，支持：

- 统一后台风格的管理页面
- 独立 SQLite 持久化（`/sqlites/tavilies.db`）
- 模块内独立 token 管理体系（不复用现有密钥管理）
- 脚本创建与面板创建共用同一数据模型并确保落库
- 健康检查与定时维护能力

## 2. 关键决策

- 采用 **Aether 原生模块实现**，不直接 vendor 上游仓库代码
- 新增模块路径：`src/modules/tavily_pool/`
- 存储方案：独立 SQLite 引擎 `sqlite:////sqlites/tavilies.db`
- 后台权限：沿用现有管理员鉴权
- token 体系：仅模块内独立实现（加密、脱敏、激活/撤销）
- 首期范围：选择 C（含健康检查与定时维护）

## 3. 模块架构

### 3.1 后端目录建议

```text
src/modules/tavily_pool/
├── __init__.py
├── routes.py
├── schemas.py
├── sqlite.py
├── models.py
├── repositories/
│   ├── account_repo.py
│   ├── token_repo.py
│   └── run_repo.py
├── services/
│   ├── account_service.py
│   ├── token_service.py
│   ├── health_service.py
│   ├── maintenance_service.py
│   └── scheduler.py
└── scripts/
    └── create_accounts.py
```

### 3.2 前端目录建议

```text
frontend/src/features/tavily-pool/
├── api.ts
├── types.ts
└── components/

frontend/src/views/admin/
├── TavilyPoolList.vue
├── TavilyPoolDetail.vue
├── TavilyHealthHistory.vue
└── TavilyMaintenanceHistory.vue
```

## 4. 数据模型（SQLite）

### 4.1 `tavily_accounts`

- 主键：`id`（uuid）
- 字段：`email`（unique）、`password_encrypted`、`status`、`daily_limit`、`daily_used`、`last_used_at`
- 健康：`health_status`、`health_checked_at`、`fail_count`
- 元数据：`source`、`notes`、`created_at`、`updated_at`

### 4.2 `tavily_tokens`

- 主键：`id`（uuid）
- 外键：`account_id`
- 字段：`token_encrypted`、`token_masked`、`expires_at`、`revoked_at`、`is_active`
- 运行态：`last_used_at`、`last_error`
- 元数据：`created_by`、`created_at`、`updated_at`

### 4.3 `tavily_health_checks`

- 主键：`id`
- 关联：`account_id`、`token_id`（可空）
- 字段：`check_type`、`status`、`response_ms`、`error_message`、`details_json`、`checked_at`

### 4.4 `tavily_maintenance_runs`（可带 items）

- 主键：`id`
- 字段：`job_name`、`total`、`success`、`failed`、`skipped`、`status`、`error_message`
- 时间：`started_at`、`finished_at`

## 5. API 设计（`/api/admin/tavily-pool`）

- 账号：`GET/POST/PUT/DELETE /accounts`，单个与批量启停
- Token：`GET/POST /accounts/{id}/tokens`，`POST /tokens/{token_id}/revoke|activate`
- 健康：`POST /health-check/run`，`POST /health-check/{account_id}`，`GET /health-check/runs`
- 维护：`POST /maintenance/run`，`GET /maintenance/runs`

## 6. 脚本与一致性

- 脚本创建逻辑改为调用 `AccountService`/`TokenService`
- 禁止脚本直接写 SQL，保证与 API 完全一致的校验与落库行为
- 脚本运行结果写入 run 记录，便于审计

## 7. 调度与容错

- `tavily_health_check_job`：周期检测账号/token 可用性
- `tavily_maintenance_job`：失效标记、过期清理、状态修正
- 失败隔离：单条失败不阻断整批任务
- 并发控制：信号量 + 批次上限，避免 SQLite 竞争写

## 8. 配置与部署

- 新增环境变量：
  - `TAVILY_POOL_DB_PATH=/sqlites/tavilies.db`
  - `TAVILY_POOL_CRYPTO_KEY=...`
  - `TAVILY_POOL_HEALTH_CHECK_CRON=...`
- docker volume：`~/share_sqlites:/sqlites`
- 启动时自动执行模块内 SQLite schema 初始化

## 9. 验收标准

- 后台可独立管理 Tavily 账号池与 token
- 脚本创建后数据可在管理面板立即可见
- token 加密与脱敏展示生效，且不与现有密钥管理冲突
- 健康检查与维护任务可手动触发且可定时执行
- UI 风格、路由、权限行为与现有管理端一致
