# All-API-Hub WebDAV Cookie Sync Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在 Aether 中新增 all-api-hub WebDAV 同步能力，按域名自动匹配 Provider 并更新 provider-ops Cookie 凭据（支持手动触发 + 定时任务）。

**Architecture:** 新增独立同步模块 `provider_sync`，负责 WebDAV 拉取、备份解析、域名匹配与凭据落库；通过 Admin API 暴露“测试连接/立即同步/预览结果”接口；通过系统配置与维护调度器接入定时执行。仅同步账号凭据，不做模型同步。

**Tech Stack:** FastAPI, SQLAlchemy, APScheduler, httpx, pytest, Vue 3 + TypeScript。

---

### Task 1: 建立备份解析与域名匹配契约

**Files:**
- Create: `src/services/provider_sync/all_api_hub_backup.py`
- Test: `tests/services/provider_sync/test_all_api_hub_backup.py`

**Step 1: Write the failing test**

```python
def test_extract_accounts_from_backup_v2_and_match_domain():
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://anyrouter.top/path",
                    "cookieAuth": {"sessionCookie": "session=abc"},
                }
            ]
        },
    }
    accounts = parse_all_api_hub_accounts(raw)
    assert accounts[0].domain == "anyrouter.top"
    assert accounts[0].session_cookie == "session=abc"
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/provider_sync/test_all_api_hub_backup.py::test_extract_accounts_from_backup_v2_and_match_domain -v`
Expected: FAIL（函数不存在）

**Step 3: Write minimal implementation**

```python
@dataclass
class ImportedAccount:
    domain: str
    session_cookie: str

def parse_all_api_hub_accounts(raw: dict[str, Any]) -> list[ImportedAccount]:
    ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/provider_sync/test_all_api_hub_backup.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/services/provider_sync/test_all_api_hub_backup.py src/services/provider_sync/all_api_hub_backup.py
git commit -m "test(provider-sync): add all-api-hub backup parser contract"
```

### Task 2: 实现 WebDAV 拉取客户端（Basic Auth + JSON）

**Files:**
- Create: `src/services/provider_sync/webdav_client.py`
- Test: `tests/services/provider_sync/test_webdav_client.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_download_backup_returns_json_text(httpx_mock):
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/provider_sync/test_webdav_client.py::test_download_backup_returns_json_text -v`
Expected: FAIL

**Step 3: Write minimal implementation**

```python
async def download_backup(url: str, username: str, password: str) -> str:
    auth = base64.b64encode(f"{username}:{password}".encode()).decode()
    ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/provider_sync/test_webdav_client.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/services/provider_sync/test_webdav_client.py src/services/provider_sync/webdav_client.py
git commit -m "feat(provider-sync): add webdav backup downloader"
```

### Task 3: 实现同步服务（域名匹配 + provider-ops 凭据更新）

**Files:**
- Create: `src/services/provider_sync/sync_service.py`
- Modify: `src/services/provider_ops/service.py`
- Test: `tests/services/provider_sync/test_sync_service.py`

**Step 1: Write the failing test**

```python
def test_sync_updates_provider_ops_credentials_by_domain(db_session):
    # provider.website = https://anyrouter.top
    # 导入账号 domain=anyrouter.top, session cookie=...
    # 断言 provider.config['provider_ops']['connector']['credentials'] 被更新
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/provider_sync/test_sync_service.py::test_sync_updates_provider_ops_credentials_by_domain -v`
Expected: FAIL

**Step 3: Write minimal implementation**

```python
class AllApiHubSyncService:
    async def sync_once(self, db: Session, *, dry_run: bool = False) -> SyncResult:
        ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/provider_sync/test_sync_service.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/services/provider_sync/test_sync_service.py src/services/provider_sync/sync_service.py src/services/provider_ops/service.py
git commit -m "feat(provider-sync): sync provider cookies by domain"
```

### Task 4: 增加 Admin API（测试连接 / 预览 / 立即同步）

**Files:**
- Create: `src/api/admin/provider_sync.py`
- Modify: `src/api/admin/__init__.py`
- Test: `tests/api/admin/test_provider_sync_api.py`

**Step 1: Write the failing test**

