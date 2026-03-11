"""Tests for AccountOpsService (decoupled from Provider)."""
from __future__ import annotations

import inspect
import re
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.modules.site_management.services.account_ops_service import AccountOpsService
from src.services.provider_ops.types import (
    ActionResult,
    ActionStatus,
    ConnectorAuthType,
    ProviderActionType,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_account(
    account_id: str = "acct-1",
    is_active: bool = True,
    architecture_id: str | None = "new_api",
    base_url: str | None = "https://api.example.com",
    site_url: str | None = "https://api.example.com",
    auth_type: str = "cookie",
    credentials: dict | None = None,
    config: dict | None = None,
) -> MagicMock:
    acct = MagicMock()
    acct.id = account_id
    acct.is_active = is_active
    acct.architecture_id = architecture_id
    acct.base_url = base_url
    acct.site_url = site_url
    acct.auth_type = auth_type
    acct.credentials = credentials or {"cookie": "enc-value"}
    acct.config = config
    acct.last_balance_status = None
    acct.last_balance_message = None
    acct.last_balance_at = None
    acct.last_balance_total = None
    acct.last_balance_currency = None
    acct.last_checkin_status = None
    acct.last_checkin_message = None
    acct.last_checkin_at = None
    acct.updated_at = None
    return acct


@pytest.fixture
def mock_db():
    return MagicMock()


@pytest.fixture
def service(mock_db):
    svc = AccountOpsService(db=mock_db)
    svc.crypto = MagicMock()
    svc.crypto.decrypt.side_effect = lambda v: f"dec({v})"
    return svc


# ---------------------------------------------------------------------------
# Test: _build_target uses account's own fields
# ---------------------------------------------------------------------------


class TestBuildTargetFromAccountFields:
    def test_execute_action_builds_target_from_account_fields(self, service, mock_db):
        """Verify that _build_target reads exclusively from the account object."""
        account = _make_account(
            architecture_id="new_api",
            base_url="https://my-site.com",
            auth_type="cookie",
            credentials={"api_key": "secret-key"},
            config={"actions": {"query_balance": {"path": "/api/balance"}}},
        )

        target = service._build_target(account)

        assert target is not None
        assert target.target_id == "acct-1"
        assert target.architecture_id == "new_api"
        assert target.base_url == "https://my-site.com"
        assert target.auth_type == ConnectorAuthType.API_KEY
        assert target.actions == {"query_balance": {"path": "/api/balance"}}
        # credentials should come from account.credentials, decrypted
        assert target.credentials == {"api_key": "dec(secret-key)"}

    def test_build_target_falls_back_to_site_url(self, service):
        """When base_url is empty, site_url should be used."""
        account = _make_account(base_url=None, site_url="https://fallback.example.com")
        target = service._build_target(account)
        assert target is not None
        assert target.base_url == "https://fallback.example.com"

    def test_build_target_falls_back_to_config_base_url(self, service):
        """When both base_url and site_url are empty, config.base_url is used."""
        account = _make_account(
            base_url=None,
            site_url=None,
            config={"base_url": "https://config-url.example.com"},
        )
        target = service._build_target(account)
        assert target is not None
        assert target.base_url == "https://config-url.example.com"

    def test_build_target_returns_none_when_no_url(self, service):
        """No URL at all should produce None."""
        account = _make_account(base_url=None, site_url=None, config={})
        target = service._build_target(account)
        assert target is None

    def test_build_target_connector_config_from_account(self, service):
        """connector_config should come from account.config only."""
        account = _make_account(
            config={"connector": {"config": {"timeout": 30}, "auth_type": "api_key"}},
        )
        target = service._build_target(account)
        assert target is not None
        assert target.connector_config == {"timeout": 30}

    def test_build_target_infers_architecture_from_auth_type(self, service):
        """When architecture_id is None, infer from auth_type."""
        account = _make_account(
            architecture_id=None,
            auth_type="cookie",
            config={},
        )
        target = service._build_target(account)
        assert target is not None
        assert target.architecture_id == "new_api"


# ---------------------------------------------------------------------------
# Test: no Provider query
# ---------------------------------------------------------------------------


class TestNoProviderLookup:
    def test_execute_action_no_provider_lookup(self, service, mock_db):
        """Verify db.query is only called with SiteAccount, never with Provider."""
        from src.modules.site_management.models import SiteAccount as SA

        account = _make_account()
        mock_db.query.return_value.filter.return_value.first.return_value = account

        # Patch _ensure_connected to return a mock connector
        mock_connector = AsyncMock()
        mock_result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"total_available": 100.0, "currency": "USD"},
            message="OK",
        )
        service.execution_engine = MagicMock()
        service.execution_engine.execute = AsyncMock(return_value=mock_result)
        service.execution_engine.create_connector = MagicMock(return_value=mock_connector)
        mock_connector.connect = AsyncMock(return_value=True)
        mock_connector.is_authenticated = AsyncMock(return_value=False)

        import asyncio
        result = asyncio.get_event_loop().run_until_complete(
            service.execute_action("acct-1", ProviderActionType.QUERY_BALANCE)
        )

        # Verify only SiteAccount queries, no Provider queries
        for call in mock_db.query.call_args_list:
            queried_model = call.args[0]
            assert queried_model is SA, (
                f"Expected db.query(SiteAccount), but got db.query({queried_model})"
            )

    def test_build_target_never_accesses_provider_id(self, service):
        """_build_target should not try to look up provider_id."""
        account = _make_account()
        account.provider_id = "some-provider-id"

        # Should succeed without any Provider DB query
        target = service._build_target(account)
        assert target is not None


