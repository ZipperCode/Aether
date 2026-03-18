# Search Pool Remove Key Encryption Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 移除搜索池模块中对上游 API Key 的加密存储，改为在独立 SQLite 中直接存放明文字段。

**Architecture:** 将 `search_pool_gateway` 模型中的 key 字段改为明文语义，服务层和代理层直接读取该字段，不再经过加解密逻辑。由于当前处于开发阶段，不兼容旧 SQLite 数据，不做迁移逻辑，改动后通过删除旧库文件重建 schema。

**Tech Stack:** Python, FastAPI, SQLAlchemy, SQLite, pytest

---

### Task 1: 固化搜索池明文存储测试边界

**Files:**
- Modify: `tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py`
- Modify: `tests/modules/search_pool_gateway/test_proxy_routes.py`
- Modify: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Reference: `src/modules/search_pool_gateway/services/key_service.py`
- Reference: `src/modules/search_pool_gateway/services/proxy_service.py`
- Reference: `src/modules/search_pool_gateway/services/usage_service.py`

**Step 1: Write the failing test**

新增或修改测试，覆盖这些行为：

```python
def test_admin_create_key_persists_plaintext_key(...):
    ...
    row = ...
    assert row.raw_key == "tvly-dev-xxx"
```

```python
def test_proxy_route_uses_plaintext_key_without_decrypt(...):
    ...
    assert forwarded_headers["Authorization"] == "Bearer tvly-dev-xxx"
```

```python
def test_usage_sync_matches_tavily_key_with_plaintext_storage(...):
    ...
    assert payload["result"]["synced_keys"] == 1
```

同时移除测试里对 `SEARCH_POOL_GATEWAY_CRYPTO_KEY` 的依赖。

**Step 2: Run test to verify it fails**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: FAIL，因为当前代码仍使用 `key_encrypted` 和加解密逻辑。

**Step 3: Write minimal implementation**

先不写生产代码，只确认失败断言落在字段名和读取路径上。

**Step 4: Run test to verify it fails for the right reason**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: FAIL，失败点聚焦在明文存储语义缺失。

**Step 5: Commit**

```bash
git add tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "test(search-pool): cover plaintext key storage"
```

### Task 2: 改造模型与服务层为明文 key

**Files:**
- Modify: `src/modules/search_pool_gateway/models.py`
- Modify: `src/modules/search_pool_gateway/repositories/key_repo.py`
- Modify: `src/modules/search_pool_gateway/services/key_service.py`
- Modify: `src/modules/search_pool_gateway/services/proxy_service.py`
- Modify: `src/modules/search_pool_gateway/services/usage_service.py`
- Delete: `src/modules/search_pool_gateway/services/crypto.py`
- Modify: `.env.example`

**Step 1: Write the failing test**

沿用 Task 1 红灯测试。

**Step 2: Run test to verify it fails**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: FAIL。

**Step 3: Write minimal implementation**

实现这些改动：

```python
class GatewayApiKey(SearchPoolGatewayBase):
    raw_key: Mapped[str] = mapped_column(Text, nullable=False)
```

```python
def create_key(...):
    return self.repo.create(
        service=service_norm,
        raw_key=normalized_key,
        key_masked=mask_key(normalized_key),
        email=email.strip(),
    )
```

```python
raw_key = leased.raw_key
```

```python
decrypted_map[row.raw_key] = row
```

并删除搜索池模块里的 `GatewayCryptoService` 依赖与 `SEARCH_POOL_GATEWAY_CRYPTO_KEY` 配置说明。

**Step 4: Run test to verify it passes**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway/models.py src/modules/search_pool_gateway/repositories/key_repo.py src/modules/search_pool_gateway/services/key_service.py src/modules/search_pool_gateway/services/proxy_service.py src/modules/search_pool_gateway/services/usage_service.py src/modules/search_pool_gateway/services/crypto.py .env.example tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py
git commit -m "refactor(search-pool): store gateway keys in plaintext"
```

### Task 3: 跑搜索池回归并确认开发环境操作约束

**Files:**
- Test: `tests/modules/search_pool_gateway/test_admin_routes_workspace.py`
- Test: `tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py`
- Test: `tests/modules/search_pool_gateway/test_proxy_routes.py`
- Test: `tests/modules/search_pool_gateway/test_usage_sync_and_stats.py`
- Modify: `docs/plans/2026-03-18-search-pool-remove-key-encryption-design.md`
- Modify: `docs/plans/2026-03-18-search-pool-remove-key-encryption.md`

**Step 1: Write the failing test**

不新增测试。

**Step 2: Run test to verify current state**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: 全部通过。

**Step 3: Write minimal implementation**

如果回归失败，仅修复与明文 key 改造直接相关的问题。并在设计/计划文档中明确：
- 当前不兼容旧 SQLite 数据
- 开发环境需要删除旧搜索池 SQLite 文件后再启动

**Step 4: Run test to verify it passes**

Run:
`uv run pytest tests/modules/search_pool_gateway/test_admin_routes_workspace.py tests/modules/search_pool_gateway/test_admin_routes_keys_tokens.py tests/modules/search_pool_gateway/test_proxy_routes.py tests/modules/search_pool_gateway/test_usage_sync_and_stats.py -q`

Expected: PASS。

**Step 5: Commit**

```bash
git add src/modules/search_pool_gateway tests/modules/search_pool_gateway .env.example docs/plans/2026-03-18-search-pool-remove-key-encryption-design.md docs/plans/2026-03-18-search-pool-remove-key-encryption.md
git commit -m "refactor(search-pool): remove key encryption"
```
