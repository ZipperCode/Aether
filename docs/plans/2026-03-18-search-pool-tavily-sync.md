# Search Pool Tavily Sync Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 让搜索池 Tavily 服务的“同步额度”真正请求 Tavily 控制台接口并回写本地 key 使用量。

**Architecture:** 在 `GatewayUsageService.sync()` 中为 Tavily 分支新增一个控制台客户端，使用环境变量中的 Cookie 请求 `https://app.tavily.com/api/keys`，将返回的 key 使用数据按明文 key 精确匹配到本地加密 key，再回写 `usage_key_*` 字段和同步状态，并输出结构化同步日志。

**Tech Stack:** Python, FastAPI, SQLAlchemy, httpx, loguru, pytest

---

### Task 1: 固化 Tavily 同步测试边界

**Files:**
- Modify: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Reference: `src/modules/search_pool_gateway/services/usage_service.py`
- Reference: `.env.example`

**Step 1: Write the failing test**

新增这些测试：

```python
def test_sync_tavily_updates_usage_fields_from_console_api(...):
    monkeypatch.setenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", "appSession=test-cookie")
    ...
    assert key["usage_key_limit"] == 2147483647
    assert key["usage_key_used"] == 18
    assert key["usage_key_remaining"] == 2147483629
    assert key["usage_account_plan"] == "development"
```

```python
def test_sync_tavily_reports_missing_cookie(...):
    ...
    assert result["errors"] == 1
    assert "cookie" in result["message"].lower()
```

```python
def test_sync_tavily_logs_sync_lifecycle(...):
    ...
    assert "stage=sync_started" in info_calls[0]
    assert "stage=sync_completed" in info_calls[-1]
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: FAIL，因为当前 `sync()` 还是占位逻辑。

**Step 3: Write minimal implementation**

先不写生产代码，先确认失败断言正确。

**Step 4: Run test to verify it fails for the right reason**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: FAIL，失败点落在 Tavily 同步结果断言上。

**Step 5: Commit**

```bash
git add tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "test(search-pool): cover tavily usage sync"
```

### Task 2: 实现 Tavily 控制台同步与日志

**Files:**
- Modify: `src/modules/search_pool_gateway/services/usage_service.py`
- Modify: `.env.example`

**Step 1: Write the failing test**

沿用 Task 1 的红灯测试。

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: FAIL。

**Step 3: Write minimal implementation**

实现这些内容：

```python
def _sync_tavily_usage(...):
    ...


def _fetch_tavily_console_keys(...):
    ...


def _log_sync_event(...):
    ...
```

行为要求：
- 仅读 Tavily 分支使用 `SEARCH_POOL_TAVILY_SYNC_COOKIE`
- 使用 `httpx` 请求 `https://app.tavily.com/api/keys`
- 解密本地 key 并精确匹配
- 回写 `usage_key_limit` / `usage_key_used` / `usage_key_remaining` / `usage_account_plan`
- 输出同步开始、接口成功/失败、匹配统计、同步结束日志
- `.env.example` 增加 Tavily 同步环境变量说明

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/services/usage_service.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py .env.example
git commit -m "feat(search-pool): sync tavily usage from console"
```

### Task 3: 跑搜索池回归集

**Files:**
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Test: `tests/modules/search_pool_gateway/test_admin_routes_workspace.py`
- Test: `tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py`
- Test: `tests/modules/search_pool_gateway/test_proxy_routes.py`

**Step 1: Write the failing test**

不新增测试。

**Step 2: Run test to verify current state**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: 全部通过。

**Step 3: Write minimal implementation**

仅在回归失败时修正兼容性。

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/modules/search_pool_gateway/test_usage_sync_and_stats.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/services/usage_service.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py .env.example docs/plans/2026-03-18-search-pool-tavily-sync-design.md docs/plans/2026-03-18-search-pool-tavily-sync.md
git commit -m "feat(search-pool): add tavily usage sync"
```
