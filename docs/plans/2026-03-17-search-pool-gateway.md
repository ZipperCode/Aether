# Search Pool Gateway Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 用 `search_pool_gateway` 替换 `tavily_pool`，实现 Tavily + Firecrawl 双服务搜索池网关，并兼容 `/api/search`、`/api/extract`、`/firecrawl/*`。

**Architecture:** 新增独立模块 `src/modules/search_pool_gateway`（独立 SQLite、服务分池轮询、独立网关 token 鉴权、代理转发）。管理端采用 `/api/admin/search-pool`；对外兼容路由由模块直接注册。前端替换 Tavily 页面为搜索池网关页面，复用现有 UI 组件风格。

**Tech Stack:** FastAPI, SQLAlchemy, SQLite, httpx, Vue 3, TypeScript, Vitest, pytest.

---

### Task 1: 建立后端模块骨架与最小可运行路由

**Files:**
- Create: `src/modules/search_pool_gateway/__init__.py`
- Create: `src/modules/search_pool_gateway/models.py`
- Create: `src/modules/search_pool_gateway/sqlite.py`
- Create: `src/modules/search_pool_gateway/schemas.py`
- Create: `src/modules/search_pool_gateway/routes_admin.py`
- Create: `src/modules/search_pool_gateway/routes_proxy.py`
- Create: `src/modules/search_pool_gateway/services/__init__.py`
- Create: `src/modules/search_pool_gateway/repositories/__init__.py`
- Test: `tests/modules/search_pool_gateway/test_module_bootstrap.py`

**Step 1: Write the failing test**

```python
from fastapi.testclient import TestClient
from src.main import app


def test_search_pool_gateway_admin_router_registered(admin_auth_headers):
    client = TestClient(app)
    resp = client.get("/api/admin/search-pool/keys", headers=admin_auth_headers)
    assert resp.status_code != 404
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_module_bootstrap.py -v`
Expected: FAIL with `404` or import error (module not found).

**Step 3: Write minimal implementation**

```python
# __init__.py 中先定义 ModuleDefinition，并在 router_factory 中合并 admin/proxy router
combined_router = APIRouter()
combined_router.include_router(admin_router)
combined_router.include_router(proxy_router)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_module_bootstrap.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add tests/modules/search_pool_gateway/test_module_bootstrap.py src/modules/search_pool_gateway
git commit -m "feat(search-pool): scaffold module and register base routes"
```

### Task 2: 实现 SQLite 模型与仓储（key/token/log）

**Files:**
- Modify: `src/modules/search_pool_gateway/models.py`
- Modify: `src/modules/search_pool_gateway/sqlite.py`
- Create: `src/modules/search_pool_gateway/repositories/key_repo.py`
- Create: `src/modules/search_pool_gateway/repositories/token_repo.py`
- Test: `tests/modules/search_pool_gateway/test_repositories.py`

**Step 1: Write the failing test**

```python
def test_key_repo_creates_and_lists_by_service(session_factory):
    with session_factory() as db:
        repo = GatewayKeyRepository(db)
        repo.create(service="tavily", raw_key="tvly-abc", email="a@example.com")
        repo.create(service="firecrawl", raw_key="fc-abc", email="b@example.com")
        tavily = repo.list_keys(service="tavily")
        assert len(tavily) == 1
        assert tavily[0].service == "tavily"
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_repositories.py -v`
Expected: FAIL because repository/model incomplete.

**Step 3: Write minimal implementation**

```python
class GatewayApiKey(GatewayBase):
    __tablename__ = "gateway_api_keys"
    # service/key_encrypted/key_masked/active/usage fields...
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_repositories.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/models.py src/modules/search_pool_gateway/sqlite.py src/modules/search_pool_gateway/repositories tests/modules/search_pool_gateway/test_repositories.py
git commit -m "feat(search-pool): add sqlite models and repositories"
```

### Task 3: 实现管理 API（keys/tokens 基础 CRUD）

**Files:**
- Create: `src/modules/search_pool_gateway/services/key_service.py`
- Create: `src/modules/search_pool_gateway/services/token_service.py`
- Modify: `src/modules/search_pool_gateway/schemas.py`
- Modify: `src/modules/search_pool_gateway/routes_admin.py`
- Test: `tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py`

