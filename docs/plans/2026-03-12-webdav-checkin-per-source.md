# WebDAV 独立签到与旧配置移除 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为每个 WebDAV 源提供独立签到调度与日志视图，并前后端彻底移除旧 all-api-hub WebDAV 配置逻辑。

**Architecture:** 在 `webdav_sources` 增加源级签到配置；`SiteManagementScheduler` 改为按源注册独立 cron job；签到日志补齐 source/account 维度字段并支持 API 过滤；前端站点管理页面接入新配置与筛选；系统设置删除旧 all-api-hub 配置入口与读写链路。

**Tech Stack:** FastAPI + SQLAlchemy + Alembic + Pytest；Vue 3 + TypeScript + Vitest。

---

### Task 1: 数据模型与迁移（源级签到配置 + 日志维度）

**Files:**
- Create: `alembic/versions/20260312_1100_add_site_source_checkin_fields_and_cleanup_legacy_webdav_configs.py`
- Modify: `src/modules/site_management/models.py`
- Modify: `src/modules/site_management/schemas.py`
- Test: `tests/modules/site_management/test_models.py`

**Step 1: Write the failing test**

```python
def test_webdav_source_has_checkin_fields() -> None:
    assert hasattr(WebDavSource, "checkin_enabled")
    assert hasattr(WebDavSource, "checkin_time")

def test_site_checkin_run_has_webdav_source_id() -> None:
    assert hasattr(SiteCheckinRun, "webdav_source_id")

def test_site_checkin_item_has_account_dimension_fields() -> None:
    for field in ("account_id", "account_domain", "account_site_url"):
        assert hasattr(SiteCheckinItem, field)
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/site_management/test_models.py -q`
Expected: FAIL，提示新增字段不存在。

**Step 3: Write minimal implementation**

```python
# src/modules/site_management/models.py
checkin_enabled = Column(Boolean, nullable=False, default=True)
checkin_time = Column(String(5), nullable=False, default="04:00")

# SiteCheckinRun
webdav_source_id = Column(String(36), nullable=True, index=True)

# SiteCheckinItem
account_id = Column(String(36), nullable=True)
account_domain = Column(String(255), nullable=True)
account_site_url = Column(String(500), nullable=True)
```

