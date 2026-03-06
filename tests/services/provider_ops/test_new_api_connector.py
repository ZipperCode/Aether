from __future__ import annotations

import sys
import types

import httpx
import pytest

_service_stub = types.ModuleType("src.services.provider_ops.service")
_service_stub.ProviderOpsService = object  # type: ignore[attr-defined]
sys.modules.setdefault("src.services.provider_ops.service", _service_stub)

_cache_stub = types.ModuleType("src.core.cache_service")


class _CacheServiceStub:
    @staticmethod
    async def get(_key: str):
        return None

    @staticmethod
    async def set(_key: str, _value, _ttl_seconds: int = 60):
        return True


_cache_stub.CacheService = _CacheServiceStub  # type: ignore[attr-defined]
sys.modules.setdefault("src.core.cache_service", _cache_stub)

_proxy_pkg_stub = types.ModuleType("src.services.proxy_node")
_proxy_pkg_stub.__path__ = []  # type: ignore[attr-defined]
sys.modules["src.services.proxy_node"] = _proxy_pkg_stub

_proxy_resolver_stub = types.ModuleType("src.services.proxy_node.resolver")


def _resolve_ops_proxy_config_stub(_config):
    return None, None


_proxy_resolver_stub.resolve_ops_proxy_config = _resolve_ops_proxy_config_stub  # type: ignore[attr-defined]
sys.modules["src.services.proxy_node.resolver"] = _proxy_resolver_stub

from src.services.provider_ops.architectures.new_api import NewApiConnector
from src.services.provider_ops.architectures import base as _arch_base

_arch_base.resolve_ops_proxy_config = _resolve_ops_proxy_config_stub  # type: ignore[assignment]


@pytest.mark.asyncio
async def test_new_api_connector_allows_api_key_without_user_id() -> None:
    connector = NewApiConnector("https://example.com")
    ok = await connector.connect({"api_key": "token-1"})
    assert ok is True
    assert await connector.is_authenticated() is True


def test_new_api_connector_does_not_send_user_header_when_missing_user_id() -> None:
    connector = NewApiConnector("https://example.com")
    # Directly set connected auth state; header behavior is what we care about.
    connector._api_key = "token-1"  # type: ignore[attr-defined]
    connector._user_id = None  # type: ignore[attr-defined]
    req = httpx.Request("GET", "https://example.com/api/user/checkin")
    req = connector._apply_auth(req)
    assert req.headers.get("Authorization") == "Bearer token-1"
    assert "New-Api-User" not in req.headers
    assert "New-API-User" not in req.headers


def test_new_api_connector_sends_compat_user_headers_when_user_id_present() -> None:
    connector = NewApiConnector("https://example.com")
    connector._api_key = "token-1"  # type: ignore[attr-defined]
    connector._user_id = "1001"  # type: ignore[attr-defined]
    req = httpx.Request("GET", "https://example.com/api/user/checkin")
    req = connector._apply_auth(req)
    assert req.headers.get("Authorization") == "Bearer token-1"
    assert req.headers.get("New-Api-User") == "1001"
    assert req.headers.get("New-API-User") == "1001"
    assert req.headers.get("Veloera-User") == "1001"
