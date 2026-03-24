from __future__ import annotations

from typing import Any

import pytest

from src.services.provider_ops.service import ProviderOpsService
from src.services.provider_ops.types import ConnectorAuthType


class _FakeDB:
    new: tuple[Any, ...] = ()
    dirty: tuple[Any, ...] = ()
    deleted: tuple[Any, ...] = ()

    def in_transaction(self) -> bool:
        return False

    def commit(self) -> None:
        pass

    def rollback(self) -> None:
        pass


class _FailingArchitecture:
    def get_verify_endpoint(self) -> str:
        return "/verify"

    async def prepare_verify_config(
        self,
        _base_url: str,
        _config: dict[str, Any],
        _credentials: dict[str, Any],
    ) -> dict[str, Any]:
        raise ValueError("invalid refresh token")

    def build_verify_headers(
        self,
        _config: dict[str, Any],
        _credentials: dict[str, Any],
    ) -> dict[str, str]:
        raise AssertionError("build_verify_headers should not be reached")


class _FakeRegistry:
    def __init__(self, architecture: Any) -> None:
        self._architecture = architecture

    def get_or_default(self, _architecture_id: str) -> Any:
        return self._architecture


@pytest.mark.asyncio
async def test_verify_auth_returns_failure_when_prepare_verify_config_raises_value_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    service = ProviderOpsService(_FakeDB())
    architecture = _FailingArchitecture()

    monkeypatch.setattr(
        "src.services.provider_ops.service.get_registry",
        lambda: _FakeRegistry(architecture),
    )

    result = await service.verify_auth(
        base_url="https://example.com",
        architecture_id="sub2api",
        auth_type=ConnectorAuthType.SESSION_LOGIN,
        config={},
        credentials={"refresh_token": "stale-token"},
    )

    assert result == {"success": False, "message": "invalid refresh token"}
