from __future__ import annotations

from types import SimpleNamespace
from unittest.mock import AsyncMock

import pytest

from src.services.provider_keys import key_model_sync_service as sync_module


class _FakeQuery:
    def __init__(self, value):
        self._value = value

    def options(self, *args, **kwargs):
        _ = args, kwargs
        return self

    def filter(self, *args, **kwargs):
        _ = args, kwargs
        return self

    def first(self):
        return self._value


class _FakeDB:
    def __init__(self, key):
        self._key = key
        self.commit_calls = 0

    def query(self, model):
        _ = model
        return _FakeQuery(self._key)

    def commit(self):
        self.commit_calls += 1


@pytest.mark.asyncio
async def test_sync_key_models_overwrites_allowed_models(monkeypatch: pytest.MonkeyPatch) -> None:
    provider = SimpleNamespace(
        id="provider-1",
        name="Provider",
        provider_type="custom",
        endpoints=[
            SimpleNamespace(
                api_format="openai:chat",
                base_url="https://api.example.com",
                is_active=True,
                header_rules=None,
            )
        ],
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

    monkeypatch.setattr(sync_module, "ensure_providers_bootstrapped", lambda: None)
    monkeypatch.setattr(sync_module.crypto_service, "decrypt", lambda v: "sk-test")
    monkeypatch.setattr(sync_module, "resolve_effective_proxy", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        sync_module,
        "fetch_models_for_key",
        AsyncMock(
            return_value=(
                [{"id": "gpt-2"}, {"id": "gpt-1"}, {"id": "gpt-1"}],
                [],
                True,
                None,
            )
        ),
    )
    monkeypatch.setattr(sync_module, "set_upstream_models_to_cache", AsyncMock())
    monkeypatch.setattr(sync_module, "on_key_allowed_models_changed", AsyncMock())

    result = await sync_module.sync_key_models(db, "key-1")

    assert result == {"success": True, "models_count": 2}
    assert key.allowed_models == ["gpt-1", "gpt-2"]
    assert key.last_models_fetch_error is None
    assert key.last_models_fetch_at is not None


@pytest.mark.asyncio
async def test_sync_key_models_keeps_allowed_models_on_failure(monkeypatch: pytest.MonkeyPatch) -> None:
    provider = SimpleNamespace(
        id="provider-1",
        name="Provider",
        provider_type="custom",
        endpoints=[
            SimpleNamespace(
                api_format="openai:chat",
                base_url="https://api.example.com",
                is_active=True,
                header_rules=None,
            )
        ],
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
        locked_models=None,
        model_include_patterns=None,
        model_exclude_patterns=None,
        last_models_fetch_at=None,
        last_models_fetch_error=None,
        upstream_metadata=None,
    )
    db = _FakeDB(key)

    monkeypatch.setattr(sync_module, "ensure_providers_bootstrapped", lambda: None)
    monkeypatch.setattr(sync_module.crypto_service, "decrypt", lambda v: "sk-test")
    monkeypatch.setattr(sync_module, "resolve_effective_proxy", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        sync_module,
        "fetch_models_for_key",
        AsyncMock(return_value=([], ["boom"], False, None)),
    )

    result = await sync_module.sync_key_models(db, "key-1")

    assert result["success"] is False
    assert result["models_count"] == 0
    assert key.allowed_models == ["keep"]
    assert key.last_models_fetch_error is not None
    assert key.last_models_fetch_at is not None
