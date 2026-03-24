from __future__ import annotations

import json
import sys
import types
from types import SimpleNamespace

import pytest

from src.core.vertex_auth import VertexAuthService
from src.services.provider.auth import get_provider_auth


class _FakeQuery:
    def __init__(self, row: object | None) -> None:
        self._row = row

    def filter(self, *_args: object, **_kwargs: object) -> "_FakeQuery":
        return self

    def first(self) -> object | None:
        return self._row


class _FakeSessionCtx:
    def __init__(self, row: object | None) -> None:
        self._row = row

    def __enter__(self) -> "_FakeSessionCtx":
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> bool:
        _ = exc_type, exc, tb
        return False

    def query(self, _model: object) -> _FakeQuery:
        return _FakeQuery(self._row)


@pytest.mark.asyncio
async def test_get_provider_auth_vertex_service_account_uses_provider_proxy(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sa_json = {
        "client_email": "svc@example.iam.gserviceaccount.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nTEST\n-----END PRIVATE KEY-----\n",
        "project_id": "demo-project",
    }
    provider_proxy = {"node_id": "provider-node", "enabled": True}
    provider = SimpleNamespace(proxy=provider_proxy)
    endpoint = SimpleNamespace(provider=provider)
    key = SimpleNamespace(
        auth_type="service_account",
        auth_config="enc_cfg",
        api_key="enc_key",
        provider=provider,
        proxy=None,
    )

    captured: dict[str, object] = {}

    monkeypatch.setattr(
        "src.core.crypto.crypto_service.decrypt",
        lambda value: json.dumps(sa_json) if value == "enc_cfg" else "",
    )

    def _fake_build_proxy_client_kwargs(
        proxy_config: dict[str, object] | None = None,
        *,
        timeout: float = 30.0,
        **_: object,
    ) -> dict[str, object]:
        captured["proxy_config"] = proxy_config
        captured["timeout"] = timeout
        return {"timeout": timeout}

    async def _fake_get_access_token(
        self: VertexAuthService,
        *,
        httpx_client_kwargs: dict[str, object] | None = None,
    ) -> str:
        captured["httpx_client_kwargs"] = httpx_client_kwargs
        return "ya29.test-token"

    monkeypatch.setattr(
        "src.services.proxy_node.resolver.build_proxy_client_kwargs",
        _fake_build_proxy_client_kwargs,
    )
    monkeypatch.setattr(VertexAuthService, "get_access_token", _fake_get_access_token)

    auth = await get_provider_auth(endpoint, key)  # type: ignore[arg-type]

    assert auth is not None
    assert captured["proxy_config"] == provider_proxy
    assert captured["timeout"] == 30
    assert captured["httpx_client_kwargs"] == {"timeout": 30}
    assert auth.auth_header == "Authorization"
    assert auth.auth_value == "Bearer ya29.test-token"
    assert auth.decrypted_auth_config == sa_json


@pytest.mark.asyncio
async def test_get_provider_auth_vertex_service_account_prefers_key_proxy(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sa_json = {
        "client_email": "svc@example.iam.gserviceaccount.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nTEST\n-----END PRIVATE KEY-----\n",
        "project_id": "demo-project",
    }
    provider = SimpleNamespace(proxy={"node_id": "provider-node", "enabled": True})
    endpoint = SimpleNamespace(provider=provider)
    key_proxy = {"node_id": "key-node", "enabled": True}
    key = SimpleNamespace(
        auth_type="service_account",
        auth_config="enc_cfg",
        api_key="enc_key",
        provider=provider,
        proxy=key_proxy,
    )

    captured: dict[str, object] = {}

    monkeypatch.setattr(
        "src.core.crypto.crypto_service.decrypt",
        lambda value: json.dumps(sa_json) if value == "enc_cfg" else "",
    )

    def _fake_build_proxy_client_kwargs(
        proxy_config: dict[str, object] | None = None,
        *,
        timeout: float = 30.0,
        **_: object,
    ) -> dict[str, object]:
        captured["proxy_config"] = proxy_config
        return {"timeout": timeout}

    async def _fake_get_access_token(
        self: VertexAuthService,
        *,
        httpx_client_kwargs: dict[str, object] | None = None,
    ) -> str:
        return "ya29.test-token"

    monkeypatch.setattr(
        "src.services.proxy_node.resolver.build_proxy_client_kwargs",
        _fake_build_proxy_client_kwargs,
    )
    monkeypatch.setattr(VertexAuthService, "get_access_token", _fake_get_access_token)

    auth = await get_provider_auth(endpoint, key)  # type: ignore[arg-type]

    assert auth is not None
    assert captured["proxy_config"] == key_proxy


@pytest.mark.asyncio
async def test_get_provider_auth_vertex_service_account_uses_provider_id_lookup_without_touching_endpoint_provider(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sa_json = {
        "client_email": "svc@example.iam.gserviceaccount.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nTEST\n-----END PRIVATE KEY-----\n",
        "project_id": "demo-project",
    }
    provider_proxy = {"node_id": "provider-node", "enabled": True}

    class _DetachedEndpoint:
        provider_id = "provider-1"

        @property
        def provider(self) -> object:
            raise RuntimeError("detached endpoint provider should not be lazy-loaded")

    fake_provider = SimpleNamespace(
        id="provider-1", provider_type="vertex_ai", proxy=provider_proxy
    )
    fake_database = types.ModuleType("src.database")
    fake_database.create_session = lambda: _FakeSessionCtx(fake_provider)
    fake_models = types.ModuleType("src.models.database")
    fake_models.Provider = type("Provider", (), {"id": "id"})

    monkeypatch.setitem(sys.modules, "src.database", fake_database)
    monkeypatch.setitem(sys.modules, "src.models.database", fake_models)
    monkeypatch.setattr(
        "src.core.crypto.crypto_service.decrypt",
        lambda value: json.dumps(sa_json) if value == "enc_cfg" else "",
    )

    captured: dict[str, object] = {}

    def _fake_build_proxy_client_kwargs(
        proxy_config: dict[str, object] | None = None,
        *,
        timeout: float = 30.0,
        **_: object,
    ) -> dict[str, object]:
        captured["proxy_config"] = proxy_config
        return {"timeout": timeout}

    async def _fake_get_access_token(
        self: VertexAuthService,
        *,
        httpx_client_kwargs: dict[str, object] | None = None,
    ) -> str:
        captured["httpx_client_kwargs"] = httpx_client_kwargs
        return "ya29.test-token"

    monkeypatch.setattr(
        "src.services.proxy_node.resolver.build_proxy_client_kwargs",
        _fake_build_proxy_client_kwargs,
    )
    monkeypatch.setattr(VertexAuthService, "get_access_token", _fake_get_access_token)

    key = SimpleNamespace(
        auth_type="service_account",
        auth_config="enc_cfg",
        api_key="enc_key",
        provider_id="provider-1",
        proxy=None,
    )

    auth = await get_provider_auth(_DetachedEndpoint(), key)  # type: ignore[arg-type]

    assert auth is not None
    assert captured["proxy_config"] == provider_proxy
    assert captured["httpx_client_kwargs"] == {"timeout": 30}