**Step 1: Write the failing test**

```python
def test_admin_can_create_list_toggle_delete_key(client, admin_auth_headers):
    create = client.post("/api/admin/search-pool/keys", json={"service": "tavily", "key": "tvly-xyz"}, headers=admin_auth_headers)
    assert create.status_code == 200
    items = client.get("/api/admin/search-pool/keys?service=tavily", headers=admin_auth_headers).json()["keys"]
    assert len(items) == 1
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py -v`
Expected: FAIL (endpoint 未实现或返回结构不匹配)。

**Step 3: Write minimal implementation**

```python
@router.post("/keys")
def create_key(...):
    return key_service.create_key(payload)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/routes_admin.py src/modules/search_pool_gateway/services src/modules/search_pool_gateway/schemas.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py
git commit -m "feat(search-pool): implement admin key and token crud"
```

### Task 4: 实现服务分池轮询与失败摘除

**Files:**
- Create: `src/modules/search_pool_gateway/services/key_pool.py`
- Modify: `src/modules/search_pool_gateway/services/key_service.py`
- Test: `tests/modules/search_pool_gateway/test_key_pool.py`

**Step 1: Write the failing test**

```python
def test_pool_round_robin_and_disable_after_three_failures(session_factory):
    pool = ServiceKeyPool(session_factory)
    k1 = pool.get_next_key("tavily")
    k2 = pool.get_next_key("tavily")
    assert k1.id != k2.id
    pool.report_result("tavily", k1.id, success=False)
    pool.report_result("tavily", k1.id, success=False)
    pool.report_result("tavily", k1.id, success=False)
    again = pool.get_next_key("tavily")
    assert again.id != k1.id
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_key_pool.py -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```python
class ServiceKeyPool:
    # per-service keys + index
    # report_result: fail>=3 -> deactivate and reload
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_key_pool.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/services/key_pool.py src/modules/search_pool_gateway/services/key_service.py tests/modules/search_pool_gateway/test_key_pool.py
git commit -m "feat(search-pool): add per-service key pool and failover"
```

### Task 5: 实现网关对外兼容代理路由

**Files:**
- Create: `src/modules/search_pool_gateway/services/proxy_service.py`
- Modify: `src/modules/search_pool_gateway/routes_proxy.py`
- Modify: `src/modules/search_pool_gateway/services/token_service.py`
- Test: `tests/modules/search_pool_gateway/test_proxy_routes.py`

**Step 1: Write the failing test**

```python
@pytest.mark.asyncio
async def test_proxy_search_uses_gateway_token_and_forwards(monkeypatch, async_client):
    resp = await async_client.post(
        "/api/search",
        headers={"Authorization": "Bearer spg-tavily-token"},
        json={"query": "hello"},
    )
    assert resp.status_code == 200
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -v`
Expected: FAIL (鉴权/转发未实现)。

**Step 3: Write minimal implementation**

```python
@router.post("/api/search")
async def proxy_tavily_search(request: Request):
    return await proxy_service.forward_tavily("search", request)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/routes_proxy.py src/modules/search_pool_gateway/services/proxy_service.py src/modules/search_pool_gateway/services/token_service.py tests/modules/search_pool_gateway/test_proxy_routes.py
git commit -m "feat(search-pool): add compatible proxy routes for tavily and firecrawl"
```

### Task 6: 实现统计与额度同步管理接口

**Files:**
- Create: `src/modules/search_pool_gateway/services/usage_service.py`
- Modify: `src/modules/search_pool_gateway/routes_admin.py`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`

**Step 1: Write the failing test**

```python
def test_admin_usage_sync_updates_usage_fields(client, admin_auth_headers):
    resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True}, headers=admin_auth_headers)
    assert resp.status_code == 200
    assert "result" in resp.json()
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```python
@router.post("/usage/sync")
async def sync_usage(...):
    return usage_service.sync(...)
```

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/routes_admin.py src/modules/search_pool_gateway/services/usage_service.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "feat(search-pool): add usage sync and stats endpoints"
```

### Task 7: 替换前端为搜索池网关页面并保持现有风格

