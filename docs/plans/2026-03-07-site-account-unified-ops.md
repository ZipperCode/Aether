# Site Account Unified Ops Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现方案B：新增 `SiteAccount` 体系并复用 Provider Ops 执行引擎，使站点管理支持缓存化同步、可配置同步策略、未匹配站点签到/余额同步，以及独立定时任务。

**Architecture:** 新增站点账号与快照缓存模型；抽取统一执行引擎承载 connector/action 执行；Provider 与 SiteAccount 通过不同 target 适配同一执行链；维护调度器新增站点同步/签到/余额任务；前端站点管理默认读缓存并支持策略配置与强制刷新。

**Tech Stack:** FastAPI, SQLAlchemy, Alembic, APScheduler, httpx, Vue 3 + TypeScript, pytest, vitest.

---

### Task 1: 新增 SiteAccount/快照/余额运行日志数据模型

**Files:**
- Modify: `src/models/database.py`
- Test: `tests/models/test_site_account_models.py`

**Step 1: Write the failing test**

```python
def test_site_account_model_has_required_fields():
    assert hasattr(SiteAccount, "domain")
    assert hasattr(SiteAccount, "provider_id")
    assert hasattr(SiteAccount, "checkin_enabled")
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/models/test_site_account_models.py::test_site_account_model_has_required_fields -v`  
Expected: FAIL（模型未定义）

**Step 3: Write minimal implementation**

```python
class SiteAccount(Base):
    __tablename__ = "site_accounts"
    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()))
    domain = Column(String(255), nullable=False, index=True)
    provider_id = Column(String(36), ForeignKey("providers.id"), nullable=True, index=True)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/models/test_site_account_models.py -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/models/database.py tests/models/test_site_account_models.py
git commit -m "feat(site-account): add site account domain models"
```

### Task 2: 增加 Alembic 迁移并验证升级/回滚

**Files:**
- Create: `alembic/versions/<timestamp>_add_site_account_tables.py`
- Test: `tests/alembic/test_site_account_migration.py`

**Step 1: Write the failing test**

```python
def test_site_account_tables_exist_after_upgrade():
    # upgrade head 后断言 site_accounts/site_source_snapshots 存在
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/alembic/test_site_account_migration.py -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
def upgrade():
    op.create_table("site_accounts", ...)
    op.create_table("site_source_snapshots", ...)
```

**Step 4: Run migration verification**

Run: `alembic upgrade head && alembic downgrade -1 && alembic upgrade head`  
Expected: 全部成功，无异常

**Step 5: Commit**

```bash
git add alembic/versions/*_add_site_account_tables.py tests/alembic/test_site_account_migration.py
git commit -m "feat(db): add site account alembic migration"
```

### Task 3: 实现 WebDAV 快照缓存服务（非每次拉取）

**Files:**
- Create: `src/services/site_management/snapshot_service.py`
- Modify: `src/services/provider_sync/webdav_client.py`
- Test: `tests/services/site_management/test_snapshot_service.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_fetch_snapshot_uses_cache_when_not_expired():
    # 第二次调用应命中缓存，不再请求 webdav
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/site_management/test_snapshot_service.py::test_fetch_snapshot_uses_cache_when_not_expired -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
class SiteSnapshotService:
    async def get_snapshot(self, db: Session, *, force_refresh: bool = False) -> SnapshotResult:
        ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/site_management/test_snapshot_service.py -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/site_management/snapshot_service.py src/services/provider_sync/webdav_client.py tests/services/site_management/test_snapshot_service.py
git commit -m "feat(site-sync): add webdav snapshot cache service"
```

### Task 4: 实现快照应用策略（matched/unmatched）并落地 SiteAccount

**Files:**
- Create: `src/services/site_management/site_account_sync_service.py`
- Modify: `src/services/provider_sync/sync_service.py`
- Test: `tests/services/site_management/test_site_account_sync_service.py`

**Step 1: Write the failing test**

```python
def test_apply_snapshot_creates_unmatched_site_accounts_when_policy_enabled():
    # unmatched account 会写入 site_accounts，provider_id 为空
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/site_management/test_site_account_sync_service.py::test_apply_snapshot_creates_unmatched_site_accounts_when_policy_enabled -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
class SiteAccountSyncService:
    def apply_snapshot(self, db: Session, snapshot: dict[str, Any], policy: str) -> ApplyResult:
        ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/site_management/test_site_account_sync_service.py -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/site_management/site_account_sync_service.py src/services/provider_sync/sync_service.py tests/services/site_management/test_site_account_sync_service.py
git commit -m "feat(site-account): support unmatched account persistence and sync policy"
```

### Task 5: 抽取统一执行引擎（Provider/SiteAccount 共用）

**Files:**
- Create: `src/services/provider_ops/execution_engine.py`
- Modify: `src/services/provider_ops/service.py`
- Test: `tests/services/provider_ops/test_execution_engine_contract.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_execution_engine_returns_same_result_for_equivalent_targets():
    # provider target 与 site-account target 同配置结果一致
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/provider_ops/test_execution_engine_contract.py -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
class OpsExecutionEngine:
    async def execute(self, target: OpsTarget, action_type: ProviderActionType, action_config: dict[str, Any] | None = None) -> ActionResult:
        ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/provider_ops/test_execution_engine_contract.py tests/services/provider_ops -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/provider_ops/execution_engine.py src/services/provider_ops/service.py tests/services/provider_ops/test_execution_engine_contract.py
git commit -m "refactor(provider-ops): extract shared execution engine"
```

### Task 6: 新增 SiteAccount Ops（签到/余额）与 API

