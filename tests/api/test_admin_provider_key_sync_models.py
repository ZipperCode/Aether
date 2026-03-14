from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.api.admin.endpoints.keys import AdminSyncKeyModelsAdapter


@pytest.mark.asyncio
async def test_sync_key_models_adapter_calls_service(monkeypatch: pytest.MonkeyPatch) -> None:
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