# ---------------------------------------------------------------------------
# Test: _persist_last_result writes balance fields
# ---------------------------------------------------------------------------


class TestPersistBalanceResult:
    def test_persist_balance_result(self, service):
        """Result should be written to last_balance_* fields on the account."""
        account = _make_account()
        result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"total_available": 42.5, "currency": "CNY"},
            message="Balance retrieved",
        )

        service._persist_last_result(
            account, action_type=ProviderActionType.QUERY_BALANCE, result=result
        )

        assert account.last_balance_status == "success"
        assert account.last_balance_message == "Balance retrieved"
        assert account.last_balance_total == 42.5
        assert account.last_balance_currency == "CNY"
        assert account.last_balance_at is not None
        assert account.updated_at is not None

    def test_persist_checkin_result(self, service):
        """Checkin result should be written to last_checkin_* fields."""
        account = _make_account()
        result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.CHECKIN,
            message="Checkin success",
        )

        service._persist_last_result(
            account, action_type=ProviderActionType.CHECKIN, result=result
        )

        assert account.last_checkin_status == "success"
        assert account.last_checkin_message == "Checkin success"
        assert account.last_checkin_at is not None
        assert account.updated_at is not None

    def test_persist_balance_result_with_none_data(self, service):
        """When data is None, balance total and currency should be None."""
        account = _make_account()
        result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data=None,
            message="OK",
        )

        service._persist_last_result(
            account, action_type=ProviderActionType.QUERY_BALANCE, result=result
        )

        assert account.last_balance_total is None
        assert account.last_balance_currency is None


# ---------------------------------------------------------------------------
# Test: no Provider imports in source
# ---------------------------------------------------------------------------


