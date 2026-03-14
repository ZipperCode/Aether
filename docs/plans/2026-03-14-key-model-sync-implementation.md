# Key Model Sync Button Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为每个 Provider Key 增加“同步模型”按钮，强制从上游获取模型并覆盖该 Key 的 `allowed_models`，不改动 `auto_fetch_models`。

**Architecture:** 后端新增 Key 级同步服务与管理端路由，复用上游模型抓取逻辑，写回 `allowed_models` 并触发缓存失效与模型关联刷新。前端新增管理端 API 调用与 Key 行按钮，触发同步并刷新列表。

**Tech Stack:** FastAPI + SQLAlchemy + Pydantic，Vue 3 + Vite + TypeScript，pytest。

---

### Task 1: 后端 Key 同步服务（含单测）

**Files:**
- Create: `src/services/provider_keys/key_model_sync_service.py`
- Modify: `src/services/provider_keys/__init__.py`
- Test: `tests/services/test_provider_key_model_sync_service.py`

**Step 1: Write the failing test**

```python
import types
from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.services.provider_keys import key_model_sync_service as sync_module


class _FakeQuery:
    def __init__(self, value):
        self._value = value

    def options(self, *args, **kwargs):
        return self

    def filter(self, *args, **kwargs):
        return self

    def first(self):
        return self._value


class _FakeDB:
    def __init__(self, key):
        self._key = key
        self.commit_calls = 0

    def query(self, model):
        return _FakeQuery(self._key)

    def commit(self):
        self.commit_calls += 1


@pytest.mark.asyncio
async def test_sync_key_models_overwrites_allowed_models(monkeypatch):
    provider = SimpleNamespace(
        id="provider-1",
        name="Provider",
        provider_type="custom",
        endpoints=[SimpleNamespace(api_format="openai:chat", base_url="https://api", is_active=True)],
        proxy=None,
    )
    key = SimpleNamespace(
        id="key-1",
        provider_id="provider-1",
        provider=provider,
        is_active=True,
        auth_type="api_key",
        api_key="ENC_KEY",
        auth_config=None,
        proxy=None,
        allowed_models=["old"],
        locked_models=["locked"],
        model_include_patterns=["gpt-*"],
        model_exclude_patterns=["*-preview"],
        last_models_fetch_at=None,
        last_models_fetch_error="err",
        upstream_metadata=None,
    )
    db = _FakeDB(key)

    monkeypatch.setattr(sync_module.crypto_service, "decrypt", lambda v: "sk-test")
    monkeypatch.setattr(sync_module, "fetch_models_for_key", AsyncMock(return_value=(
        [{"id": "gpt-1"}, {"id": "gpt-2"}], [], True, None
    )))
    monkeypatch.setattr(sync_module, "set_upstream_models_to_cache", AsyncMock())
    monkeypatch.setattr(sync_module, "on_key_allowed_models_changed", AsyncMock())

    result = await sync_module.sync_key_models(db, "key-1")

    assert result["success"] is True
    assert result["models_count"] == 2
    assert key.allowed_models == ["gpt-1", "gpt-2"]
    assert key.last_models_fetch_error is None
    assert key.last_models_fetch_at is not None


@pytest.mark.asyncio
async def test_sync_key_models_keeps_allowed_models_on_failure(monkeypatch):
    provider = SimpleNamespace(
        id="provider-1",
        name="Provider",
        provider_type="custom",
        endpoints=[SimpleNamespace(api_format="openai:chat", base_url="https://api", is_active=True)],
        proxy=None,
    )
    key = SimpleNamespace(
        id="key-1",
        provider_id="provider-1",
        provider=provider,
        is_active=True,
        auth_type="api_key",
        api_key="ENC_KEY",
        auth_config=None,
        proxy=None,
        allowed_models=["keep"],
        last_models_fetch_at=None,
        last_models_fetch_error=None,
        upstream_metadata=None,
    )
    db = _FakeDB(key)

    monkeypatch.setattr(sync_module.crypto_service, "decrypt", lambda v: "sk-test")
    monkeypatch.setattr(sync_module, "fetch_models_for_key", AsyncMock(return_value=([], ["boom"], False, None)))

    result = await sync_module.sync_key_models(db, "key-1")

    assert result["success"] is False
    assert key.allowed_models == ["keep"]
    assert key.last_models_fetch_error
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/services/test_provider_key_model_sync_service.py -v`
Expected: FAIL with `ModuleNotFoundError` or missing function.

