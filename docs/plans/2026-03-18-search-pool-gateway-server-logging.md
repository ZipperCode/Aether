# Search Pool Gateway Server Logging Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为搜索池网关代理请求增加可排障的服务端结构化日志，同时保留现有 usage 聚合逻辑。

**Architecture:** 在 `src/modules/search_pool_gateway/services/proxy_service.py` 内增加小型日志辅助函数，统一生成 `request_id`、执行敏感值脱敏，并在鉴权、选 key、转发开始、转发结束、异常退出几个阶段写日志。测试通过 monkeypatch `logger` 验证日志行为，不引入新存储层。

**Tech Stack:** Python, FastAPI, httpx, loguru, pytest

---

### Task 1: 固化代理日志测试边界

**Files:**
- Modify: `tests/modules/search_pool_gateway/test_proxy_routes.py`
- Reference: `src/modules/search_pool_gateway/services/proxy_service.py`

**Step 1: Write the failing test**

新增这些测试：

```python
def test_proxy_tavily_route_logs_masked_request_lifecycle(...):
    ...
    assert info_calls[0]["stage"] == "key_selected"
    assert info_calls[1]["stage"] == "upstream_request_started"
    assert info_calls[2]["stage"] == "upstream_response_received"
    assert "real-gateway-token" not in rendered_messages
    assert "tvly-secret-123456" not in rendered_messages
```

```python
def test_proxy_tavily_route_logs_auth_failed(...):
    ...
    assert warning_calls[0]["stage"] == "auth_failed"
```

```python
def test_proxy_tavily_route_logs_key_unavailable(...):
    ...
    assert warning_calls[0]["stage"] == "key_unavailable"
```

```python
def test_proxy_tavily_route_logs_upstream_failure(...):
    ...
    assert error_calls[0]["stage"] == "upstream_request_failed"
```

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: FAIL，因为当前没有这些日志行为。

**Step 3: Write minimal implementation**

先不要实现；等测试红灯确认后再改生产代码。

**Step 4: Run test to verify it fails for the right reason**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: FAIL，且失败点落在日志断言而不是测试拼写错误。

**Step 5: Commit**

```bash
git add tests/modules/search_pool_gateway/test_proxy_routes.py
git commit -m "test(search-pool): cover proxy server logging"
```

### Task 2: 实现 proxy_service 结构化日志

**Files:**
- Modify: `src/modules/search_pool_gateway/services/proxy_service.py`
- Reference: `src/core/logger.py`

**Step 1: Write the failing test**

沿用 Task 1 的失败测试，不再新增。

**Step 2: Run test to verify it fails**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: FAIL。

**Step 3: Write minimal implementation**

实现这些内容：

```python
def _mask_secret(value: str | None) -> str:
    ...


def _log_proxy_event(level: str, *, service: str, request_id: str, stage: str, ...):
    ...
```

在 `proxy_tavily` / `proxy_firecrawl` / `_authenticate_gateway_token` 里补日志：
- token 缺失/无效时 `auth_failed`
- 无可用 key 时 `key_unavailable`
- 选 key后 `key_selected`
- 转发前 `upstream_request_started`
- 收到上游响应后 `upstream_response_received`
- 异常时 `upstream_request_failed`

记录耗时并脱敏 token/key。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/services/proxy_service.py tests/modules/search_pool_gateway/test_proxy_routes.py
git commit -m "feat(search-pool): add proxy server logging"
```

### Task 3: 回归验证并确认没有破坏现有行为

**Files:**
- Test: `tests/modules/search_pool_gateway/test_proxy_routes.py`
- Test: `tests/modules/search_pool_gateway/test_admin_routes_workspace.py`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`

**Step 1: Write the failing test**

不新增测试，执行现有回归集即可。

**Step 2: Run test to verify current state**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: 全部通过。

**Step 3: Write minimal implementation**

无需新增实现，仅在失败时修正兼容性问题。

**Step 4: Run test to verify it passes**

Run: `pytest tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`
Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/services/proxy_service.py tests/modules/search_pool_gateway/test_proxy_routes.py docs/plans/2026-03-18-search-pool-gateway-server-logging-design.md docs/plans/2026-03-18-search-pool-gateway-server-logging.md
git commit -m "feat(search-pool): add proxy request diagnostics"
```