class TestNoProviderImports:
    def test_no_provider_imports(self):
        """Inspect source code: must not contain any Provider import or reference."""
        import src.modules.site_management.services.account_ops_service as mod

        source = inspect.getsource(mod)
        # Match standalone "Provider" as a word boundary (not ProviderActionType, etc.)
        # We only want to flag raw Provider model references
        forbidden_patterns = [
            r"from\s+\S+\s+import\s+.*\bProvider\b",  # import Provider
            r"\bProvider\.(?!Action)",  # Provider.xxx (but not ProviderActionType)
            r"db\.query\(Provider\)",  # db.query(Provider)
        ]
        for pattern in forbidden_patterns:
            matches = re.findall(pattern, source)
            assert not matches, (
                f"account_ops_service.py must not reference Provider model. "
                f"Pattern '{pattern}' matched: {matches}"
            )

    def test_no_extract_provider_ops_config_method(self):
        """The _extract_provider_ops_config method must not exist."""
        assert not hasattr(AccountOpsService, "_extract_provider_ops_config"), (
            "AccountOpsService must not have _extract_provider_ops_config method"
        )


# ---------------------------------------------------------------------------
# Test: _resolve_* methods have no provider_config parameter
# ---------------------------------------------------------------------------


class TestResolveMethodSignatures:
    def test_resolve_auth_type_has_no_provider_config(self):
        """_resolve_auth_type should not accept provider_config."""
        import inspect as insp

        sig = insp.signature(AccountOpsService._resolve_auth_type)
        assert "provider_config" not in sig.parameters

    def test_resolve_connector_config_has_no_provider_config(self):
        import inspect as insp

        sig = insp.signature(AccountOpsService._resolve_connector_config)
        assert "provider_config" not in sig.parameters

    def test_resolve_actions_has_no_provider_config(self):
        import inspect as insp

        sig = insp.signature(AccountOpsService._resolve_actions)
        assert "provider_config" not in sig.parameters

    def test_resolve_credentials_has_no_provider_config(self):
        import inspect as insp

        sig = insp.signature(AccountOpsService._resolve_credentials)
        assert "provider_config" not in sig.parameters


# ---------------------------------------------------------------------------
# Test: checkin helper
# ---------------------------------------------------------------------------


class TestCheckinHelpers:
    def test_to_checkin_result_from_new_api_success(self):
        result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"checkin_success": True, "total_available": 100.0},
            message="OK",
        )
        checkin = AccountOpsService._to_checkin_result_from_new_api(result)
        assert checkin.status == ActionStatus.SUCCESS
        assert checkin.action_type == ProviderActionType.CHECKIN

    def test_to_checkin_result_from_new_api_already_done(self):
        result = ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=ProviderActionType.QUERY_BALANCE,
            data={"checkin_success": None},
            message="OK",
        )
        checkin = AccountOpsService._to_checkin_result_from_new_api(result)
        assert checkin.status == ActionStatus.ALREADY_DONE

    def test_contains_checkin_not_supported_signal(self):
        assert AccountOpsService._contains_checkin_not_supported_signal("签到功能未启用")
        assert AccountOpsService._contains_checkin_not_supported_signal("Feature not enabled yet")
        assert not AccountOpsService._contains_checkin_not_supported_signal("签到成功")
        assert not AccountOpsService._contains_checkin_not_supported_signal(None)
        assert not AccountOpsService._contains_checkin_not_supported_signal("")


# ---------------------------------------------------------------------------
# Test: _extract_balance_total_and_currency
# ---------------------------------------------------------------------------


class TestExtractBalance:
    def test_dict_with_total_available(self):
        total, currency = AccountOpsService._extract_balance_total_and_currency(
            {"total_available": 42.5, "currency": "USD"}
        )
        assert total == 42.5
        assert currency == "USD"

    def test_dict_with_balance_fallback(self):
        total, currency = AccountOpsService._extract_balance_total_and_currency(
            {"balance": 10.0}
        )
        assert total == 10.0
        assert currency is None

    def test_none_data(self):
        total, currency = AccountOpsService._extract_balance_total_and_currency(None)
        assert total is None
        assert currency is None

    def test_invalid_total(self):
        total, currency = AccountOpsService._extract_balance_total_and_currency(
            {"total_available": "not-a-number"}
        )
        assert total is None