**Step 3: Write minimal implementation**

```python
from datetime import datetime, timezone
from typing import Any

from sqlalchemy.orm import Session, joinedload

from src.core.crypto import crypto_service
from src.core.exceptions import NotFoundException
from src.core.provider_types import ProviderType
from src.models.database import ProviderAPIKey
from src.services.model.fetch_scheduler import (
    MODEL_FETCH_HTTP_TIMEOUT,
    set_upstream_models_to_cache,
)
from src.services.model.upstream_fetcher import (
    UpstreamModelsFetchContext,
    build_format_to_config,
    fetch_models_for_key,
)
from src.services.model.upstream_fetcher import UpstreamModelsFetcherRegistry
from src.services.provider.envelope import ensure_providers_bootstrapped
from src.services.provider.oauth_token import resolve_oauth_access_token
from src.services.proxy_node.resolver import resolve_effective_proxy
from src.services.model.upstream_fetcher import merge_upstream_metadata
from src.services.model.global_model import on_key_allowed_models_changed


def _aggregate_models_for_cache(models: list[dict]) -> list[dict]:
    model_map: dict[str, dict] = {}
    for model in models:
        model_id = model.get("id")
        if not model_id:
            continue
        api_format = model.get("api_format", "")
        existing_formats = model.get("api_formats") or []
        if model_id not in model_map:
            aggregated = {"id": model_id, "api_formats": []}
            for key, value in model.items():
                if key not in ("id", "api_format", "api_formats"):
                    aggregated[key] = value
            model_map[model_id] = aggregated
        if api_format and api_format not in model_map[model_id]["api_formats"]:
            model_map[model_id]["api_formats"].append(api_format)
        for fmt in existing_formats:
            if fmt and fmt not in model_map[model_id]["api_formats"]:
                model_map[model_id]["api_formats"].append(fmt)
    result = list(model_map.values())
    for model in result:
        model["api_formats"].sort()
    result.sort(key=lambda m: m["id"])
    return result


async def sync_key_models(db: Session, key_id: str) -> dict[str, Any]:
    key = (
        db.query(ProviderAPIKey)
        .options(joinedload(ProviderAPIKey.provider).joinedload("endpoints"))
        .filter(ProviderAPIKey.id == key_id)
        .first()
    )
    if not key:
        raise NotFoundException("Key not found")

    provider = key.provider
    if not provider:
        return {"success": False, "models_count": 0, "error": "Provider not found"}

    ensure_providers_bootstrapped()
    format_to_endpoint = build_format_to_config(provider.endpoints)
    has_custom_fetcher = UpstreamModelsFetcherRegistry.get(str(provider.provider_type or "")) is not None
    if not format_to_endpoint and not has_custom_fetcher:
        return {"success": False, "models_count": 0, "error": "No active endpoints"}

    if not key.api_key:
        return {"success": False, "models_count": 0, "error": "No API key configured"}

    auth_type = str(getattr(key, "auth_type", "api_key") or "api_key").lower()
    auth_config = None
    api_key_value = ""
    if auth_type == "oauth":
        endpoint_api_format = "gemini:chat" if str(provider.provider_type or "").lower() == ProviderType.ANTIGRAVITY else None
        resolved = await resolve_oauth_access_token(
            key_id=str(key.id),
            encrypted_api_key=str(key.api_key),
            encrypted_auth_config=str(key.auth_config) if key.auth_config else None,
            provider_proxy_config=resolve_effective_proxy(getattr(provider, "proxy", None), getattr(key, "proxy", None)),
            endpoint_api_format=endpoint_api_format,
        )
        api_key_value = resolved.access_token
        auth_config = resolved.decrypted_auth_config
    else:
        api_key_value = crypto_service.decrypt(key.api_key)
        if key.auth_config:
            try:
                auth_config = crypto_service.decrypt(key.auth_config)
            except Exception:
                auth_config = None

    fetch_ctx = UpstreamModelsFetchContext(
        provider_type=str(provider.provider_type or ""),
        api_key_value=str(api_key_value or ""),
        format_to_endpoint=format_to_endpoint,
        proxy_config=resolve_effective_proxy(getattr(provider, "proxy", None), getattr(key, "proxy", None)),
        auth_config=auth_config,
    )
    all_models, errors, has_success, upstream_metadata = await fetch_models_for_key(
        fetch_ctx, timeout_seconds=MODEL_FETCH_HTTP_TIMEOUT
    )

    key.last_models_fetch_at = datetime.now(timezone.utc)
    if not has_success:
        key.last_models_fetch_error = "; ".join(errors) if errors else "All endpoints failed"
        db.commit()
        return {"success": False, "models_count": 0, "error": key.last_models_fetch_error}

    key.last_models_fetch_error = None
    if upstream_metadata and isinstance(upstream_metadata, dict):
        key.upstream_metadata = merge_upstream_metadata(key.upstream_metadata, upstream_metadata)

    model_ids = sorted({m.get("id") for m in all_models if m.get("id")})
    key.allowed_models = list(model_ids)

    unique_models = _aggregate_models_for_cache(all_models)
    if unique_models:
        await set_upstream_models_to_cache(provider.id, key.id, unique_models)

    db.commit()
    await on_key_allowed_models_changed(db=db, provider_id=provider.id, allowed_models=list(key.allowed_models or []))

    return {"success": True, "models_count": len(model_ids)}
```

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/services/test_provider_key_model_sync_service.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/services/provider_keys/key_model_sync_service.py src/services/provider_keys/__init__.py tests/services/test_provider_key_model_sync_service.py
git commit -m "feat: add provider key model sync service"
```

---

### Task 2: 管理端路由与适配器（含单测）

**Files:**
- Modify: `src/api/admin/endpoints/keys.py`
- Test: `tests/api/test_admin_provider_key_sync_models.py`

**Step 1: Write the failing test**

```python
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.api.admin.endpoints.keys import AdminSyncKeyModelsAdapter


