from __future__ import annotations

import pytest

from src.api.admin import site_management as site_management_api
from src.services.provider_ops.types import ActionResult, ActionStatus, ProviderActionType


class _FakeSession:
    pass


@pytest.mark.asyncio
async def test_manual_checkin_for_site_account_returns_action_result() -> None:
    fake_db = _FakeSession()

    class _FakeOpsService:
        def __init__(self, db):
            assert db is fake_db

        async def checkin(self, account_id: str):
            assert account_id == "account-1"
            return ActionResult(
                status=ActionStatus.SUCCESS,
                action_type=ProviderActionType.CHECKIN,
                data={"checkin_success": True},
                message="签到成功",
            )

    original = getattr(site_management_api, "SiteAccountOpsService", None)
    site_management_api.SiteAccountOpsService = _FakeOpsService  # type: ignore[attr-defined]
    try:
        resp = await site_management_api.checkin_site_account(  # type: ignore[attr-defined]
            account_id="account-1",
            db=fake_db,
            _=object(),
        )
        assert resp["status"] == "success"
        assert resp["action_type"] == "checkin"
        assert resp["data"]["checkin_success"] is True
    finally:
        if original is not None:
            site_management_api.SiteAccountOpsService = original  # type: ignore[attr-defined]
        else:
            delattr(site_management_api, "SiteAccountOpsService")


@pytest.mark.asyncio
async def test_manual_balance_for_site_account_returns_action_result() -> None:
    fake_db = _FakeSession()

    class _FakeOpsService:
        def __init__(self, db):
            assert db is fake_db

        async def query_balance(self, account_id: str):
            assert account_id == "account-2"
            return ActionResult(
                status=ActionStatus.SUCCESS,
                action_type=ProviderActionType.QUERY_BALANCE,
                data={"total_available": 12.5, "currency": "USD"},
                message="ok",
            )

    original = getattr(site_management_api, "SiteAccountOpsService", None)
    site_management_api.SiteAccountOpsService = _FakeOpsService  # type: ignore[attr-defined]
    try:
        resp = await site_management_api.balance_site_account(  # type: ignore[attr-defined]
            account_id="account-2",
            db=fake_db,
            _=object(),
        )
        assert resp["status"] == "success"
        assert resp["action_type"] == "query_balance"
        assert resp["data"]["total_available"] == 12.5
    finally:
        if original is not None:
            site_management_api.SiteAccountOpsService = original  # type: ignore[attr-defined]
        else:
            delattr(site_management_api, "SiteAccountOpsService")
