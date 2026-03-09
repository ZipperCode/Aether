from __future__ import annotations

from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Any

import pytest

from src.services.provider_ops.types import (
    ActionResult,
    ActionStatus,
    ConnectorAuthType,
    ProviderActionType,
)


@dataclass
class _FakeConnector:
    authenticated: bool = True

    async def is_authenticated(self) -> bool:
        return self.authenticated

    @asynccontextmanager
    async def get_client(self):
        yield object()


class _FakeAction:
    action_type = ProviderActionType.QUERY_BALANCE

    def __init__(self, config: dict[str, Any] | None = None):
        self.config = dict(config or {})

    async def execute(self, _client: Any) -> ActionResult:
        return ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"merged_config": self.config},
        )


class _FakeArchitecture:
    architecture_id = "fake_arch"

    def supports_action(self, action_type: ProviderActionType) -> bool:
        return action_type == ProviderActionType.QUERY_BALANCE

    def get_action(
        self, action_type: ProviderActionType, config: dict[str, Any] | None = None
    ) -> _FakeAction:
        assert action_type == ProviderActionType.QUERY_BALANCE
        return _FakeAction(config)

    def get_connector(self, *_, **__) -> _FakeConnector:
        return _FakeConnector()


class _FakeRegistry:
    def __init__(self, arch: _FakeArchitecture) -> None:
        self.arch = arch

    def get_or_default(self, architecture_id: str | None = None) -> _FakeArchitecture:
        assert architecture_id in {None, "fake_arch"}
        return self.arch


@pytest.mark.asyncio
async def test_execution_engine_merges_saved_config_and_runtime_override() -> None:
    from src.services.provider_ops.execution_engine import OpsExecutionEngine, OpsExecutionTarget

    arch = _FakeArchitecture()
    engine = OpsExecutionEngine(registry_getter=lambda: _FakeRegistry(arch))

    target = OpsExecutionTarget(
        target_id="site-account-1",
        architecture_id="fake_arch",
        base_url="https://example.com",
        auth_type=ConnectorAuthType.API_KEY,
        connector_config={},
        credentials={"cookie": "sid=1", "user_id": "u1"},
        actions={"query_balance": {"config": {"endpoint": "/saved"}}},
    )
    connector = _FakeConnector(authenticated=True)

    result = await engine.execute(
        connector=connector,
        target=target,
        action_type=ProviderActionType.QUERY_BALANCE,
        action_config={"endpoint": "/runtime", "extra": "v"},
    )

    assert result.status == ActionStatus.SUCCESS
    merged = result.data["merged_config"]
    assert merged["endpoint"] == "/runtime"
    assert merged["extra"] == "v"
    assert merged["_provider_id"] == "site-account-1"
    assert merged["_has_cookie"] is True
    assert merged["_has_user_id"] is True


@pytest.mark.asyncio
async def test_execution_engine_returns_auth_expired_when_connector_invalid() -> None:
    from src.services.provider_ops.execution_engine import OpsExecutionEngine, OpsExecutionTarget

    engine = OpsExecutionEngine(registry_getter=lambda: _FakeRegistry(_FakeArchitecture()))
    target = OpsExecutionTarget(
        target_id="provider-1",
        architecture_id="fake_arch",
        base_url="https://example.com",
        auth_type=ConnectorAuthType.API_KEY,
        connector_config={},
        credentials={"api_key": "k"},
        actions={},
    )

    result = await engine.execute(
        connector=_FakeConnector(authenticated=False),
        target=target,
        action_type=ProviderActionType.QUERY_BALANCE,
    )

    assert result.status == ActionStatus.AUTH_EXPIRED
    assert result.message == "认证已过期，请重新连接"