@pytest.mark.asyncio
async def test_sync_key_models_adapter_calls_service(monkeypatch):
    db = MagicMock()
    result_payload = {"success": True, "models_count": 2}
    monkeypatch.setattr(
        "src.api.admin.endpoints.keys.sync_key_models",
        AsyncMock(return_value=result_payload),
    )
    context = SimpleNamespace(db=db, request=SimpleNamespace(state=SimpleNamespace()))

    adapter = AdminSyncKeyModelsAdapter(key_id="key-1")
    result = await adapter.handle(context)

    assert result == result_payload
```

**Step 2: Run test to verify it fails**

Run: `uv run pytest tests/api/test_admin_provider_key_sync_models.py -v`
Expected: FAIL with adapter missing.

**Step 3: Write minimal implementation**

```python
from pydantic import BaseModel

class SyncKeyModelsResponse(BaseModel):
    success: bool
    models_count: int
    error: str | None = None


@router.post("/keys/{key_id}/sync-models", response_model=SyncKeyModelsResponse)
async def sync_key_models_route(
    key_id: str,
    request: Request,
    db: Session = Depends(get_db),
) -> SyncKeyModelsResponse:
    adapter = AdminSyncKeyModelsAdapter(key_id=key_id)
    return await pipeline.run(adapter=adapter, http_request=request, db=db, mode=adapter.mode)


