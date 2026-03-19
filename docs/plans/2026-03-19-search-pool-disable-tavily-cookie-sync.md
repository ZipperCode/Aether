# Search Pool Disable Tavily Cookie Sync Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 下线 Tavily 基于环境变量 Cookie 的额度同步实现，并将管理端同步能力回退为占位状态。

**Architecture:** 后端在 `GatewayUsageService.sync()` 的 Tavily 分支直接返回固定占位结果，不再请求 Tavily 控制台，也不再依赖任何 Cookie 相关环境变量。前端继续保留按钮与接口，但同步成功提示改为“暂未启用”，同时删掉 `.env.example` 中已废弃配置，并调整测试覆盖新行为。

**Tech Stack:** Python, FastAPI, SQLAlchemy, Vue 3, TypeScript, Vitest, Pytest

---

### Task 1: 固化 Tavily 占位同步测试

**Files:**
- Modify: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`

**Step 1: 写失败测试，覆盖 Tavily 同步占位结果**

```python
def test_admin_usage_sync_returns_placeholder_for_tavily(...):
    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    result = sync_resp.json()["result"]
    assert result["service"] == "tavily"
    assert result["synced_keys"] == 0
    assert result["errors"] == 0
    assert "暂未启用" in result["message"]
```

**Step 2: 写失败测试，确认不依赖环境变量也不改写 usage 字段**

```python
def test_admin_usage_sync_tavily_keeps_existing_usage_fields(...):
    create_key = client.post("/api/admin/search-pool/keys", json={"service": "tavily", "key": "tvly-dev-xxx"})
    # 通过 session 直接写入 usage 字段后执行 sync
    assert key["usage_key_used"] == 18
    assert key["usage_sync_error"] == ""
```

**Step 3: 运行测试确认失败**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: FAIL，失败点落在 Tavily 仍尝试读取 Cookie / 执行真实同步。

**Step 4: 提交测试改动**

```bash
git add tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "test(search-pool): cover placeholder tavily usage sync"
```

### Task 2: 下线后端 Cookie 同步实现

**Files:**
- Modify: `src/modules/search_pool_gateway/services/usage_service.py`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`

**Step 1: 删除 Tavily Cookie 环境变量与控制台请求实现**

```python
# 删除:
TAVILY_SYNC_URL = ...
TAVILY_SYNC_COOKIE_ENV = ...
TAVILY_SYNC_USER_AGENT_ENV = ...
TAVILY_SYNC_REFERER_ENV = ...
def _fetch_tavily_console_keys(...): ...
```

**Step 2: 将 Tavily 分支改为固定占位返回**

```python
def _sync_tavily_usage(self) -> dict[str, Any]:
    now = datetime.now(timezone.utc)
    return {
        "service": "tavily",
        "synced_keys": 0,
        "errors": 0,
        "message": "Tavily 额度同步能力暂未启用",
        "synced_at": now.isoformat(),
    }
```

**Step 3: 保持现有 usage 字段不变**

```python
# 不查询、不更新、不提交 key usage 字段
```

**Step 4: 运行测试确认通过**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: PASS

**Step 5: 提交后端改动**

```bash
git add src/modules/search_pool_gateway/services/usage_service.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "feat(search-pool): disable tavily cookie sync"
```

### Task 3: 调整前端同步提示

**Files:**
- Modify: `frontend/src/views/admin/SearchPoolServiceWorkspace.vue`
- Test: `frontend/src/views/admin/__tests__/search-pool-pages.spec.ts`

**Step 1: 修改同步成功提示文案**

```ts
success('Tavily 同步能力暂未启用')
```

**Step 2: 保留按钮与刷新逻辑**

```ts
await searchPoolApi.syncUsage(currentService.value, true)
await loadWorkspace()
```

**Step 3: 运行前端页面测试**

Run: `cd frontend && npm run test:run -- src/views/admin/__tests__/search-pool-pages.spec.ts`

Expected: PASS

**Step 4: 提交前端改动**

```bash
git add frontend/src/views/admin/SearchPoolServiceWorkspace.vue frontend/src/views/admin/__tests__/search-pool-pages.spec.ts
git commit -m "feat(frontend): show tavily sync placeholder status"
```

### Task 4: 清理配置说明

**Files:**
- Modify: `.env.example`
- Test: `uv run pytest tests/modules/search_pool_gateway -q`

**Step 1: 删除 Tavily Cookie 同步环境变量注释**

```env
# 删除:
# SEARCH_POOL_TAVILY_SYNC_COOKIE=...
# SEARCH_POOL_TAVILY_SYNC_USER_AGENT=...
# SEARCH_POOL_TAVILY_SYNC_REFERER=...
```

**Step 2: 如有需要，补一句占位说明**

```env
# Tavily 额度同步能力暂未启用
```

**Step 3: 运行模块级测试**

Run: `uv run pytest tests/modules/search_pool_gateway -q`

Expected: PASS

**Step 4: 提交配置改动**

```bash
git add .env.example
git commit -m "docs: remove tavily cookie sync env vars"
```

### Task 5: 全量回归与文档收口

**Files:**
- Modify: `docs/plans/2026-03-19-search-pool-disable-tavily-cookie-sync-design.md`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Test: `tests/modules/search_pool_gateway`
- Test: `frontend/src/features/search-pool/__tests__/api.spec.ts`
- Test: `frontend/src/features/search-pool/__tests__/workspace.spec.ts`
- Test: `frontend/src/views/admin/__tests__/search-pool-pages.spec.ts`

**Step 1: 跑后端专项测试**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: PASS

**Step 2: 跑后端模块测试**

Run: `uv run pytest tests/modules/search_pool_gateway -q`

Expected: PASS

**Step 3: 跑前端相关测试**

Run: `cd frontend && npm run test:run -- src/features/search-pool/__tests__/api.spec.ts src/features/search-pool/__tests__/workspace.spec.ts src/views/admin/__tests__/search-pool-pages.spec.ts`

Expected: PASS

**Step 4: 检查变更文件并提交**

```bash
git add docs/plans/2026-03-19-search-pool-disable-tavily-cookie-sync-design.md docs/plans/2026-03-19-search-pool-disable-tavily-cookie-sync.md
git commit -m "docs: record tavily cookie sync rollback plan"
```