```python
def test_trigger_sync_returns_summary(client, admin_token):
    resp = client.post("/api/admin/provider-sync/trigger", headers=admin_headers(admin_token))
    assert resp.status_code == 200
    assert "matched" in resp.json()
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/api/admin/test_provider_sync_api.py::test_trigger_sync_returns_summary -v`
Expected: FAIL

**Step 3: Write minimal implementation**

```python
router = APIRouter(prefix="/api/admin/provider-sync", tags=["Provider Sync"])
@router.post("/trigger")
async def trigger_sync(...): ...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/api/admin/test_provider_sync_api.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/api/admin/test_provider_sync_api.py src/api/admin/provider_sync.py src/api/admin/__init__.py
git commit -m "feat(api): add all-api-hub sync admin endpoints"
```

### Task 5: 接入系统配置与定时任务

**Files:**
- Modify: `src/services/system/config.py`
- Modify: `src/services/system/maintenance_scheduler.py`
- Test: `tests/services/test_maintenance_scheduler_provider_sync.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_scheduled_provider_sync_runs_when_enabled(monkeypatch):
    ...
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/services/test_maintenance_scheduler_provider_sync.py::test_scheduled_provider_sync_runs_when_enabled -v`
Expected: FAIL

**Step 3: Write minimal implementation**

```python
# 新增配置 key:
# enable_all_api_hub_sync, all_api_hub_sync_time,
# all_api_hub_webdav_url, all_api_hub_webdav_username, all_api_hub_webdav_password
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/services/test_maintenance_scheduler_provider_sync.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add tests/services/test_maintenance_scheduler_provider_sync.py src/services/system/config.py src/services/system/maintenance_scheduler.py
git commit -m "feat(scheduler): add all-api-hub cookie sync job"
```

### Task 6: 管理端配置 UI（系统设置）

**Files:**
- Modify: `frontend/src/views/admin/system-settings/composables/useSystemConfig.ts`
- Modify: `frontend/src/views/admin/system-settings/composables/useScheduledTasks.ts`
- Create: `frontend/src/views/admin/system-settings/AllApiHubSyncSection.vue`
- Modify: `frontend/src/views/admin/system-settings/SystemSettings.vue`
- Create: `frontend/src/api/providerSync.ts`
- Test: `frontend/src/views/admin/system-settings/__tests__/AllApiHubSyncSection.test.ts`

**Step 1: Write the failing test**

```ts
it('submits trigger sync and renders summary', async () => {
  // mock /api/admin/provider-sync/trigger
})
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- AllApiHubSyncSection.test.ts`
Expected: FAIL

**Step 3: Write minimal implementation**

```ts
export async function triggerAllApiHubSync() {
  return client.post('/api/admin/provider-sync/trigger')
}
```

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- AllApiHubSyncSection.test.ts`
Expected: PASS

**Step 5: Commit**

```bash
git add frontend/src/api/providerSync.ts frontend/src/views/admin/system-settings/AllApiHubSyncSection.vue frontend/src/views/admin/system-settings/composables/useSystemConfig.ts frontend/src/views/admin/system-settings/composables/useScheduledTasks.ts frontend/src/views/admin/system-settings/SystemSettings.vue frontend/src/views/admin/system-settings/__tests__/AllApiHubSyncSection.test.ts
git commit -m "feat(frontend): add all-api-hub sync settings and trigger UI"
```

### Task 7: 端到端验证与文档

**Files:**
- Modify: `README.md`
- Create: `docs/deploy/all-api-hub-sync.md`

**Step 1: Write validation checklist**

```md
1) 填写 WebDAV 配置
2) 测试连接
3) 手动触发同步
4) 检查 Provider Auth 中 cookie 已更新
```

**Step 2: Run full verification**

Run:
- `source .venv/bin/activate && pytest tests/services/provider_sync tests/api/admin/test_provider_sync_api.py -v`
- `cd frontend && npm run test:run -- AllApiHubSyncSection.test.ts`

Expected: PASS

**Step 3: Update docs**

补充配置项说明、同步行为、失败排查与安全注意事项（密码加密存储）。

**Step 4: Commit**

```bash
git add README.md docs/deploy/all-api-hub-sync.md
git commit -m "docs: add all-api-hub cookie sync guide"
```