**Files:**
- Create: `src/services/site_management/site_account_ops_service.py`
- Modify: `src/api/admin/site_management.py`
- Test: `tests/api/admin/test_site_management_site_account_ops_api.py`

**Step 1: Write the failing test**

```python
def test_manual_checkin_for_unmatched_site_account_returns_action_result():
    resp = client.post("/api/admin/site-management/accounts/{id}/checkin")
    assert resp.status_code == 200
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/api/admin/test_site_management_site_account_ops_api.py::test_manual_checkin_for_unmatched_site_account_returns_action_result -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
@router.post("/accounts/{account_id}/checkin")
async def checkin_site_account(...):
    ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/api/admin/test_site_management_site_account_ops_api.py -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/site_management/site_account_ops_service.py src/api/admin/site_management.py tests/api/admin/test_site_management_site_account_ops_api.py
git commit -m "feat(site-management): add site account checkin and balance APIs"
```

### Task 7: 调度器接入站点同步策略 + 站点定时签到/余额同步

**Files:**
- Modify: `src/services/system/config.py`
- Modify: `src/services/system/maintenance_scheduler.py`
- Test: `tests/services/test_maintenance_scheduler_site_account_jobs.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_site_account_checkin_job_runs_without_provider_page_access():
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/test_maintenance_scheduler_site_account_jobs.py -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
# 新增配置键：
# site_sync_mode/site_sync_interval_minutes/site_sync_cron
# enable_site_account_checkin/enable_site_account_balance_sync
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/test_maintenance_scheduler_site_account_jobs.py tests/services/test_maintenance_scheduler_provider_sync.py -v`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/system/config.py src/services/system/maintenance_scheduler.py tests/services/test_maintenance_scheduler_site_account_jobs.py
git commit -m "feat(scheduler): add site account sync/checkin/balance jobs"
```

### Task 8: Provider 页面行为去耦（默认不隐式触发签到）

**Files:**
- Modify: `src/services/provider_ops/actions/balance.py`
- Modify: `frontend/src/features/providers/composables/useProviderBalance.ts`
- Test: `tests/services/provider_ops/test_balance_action_skip_checkin.py`
- Test: `frontend/src/features/providers/composables/__tests__/useProviderBalance.test.ts`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_balance_query_can_skip_checkin_when_flag_enabled():
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/provider_ops/test_balance_action_skip_checkin.py -v`  
Expected: FAIL

**Step 3: Write minimal implementation**

```python
if self.config.get("skip_checkin"):
    checkin_result = None
else:
    checkin_result = await self._do_checkin(client)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/provider_ops/test_balance_action_skip_checkin.py -v && cd frontend && npm run test:run -- useProviderBalance.test.ts`  
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/provider_ops/actions/balance.py frontend/src/features/providers/composables/useProviderBalance.ts tests/services/provider_ops/test_balance_action_skip_checkin.py frontend/src/features/providers/composables/__tests__/useProviderBalance.test.ts
git commit -m "refactor(balance): decouple provider page balance from implicit checkin"
```

### Task 9: 站点管理前端改为“缓存优先 + 策略配置 + 未匹配站点操作”

**Files:**
- Modify: `frontend/src/api/admin.ts`
- Modify: `frontend/src/views/admin/SiteManagement.vue`
- Create: `frontend/src/views/admin/system-settings/site-management/SyncStrategySection.vue`
- Modify: `frontend/src/views/admin/system-settings/composables/useSystemConfig.ts`
- Test: `frontend/src/views/admin/__tests__/SiteManagement.test.ts`

**Step 1: Write the failing test**

```ts
it('loads cached site accounts and can force refresh snapshot', async () => {
  // 默认读取缓存接口，点击按钮后请求 force_refresh=true
})
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- SiteManagement.test.ts`  
Expected: FAIL

**Step 3: Write minimal implementation**

```ts
async getSiteAccounts(forceRefresh = false) {
  return apiClient.get('/api/admin/site-management/accounts', { params: { force_refresh: forceRefresh } })
}
```

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- SiteManagement.test.ts && npm run type-check`  
Expected: PASS

**Step 5: Commit**

```bash
git add frontend/src/api/admin.ts frontend/src/views/admin/SiteManagement.vue frontend/src/views/admin/system-settings/site-management/SyncStrategySection.vue frontend/src/views/admin/system-settings/composables/useSystemConfig.ts frontend/src/views/admin/__tests__/SiteManagement.test.ts
git commit -m "feat(frontend): add cached site account flow and sync strategy controls"
```

### Task 10: 端到端回归与文档

**Files:**
- Modify: `docs/plans/2026-03-07-site-account-unified-ops-design.md`
- Create: `docs/site-management/site-account-ops.md`

**Step 1: Run backend targeted tests**

Run: `pytest tests/services/site_management tests/api/admin/test_site_management* tests/services/provider_ops tests/services/test_maintenance_scheduler_site_account_jobs.py -v`  
Expected: PASS

**Step 2: Run frontend verification**

Run: `cd frontend && npm run lint && npm run type-check && npm run test:run`  
Expected: PASS

**Step 3: Run integration smoke**

Run: `pytest tests/services/provider_sync tests/api/admin/test_provider_sync_api.py tests/api/admin/test_site_management_api.py -v`  
Expected: PASS

**Step 4: Update docs**

```markdown
# Site Account Ops
- 同步策略说明
- 缓存行为
- 未匹配站点签到/余额配置方式
```

**Step 5: Commit**

```bash
git add docs/plans/2026-03-07-site-account-unified-ops-design.md docs/site-management/site-account-ops.md
git commit -m "docs(site-management): document unified site account ops and sync strategy"
```
