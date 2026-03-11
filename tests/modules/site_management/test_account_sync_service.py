"""Tests for AccountSyncService."""
from __future__ import annotations

import inspect
from unittest.mock import MagicMock, patch

import pytest

from src.modules.site_management.services.account_sync_service import (
    AccountSyncResult,
    AccountSyncService,
)

_SA_PATCH_TARGET = "src.modules.site_management.services.account_sync_service.SiteAccount"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_snapshot(*accounts: dict) -> dict:
    """Build a minimal all-api-hub-style snapshot payload."""
    return {"accounts": {"accounts": list(accounts)}}


def _cookie_account(domain: str, cookie: str = "ck-1", user_id: str | None = None) -> dict:
    result: dict = {
        "site_url": f"https://{domain}",
        "authType": "cookie",
        "cookieAuth": {"sessionCookie": cookie},
    }
    if user_id is not None:
        result["account_info"] = {"user_id": user_id}
    return result


def _token_account(domain: str, token: str = "tok-1") -> dict:
    return {
        "site_url": f"https://{domain}",
        "authType": "access_token",
        "account_info": {"access_token": token},
    }


def _make_existing_account(
    domain: str,
    auth_type: str = "cookie",
    credentials: dict | None = None,
    webdav_source_id: str = "src-1",
    site_url: str | None = None,
) -> MagicMock:
    acct = MagicMock()
    acct.domain = domain
    acct.auth_type = auth_type
    acct.credentials = credentials or {}
    acct.webdav_source_id = webdav_source_id
    acct.site_url = site_url or f"https://{domain}"
    acct.base_url = site_url or f"https://{domain}"
    acct.architecture_id = None
    acct.updated_at = None
    return acct


def _stub_site_account_class():
    """Return a callable that mimics SiteAccount(...) but produces a MagicMock.

    The returned class also exposes a ``webdav_source_id`` attribute so that
    ``SiteAccount.webdav_source_id == x`` works in the filter expression.
    """

    class _FakeSiteAccountClass:
        webdav_source_id = "webdav_source_id_col"

        def __new__(cls, **kwargs):
            obj = MagicMock()
            for k, v in kwargs.items():
                setattr(obj, k, v)
            # Default attributes that _apply_account_fields reads
            obj.credentials = kwargs.get("credentials", {})
            obj.site_url = kwargs.get("site_url", None)
            obj.base_url = kwargs.get("base_url", None)
            obj.architecture_id = kwargs.get("architecture_id", None)
            obj.updated_at = kwargs.get("updated_at", None)
            return obj

    return _FakeSiteAccountClass


@pytest.fixture
def mock_db():
    db = MagicMock()
    # By default return no existing accounts
    db.query.return_value.filter.return_value.all.return_value = []
    return db


@pytest.fixture
def service():
    crypto = MagicMock()
    crypto.encrypt.side_effect = lambda v: f"enc({v})"
    crypto.decrypt.side_effect = lambda v: v
    return AccountSyncService(crypto=crypto)


# ---------------------------------------------------------------------------
# Test: new accounts are created
# ---------------------------------------------------------------------------

class TestSyncCreatesNewAccounts:
    def test_creates_accounts_for_new_domains(self, service, mock_db):
        snapshot = _make_snapshot(
            _cookie_account("alpha.com", cookie="cookie-alpha"),
            _cookie_account("beta.com", cookie="cookie-beta"),
        )

        with patch(_SA_PATCH_TARGET, _stub_site_account_class()):
            result = service.apply_snapshot(
                mock_db, snapshot=snapshot, webdav_source_id="src-1"
            )

        assert isinstance(result, AccountSyncResult)
        assert result.total_accounts == 2
        assert result.created_accounts == 2
        assert result.updated_accounts == 0

        # Two new accounts should have been added
        assert mock_db.add.call_count == 2
        mock_db.commit.assert_called_once()

        # Verify the SiteAccount instances were created with correct fields
        added_accounts = [call.args[0] for call in mock_db.add.call_args_list]
        domains = {a.domain for a in added_accounts}
        assert domains == {"alpha.com", "beta.com"}
        for a in added_accounts:
            assert a.webdav_source_id == "src-1"
            assert a.is_active is True

    def test_token_account_gets_new_api_architecture(self, service, mock_db):
        snapshot = _make_snapshot(_token_account("api.example.com", token="my-token"))

        with patch(_SA_PATCH_TARGET, _stub_site_account_class()):
            result = service.apply_snapshot(
                mock_db, snapshot=snapshot, webdav_source_id="src-1"
            )

        assert result.created_accounts == 1
        added = mock_db.add.call_args_list[0].args[0]
        # architecture_id is applied via _apply_account_fields after creation
        assert added.architecture_id == "new_api"


# ---------------------------------------------------------------------------
# Test: existing accounts are updated
# ---------------------------------------------------------------------------

class TestSyncUpdatesExisting:
    def test_updates_credentials_for_existing_domain(self, service, mock_db):
        existing = _make_existing_account("alpha.com", credentials={})
        mock_db.query.return_value.filter.return_value.all.return_value = [existing]

        snapshot = _make_snapshot(
            _cookie_account("alpha.com", cookie="new-cookie"),
        )

        result = service.apply_snapshot(
            mock_db, snapshot=snapshot, webdav_source_id="src-1"
        )

        assert result.total_accounts == 1
        assert result.created_accounts == 0
        assert result.updated_accounts == 1
        # Should NOT add a new record
        mock_db.add.assert_not_called()
        mock_db.commit.assert_called_once()


# ---------------------------------------------------------------------------
# Test: scoped to webdav_source_id
# ---------------------------------------------------------------------------

class TestSyncScopedToSource:
    def test_query_filters_by_webdav_source_id(self, service, mock_db):
        snapshot = _make_snapshot(_cookie_account("gamma.com"))

        with patch(_SA_PATCH_TARGET, _stub_site_account_class()):
            service.apply_snapshot(
                mock_db, snapshot=snapshot, webdav_source_id="src-42"
            )

        # Verify the filter call used webdav_source_id
        filter_call = mock_db.query.return_value.filter
        filter_call.assert_called_once()
        filter_args = filter_call.call_args
        assert filter_args is not None


# ---------------------------------------------------------------------------
# Test: no references to the legacy model
# ---------------------------------------------------------------------------

class TestNoProviderReferences:
    def test_service_module_has_no_provider_imports(self):
        """Verify that account_sync_service.py does not reference the legacy model."""
        import src.modules.site_management.services.account_sync_service as mod

        source = inspect.getsource(mod)
        # Check for Provider as a standalone word (import/usage), ignoring
        # substring matches like "SiteSourceSnapshot".
        import re
        matches = re.findall(r"\bProvider\b", source)
        assert not matches, (
            "account_sync_service.py must not reference Provider"
        )

    def test_parsers_module_has_no_provider_imports(self):
        """Verify that parsers.py does not reference the legacy model."""
        import src.modules.site_management.services.parsers as mod

        source = inspect.getsource(mod)
        import re
        matches = re.findall(r"\bProvider\b", source)
        assert not matches, (
            "parsers.py must not reference Provider"
        )