迁移中同时完成：
- 新增列与索引。
- 从 `site_account_checkin_time`、`enable_site_account_checkin` 回填 `webdav_sources`。
- 删除 `system_configs` 中旧 `all_api_hub_*` 键。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/site_management/test_models.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add alembic/versions/20260312_1100_add_site_source_checkin_fields_and_cleanup_legacy_webdav_configs.py src/modules/site_management/models.py src/modules/site_management/schemas.py tests/modules/site_management/test_models.py
git commit -m "feat(site-mgmt): add per-source checkin fields and migration"
```

### Task 2: WebDAV Source 服务与 API 入参/返回扩展

**Files:**
- Modify: `src/modules/site_management/schemas.py`
- Modify: `src/modules/site_management/services/webdav_source_service.py`
- Modify: `src/modules/site_management/routes.py`
- Test: `tests/modules/site_management/test_webdav_source_service.py`
- Test: `tests/modules/site_management/test_routes.py`（新建）

**Step 1: Write the failing test**

```python
def test_update_source_supports_checkin_config(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source
    service.update("source-1", checkin_enabled=False, checkin_time="08:30")
    assert fake_source.checkin_enabled is False
    assert fake_source.checkin_time == "08:30"
```

为 routes 新增：
- `POST/PUT /sources` 可读写 `checkin_enabled/checkin_time`。
- `GET /sources` 返回包含这两个字段。

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/site_management/test_webdav_source_service.py tests/modules/site_management/test_routes.py -q`
Expected: FAIL。

**Step 3: Write minimal implementation**

```python
# schemas.py
class CreateWebDavSourceRequest(BaseModel):
    ...
    checkin_enabled: bool = True
    checkin_time: str = "04:00"

class UpdateWebDavSourceRequest(BaseModel):
    ...
    checkin_enabled: bool | None = None
    checkin_time: str | None = None
```

在 service 中添加时间校验（`HH:MM`）并在 create/update 落库；在 routes 序列化中返回字段。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/site_management/test_webdav_source_service.py tests/modules/site_management/test_routes.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/site_management/schemas.py src/modules/site_management/services/webdav_source_service.py src/modules/site_management/routes.py tests/modules/site_management/test_webdav_source_service.py tests/modules/site_management/test_routes.py
git commit -m "feat(site-mgmt): expose per-source checkin config in api"
```

### Task 3: 调度器改造为“按源独立 job”

**Files:**
- Modify: `src/modules/site_management/services/scheduler.py`
- Modify: `src/modules/site_management/__init__.py`
- Test: `tests/modules/site_management/test_scheduler.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_register_source_specific_checkin_jobs(monkeypatch):
    scheduler = SiteManagementScheduler()
    # mock get_scheduler().add_cron_job called per source
    ...
    assert "site_account_checkin:s1" in registered_job_ids
    assert "site_account_checkin:s2" in registered_job_ids
```

新增测试覆盖：
- 源配置变更后 `refresh_source_checkin_job(source_id)` 重建任务。
- `_perform_site_account_checkin(source_id=...)` 仅处理该源账号。

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/site_management/test_scheduler.py -q`
Expected: FAIL。

**Step 3: Write minimal implementation**

```python
CHECKIN_JOB_PREFIX = "site_account_checkin:"

def _source_checkin_job_id(source_id: str) -> str:
    return f"{CHECKIN_JOB_PREFIX}{source_id}"

async def _scheduled_site_account_checkin_for_source(source_id: str) -> None:
    scheduler = SiteManagementScheduler()
    await scheduler._perform_site_account_checkin(source_id=source_id)
```

- 启动时遍历活跃源注册 job。
- 停止时删除 prefix 匹配 job。
- 新增 `refresh_source_checkin_job` 与 `refresh_all_source_checkin_jobs`。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/site_management/test_scheduler.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/site_management/services/scheduler.py src/modules/site_management/__init__.py tests/modules/site_management/test_scheduler.py
git commit -m "feat(site-mgmt): schedule checkin jobs per webdav source"
```

### Task 4: 签到日志 source/account 维度化与查询筛选

**Files:**
- Modify: `src/modules/site_management/services/log_service.py`
- Modify: `src/modules/site_management/services/scheduler.py`
- Modify: `src/modules/site_management/routes.py`
- Modify: `src/modules/site_management/schemas.py`
- Test: `tests/modules/site_management/test_routes.py`
- Test: `tests/modules/site_management/test_scheduler.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_list_checkin_runs_supports_source_filter(client, admin_headers):
    resp = client.get("/api/admin/site-management/checkin-runs", params={"source_id": "s1"}, headers=admin_headers)
    assert resp.status_code == 200
    for item in resp.json()["items"]:
        assert item["webdav_source_id"] == "s1"
```

再加：`checkin-runs/{id}/items` 返回 `account_domain/account_site_url`。

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/site_management/test_routes.py tests/modules/site_management/test_scheduler.py -q`
Expected: FAIL。

**Step 3: Write minimal implementation**

```python
# routes.py
@router.get("/checkin-runs")
async def list_checkin_runs(..., source_id: str | None = Query(None)):
    query = db.query(SiteCheckinRun)
    if source_id:
        query = query.filter(SiteCheckinRun.webdav_source_id == source_id)
```

- `record_checkin_run` 接受并落库 `webdav_source_id`。
- scheduler 写入 run/items 时携带 source/account 字段。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/site_management/test_routes.py tests/modules/site_management/test_scheduler.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/site_management/services/log_service.py src/modules/site_management/services/scheduler.py src/modules/site_management/routes.py src/modules/site_management/schemas.py tests/modules/site_management/test_routes.py tests/modules/site_management/test_scheduler.py
git commit -m "feat(site-mgmt): add source-scoped checkin logs and filtering"
```

### Task 5: 前端接入源级签到配置与日志筛选

**Files:**
- Modify: `frontend/src/features/site-management/types.ts`
- Modify: `frontend/src/features/site-management/api.ts`
- Modify: `frontend/src/features/site-management/components/WebDavSourceFormDialog.vue`
- Modify: `frontend/src/features/site-management/components/WebDavSourceCard.vue`
- Modify: `frontend/src/features/site-management/components/CheckinHistoryTable.vue`
- Modify: `frontend/src/views/admin/SiteCheckinHistory.vue`
- Test: `frontend/src/features/site-management/__tests__/api.spec.ts`（新建）

**Step 1: Write the failing test**

```ts
it('passes source_id when listing checkin runs', async () => {
  await siteManagementApi.listCheckinRuns({ page: 1, source_id: 's1' })
  expect(apiClient.get).toHaveBeenCalledWith('/api/admin/site-management/checkin-runs', {
    params: { page: 1, source_id: 's1' },
  })
})
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- src/features/site-management/__tests__/api.spec.ts`
Expected: FAIL。

**Step 3: Write minimal implementation**

```ts
export interface WebDavSource {
  ...
  checkin_enabled: boolean
  checkin_time: string
}

export interface CheckinRun {
  ...
  webdav_source_id: string | null
}
```

- 表单新增签到开关+时间选择并回传 payload。
- 历史表增加 source 过滤控件并传递 `source_id`。

**Step 4: Run test to verify it passes**

Run:
- `cd frontend && npm run test:run -- src/features/site-management/__tests__/api.spec.ts`
- `cd frontend && npm run type-check`
Expected: PASS。

**Step 5: Commit**

```bash
git add frontend/src/features/site-management/types.ts frontend/src/features/site-management/api.ts frontend/src/features/site-management/components/WebDavSourceFormDialog.vue frontend/src/features/site-management/components/WebDavSourceCard.vue frontend/src/features/site-management/components/CheckinHistoryTable.vue frontend/src/views/admin/SiteCheckinHistory.vue frontend/src/features/site-management/__tests__/api.spec.ts
git commit -m "feat(frontend): support per-source checkin config and source filter"
```

### Task 6: 前后端移除旧 all-api-hub WebDAV 配置逻辑

**Files:**
- Modify: `src/services/system/config.py`
- Modify: `src/api/admin/system.py`
- Modify: `frontend/src/views/admin/SystemSettings.vue`
- Modify: `frontend/src/views/admin/system-settings/composables/useSystemConfig.ts`
- Modify: `frontend/src/views/admin/system-settings/composables/useScheduledTasks.ts`
- Modify: `frontend/src/views/admin/system-settings/ScheduledTasksSection.vue`
- Delete/Modify: `tests/services/test_maintenance_scheduler_provider_sync.py`
- Modify: 相关前端定时任务配置测试（如有）

**Step 1: Write the failing test**

```python
def test_legacy_all_api_hub_keys_not_in_default_configs():
    assert "all_api_hub_webdav_url" not in SystemConfigService.DEFAULT_CONFIGS
    assert "enable_all_api_hub_sync" not in SystemConfigService.DEFAULT_CONFIGS
```

前端新增断言：系统设置配置 key 列表不再包含 `all_api_hub_*`。

**Step 2: Run test to verify it fails**

Run:
- `pytest tests/services/test_system_config.py -q`（若不存在则新建）
- `cd frontend && npm run test:run -- src/views/admin/system-settings/__tests__/useSystemConfig.spec.ts`
Expected: FAIL。

**Step 3: Write minimal implementation**

```python
# config.py
DEFAULT_CONFIGS = {
    ...
    # remove all_api_hub_* keys
}
SENSITIVE_KEYS = {"smtp_password"}
```

- 前端删掉 all-api-hub 对应字段、UI、保存逻辑。
- 移除维护调度/系统设置里对这些键的引用（若存在）。

**Step 4: Run test to verify it passes**

Run:
- `pytest tests/services/test_system_config.py -q`
- `cd frontend && npm run test:run -- src/views/admin/system-settings/__tests__/useSystemConfig.spec.ts`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/services/system/config.py src/api/admin/system.py frontend/src/views/admin/SystemSettings.vue frontend/src/views/admin/system-settings/composables/useSystemConfig.ts frontend/src/views/admin/system-settings/composables/useScheduledTasks.ts frontend/src/views/admin/system-settings/ScheduledTasksSection.vue tests/services/test_maintenance_scheduler_provider_sync.py tests/services/test_system_config.py
git commit -m "refactor(config): remove legacy all-api-hub webdav settings"
```

### Task 7: 端到端验证与收尾

**Files:**
- Modify: `docs/plans/2026-03-12-webdav-checkin-per-source.md`（补执行记录）
- Optional Modify: `docs/` 中相关说明

**Step 1: Run backend test suite subset**

Run:
- `pytest tests/modules/site_management -q`
- `pytest tests/services/test_system_config.py -q`

Expected: PASS。

**Step 2: Run frontend verification**

Run:
- `cd frontend && npm run type-check`
- `cd frontend && npm run test:run -- src/features/site-management src/views/admin/system-settings`

Expected: PASS。

**Step 3: Run migration verification**

Run:
- `alembic upgrade head`
- `alembic downgrade <target_revision>`
- `alembic upgrade head`

Expected: 均成功，无异常。

**Step 4: Manual smoke checklist**

```text
- 新建 WebDAV 源，设置签到开关/时间并保存成功
- 修改签到时间后无需重启即可生效（查看下次执行时间）
- 签到历史按 source 过滤有效
- 系统设置不再显示 all-api-hub WebDAV 配置
```

**Execution Record (2026-03-12)**

- Backend:
  - `uv run pytest tests/modules/site_management -q` -> `78 passed, 1 warning`
  - `uv run pytest tests/services/test_system_config.py -q` -> `2 passed`
- Frontend:
  - `cd frontend && npm run type-check` -> PASS
  - `cd frontend && npm run test:run -- src/features/site-management src/views/admin/system-settings` -> `2 passed`
- Migration:
  - 使用 docker 容器 `aether-postgres` 的 `POSTGRES_USER/POSTGRES_PASSWORD/POSTGRES_DB` 组装 `DATABASE_URL=postgresql://postgres:***@localhost:5432/aether`
  - 在 feature worktree 下执行 `uv run alembic current && uv run alembic upgrade head && uv run alembic downgrade <target_revision> && uv run alembic upgrade head && uv run alembic current`
  - 结果：初始 revision 为 `a9b1c2d3e4f5 (head)`，成功回滚到 `f6a7b8c9d0e1` 后重新升级回 `a9b1c2d3e4f5 (head)`
- Manual smoke:
  - 本轮未执行 UI 手工冒烟；仅完成自动化验证与迁移链路验证

**Step 5: Commit**

```bash
git add -A
git commit -m "test(site-mgmt): verify per-source checkin workflow end-to-end"
```