@dataclass
class AdminSyncKeyModelsAdapter(AdminApiAdapter):
    key_id: str

    async def handle(self, context: ApiRequestContext) -> Any:  # type: ignore[override]
        return await sync_key_models(context.db, self.key_id)
```

**Step 4: Run test to verify it passes**

Run: `uv run pytest tests/api/test_admin_provider_key_sync_models.py -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/api/admin/endpoints/keys.py tests/api/test_admin_provider_key_sync_models.py
git commit -m "feat: add admin endpoint for key model sync"
```

---

### Task 3: 前端 API + Key 行按钮

**Files:**
- Modify: `frontend/src/api/admin.ts`
- Modify: `frontend/src/features/providers/components/ProviderDetailDrawer.vue`

**Step 1: Write the failing test**

如果现有前端测试体系未覆盖此区域，可跳过前端单测，仅做手动验证；但需运行 `npm run lint`。

**Step 2: Write minimal implementation**

```ts
export interface ProviderKeySyncModelsResponse {
  success: boolean
  models_count: number
  error?: string
}

async syncProviderKeyModels(keyId: string): Promise<ProviderKeySyncModelsResponse> {
  const response = await apiClient.post<ProviderKeySyncModelsResponse>(
    `/api/admin/endpoints/keys/${keyId}/sync-models`
  )
  return response.data
}
```

```ts
const syncingKeyId = ref<string | null>(null)

async function handleSyncKeyModels(key: EndpointAPIKey) {
  if (syncingKeyId.value) return
  const confirmed = await confirm({
    title: '同步上游模型',
    message: '将从上游获取模型并覆盖当前模型权限，是否继续？',
    confirmText: '确认同步',
  })
  if (!confirmed) return

  syncingKeyId.value = key.id
  try {
    const result = await syncProviderKeyModels(key.id)
    if (result.success) {
      showSuccess(`已同步 ${result.models_count} 个模型`)
    } else {
      showError(result.error || '同步失败', '错误')
    }
    await handleKeyChanged()
  } catch (err: unknown) {
    showError(parseApiError(err, '同步失败'), '错误')
  } finally {
    syncingKeyId.value = null
  }
}
```

```vue
<Button
  variant="ghost"
  size="icon"
  class="h-7 w-7"
  :disabled="syncingKeyId === key.id"
  title="同步模型"
  @click="handleSyncKeyModels(key)"
>
  <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': syncingKeyId === key.id }" />
</Button>
```

**Step 3: Run lint/test**

Run: `cd frontend && npm run lint`
Expected: PASS

**Step 4: Commit**

```bash
git add frontend/src/api/admin.ts frontend/src/features/providers/components/ProviderDetailDrawer.vue
git commit -m "feat: add per-key model sync action"
```

---

### Task 4: 回归测试与文档整理

**Files:**
- Modify: `docs/plans/2026-03-14-key-model-sync-implementation.md`

**Step 1: Run backend tests**

Run: `uv run pytest tests/services/test_provider_key_model_sync_service.py tests/api/test_admin_provider_key_sync_models.py -v`
Expected: PASS

**Step 2: Update plan with actual results**

在本计划末尾追加“执行记录”，记录运行的命令与结果摘要。

**Step 3: Commit**

```bash
git add docs/plans/2026-03-14-key-model-sync-implementation.md
git commit -m "docs: record key model sync implementation results"
```

---

## 执行记录

- `uv run pytest tests/services/test_provider_key_model_sync_service.py -v`
  - 结果：PASS
- `uv run pytest tests/api/test_admin_provider_key_sync_models.py -v`
  - 结果：PASS
- `cd frontend && npm run lint`
  - 结果：2 warnings（`AlertDialog.vue` 的 `v-html`，`ModelTestDialog.vue` 的事件命名），无 error
- `uv run pytest tests/services/test_provider_key_model_sync_service.py tests/api/test_admin_provider_key_sync_models.py -v`
  - 结果：PASS
