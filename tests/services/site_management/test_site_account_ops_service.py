from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from src.models.database import SiteAccount
from src.services.provider_ops.types import ActionResult, ActionStatus, ProviderActionType
from src.services.site_management.site_account_ops_service import SiteAccountOpsService


class _FakeQuery:
    def __init__(self, account: SiteAccount | None) -> None:
        self._account = account

    def filter(self, *_args, **_kwargs) -> _FakeQuery:
        return self

    def first(self) -> SiteAccount | None:
        return self._account


class _FakeSession:
    def __init__(self, account: SiteAccount | None) -> None:
        self.account = account
        self.commit_calls = 0

    def query(self, *_args, **_kwargs) -> _FakeQuery:
        return _FakeQuery(self.account)

    def commit(self) -> None:
        self.commit_calls += 1


@pytest.mark.asyncio
async def test_checkin_new_api_updates_checkin_status() -> None:
    account = SiteAccount(
        id="acc-1",
        domain="api.example.com",
        architecture_id="new_api",
        auth_type="access_token",
        site_url="https://api.example.com",
        is_active=True,
    )
    db = _FakeSession(account)
    service = SiteAccountOpsService(db)
    service.execute_action = AsyncMock(  # type: ignore[method-assign]
        return_value=ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"checkin_success": True},
            message="签到成功",
        )
    )

    result = await service.checkin("acc-1")

    assert result.action_type == ProviderActionType.CHECKIN
    assert result.status == ActionStatus.SUCCESS
    assert account.last_checkin_status == "success"
    assert account.last_checkin_message == "签到成功"
    assert db.commit_calls == 1


@pytest.mark.asyncio
async def test_checkin_new_api_already_done_maps_to_already_done_status() -> None:
    account = SiteAccount(
        id="acc-2",
        domain="api.example.com",
        architecture_id="new_api",
        auth_type="access_token",
        site_url="https://api.example.com",
        is_active=True,
    )
    db = _FakeSession(account)
    service = SiteAccountOpsService(db)
    service.execute_action = AsyncMock(  # type: ignore[method-assign]
        return_value=ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"checkin_success": None},
            message="今日已签到",
        )
    )

    result = await service.checkin("acc-2")

    assert result.action_type == ProviderActionType.CHECKIN
    assert result.status == ActionStatus.ALREADY_DONE
    assert account.last_checkin_status == "already_done"
    assert account.last_checkin_message == "今日已签到"
    assert db.commit_calls == 1


@pytest.mark.asyncio
async def test_checkin_new_api_not_supported_message_maps_to_not_supported_status() -> None:
    account = SiteAccount(
        id="acc-3",
        domain="api.example.com",
        architecture_id="new_api",
        auth_type="access_token",
        site_url="https://api.example.com",
        is_active=True,
    )
    db = _FakeSession(account)
    service = SiteAccountOpsService(db)
    service.execute_action = AsyncMock(  # type: ignore[method-assign]
        return_value=ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"checkin_success": False},
            message="签到功能未启用",
        )
    )

    result = await service.checkin("acc-3")

    assert result.action_type == ProviderActionType.CHECKIN
    assert result.status == ActionStatus.NOT_SUPPORTED
    assert account.last_checkin_status == "not_supported"
    assert account.last_checkin_message == "签到功能未启用"
    assert db.commit_calls == 1