**Files:**
- Create: `frontend/src/features/search-pool/api.ts`
- Create: `frontend/src/features/search-pool/types.ts`
- Create: `frontend/src/views/admin/SearchPoolDashboard.vue`
- Create: `frontend/src/views/admin/SearchPoolKeys.vue`
- Create: `frontend/src/views/admin/SearchPoolTokens.vue`
- Modify: `frontend/src/router/index.ts`
- Delete: `frontend/src/features/tavily-pool/api.ts`
- Delete: `frontend/src/features/tavily-pool/types.ts`
- Delete: `frontend/src/features/tavily-pool/components/TavilyAccountImportDialog.vue`
- Delete: `frontend/src/views/admin/TavilyPoolList.vue`
- Delete: `frontend/src/views/admin/TavilyPoolDetail.vue`
- Delete: `frontend/src/views/admin/TavilyHealthHistory.vue`
- Delete: `frontend/src/views/admin/TavilyMaintenanceHistory.vue`
- Test: `frontend/src/features/search-pool/__tests__/api.spec.ts`

**Step 1: Write the failing test**

```ts
it('list keys calls /api/admin/search-pool/keys', async () => {
  await searchPoolApi.listKeys('tavily')
  expect(apiClient.get).toHaveBeenCalledWith('/api/admin/search-pool/keys', { params: { service: 'tavily' } })
})
```

**Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:run -- src/features/search-pool/__tests__/api.spec.ts`
Expected: FAIL (模块不存在)。

**Step 3: Write minimal implementation**

```ts
const BASE = '/api/admin/search-pool'
export const searchPoolApi = { listKeys: async (service) => ... }
```

**Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:run -- src/features/search-pool/__tests__/api.spec.ts`
Expected: PASS.

**Step 5: Commit**

```bash
git add frontend/src/router/index.ts frontend/src/features/search-pool frontend/src/views/admin/SearchPoolDashboard.vue frontend/src/views/admin/SearchPoolKeys.vue frontend/src/views/admin/SearchPoolTokens.vue frontend/src/features/tavily-pool frontend/src/views/admin/Tavily*.vue
git commit -m "feat(frontend): replace tavily pool pages with search pool gateway ui"
```

### Task 8: 删除旧 Tavily 模块并完成回归验证

**Files:**
- Delete: `src/modules/tavily_pool/**`
- Modify: `tests/modules/tavily_pool/*`（删除或迁移为 search_pool_gateway 测试）
- Create: `tests/modules/search_pool_gateway/test_backward_incompatible_removal.py`
- Modify: `.env.example`（新增 `SEARCH_POOL_GATEWAY_*` 变量）
- Modify: `README.md`（更新模块说明与接口）

**Step 1: Write the failing test**

```python
def test_tavily_pool_module_no_longer_discovered():
    from src.modules import ALL_MODULES
    names = {m.metadata.name for m in ALL_MODULES}
    assert "tavily_pool" not in names
    assert "search_pool_gateway" in names
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_backward_incompatible_removal.py -v`
Expected: FAIL (`tavily_pool` 仍在)。

**Step 3: Write minimal implementation**

```text
删除 src/modules/tavily_pool 目录与对应过时测试，补齐新模块文档与配置。
```

**Step 4: Run test to verify it passes**

Run:
- `pytest tests/modules/search_pool_gateway -v`
- `pytest -q`
- `cd frontend && npm run test:run`
- `cd frontend && npm run type-check`

Expected: 全部 PASS。

**Step 5: Commit**

```bash
git add src/modules/tavily_pool tests/modules docs .env.example README.md
git commit -m "refactor(search-pool): remove tavily pool and finalize gateway replacement"
```

### Task 9: 最终验证与交付检查

**Files:**
- Modify: `docs/plans/2026-03-17-search-pool-gateway.md`（记录实际偏差与完成状态）

**Step 1: Run full verification suite**

Run:
- `pytest --cov=src`
- `cd frontend && npm run lint && npm run type-check && npm run test:run`

Expected: 通过；若失败，逐项修复并重复。

**Step 2: Smoke test proxy compatibility**

Run:
- 使用测试 token 调 `POST /api/search`
- 使用测试 token 调 `POST /api/extract`
- 使用测试 token 调 `POST /firecrawl/v2/scrape`

Expected: 请求可达、鉴权生效、转发行为正确。

**Step 3: Final commit**

```bash
git add -A
git commit -m "chore(search-pool): finalize replacement verification"
```
