from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace
from typing import Any

from src.core.crypto import CryptoService
from src.models.database import Provider, SiteAccount
from src.services.site_management.site_account_sync_service import SiteAccountSyncService


class _FakeQuery:
    def __init__(self, data: list[Any]) -> None:
        self._data = data

    def all(self) -> list[Any]:
        return list(self._data)


class _FakeSession:
    def __init__(self, providers: list[Any] | None = None, site_accounts: list[Any] | None = None) -> None:
        self.providers = providers or []
        self.site_accounts = site_accounts or []
        self.added: list[Any] = []
        self.commit_calls = 0

    def query(self, model: Any) -> _FakeQuery:
        if model is Provider:
            return _FakeQuery(self.providers)
        if model is SiteAccount:
            return _FakeQuery(self.site_accounts)
        raise AssertionError(f"unsupported query model: {model}")

    def add(self, item: Any) -> None:
        self.added.append(item)
        if isinstance(item, SiteAccount):
            self.site_accounts.append(item)

    def commit(self) -> None:
        self.commit_calls += 1


def test_apply_snapshot_creates_unmatched_site_account_when_policy_allows() -> None:
    service = SiteAccountSyncService()
    db = _FakeSession(providers=[], site_accounts=[])
    backup = {
        "version": "2.0",
        "accounts": {"accounts": [{"site_url": "https://unmatched.example.com", "cookieAuth": {"cookie": "c=1"}}]},
    }

    result = service.apply_snapshot(
        db,
        snapshot=backup,
        apply_policy="matched_and_unmatched",
        source_snapshot_id="snap-1",
    )

    assert result.total_accounts == 1
    assert result.created_accounts == 1
    assert result.unmatched_accounts == 1
    assert db.commit_calls == 1
    assert any(
        isinstance(item, SiteAccount) and item.domain == "unmatched.example.com" for item in db.added
    )


def test_apply_snapshot_binds_provider_when_domain_matches() -> None:
    provider = SimpleNamespace(
        id="provider-1",
        website="https://matched.example.com",
        config={"provider_ops": {"architecture_id": "new_api"}},
    )
    existing = SiteAccount(
        domain="matched.example.com",
        source_type="all_api_hub_webdav",
        auth_type="cookie",
    )
    service = SiteAccountSyncService()
    db = _FakeSession(providers=[provider], site_accounts=[existing])
    backup = {
        "version": "2.0",
        "accounts": {"accounts": [{"site_url": "https://matched.example.com", "cookieAuth": {"cookie": "c=2"}}]},
    }

    result = service.apply_snapshot(
        db,
        snapshot=backup,
        apply_policy="matched_and_unmatched",
        source_snapshot_id="snap-2",
    )

    assert result.total_accounts == 1
    assert result.matched_accounts == 1
    assert existing.provider_id == "provider-1"
    assert existing.architecture_id == "new_api"
    assert db.commit_calls == 1


def test_apply_snapshot_creates_multiple_accounts_for_same_domain_when_user_id_differs() -> None:
    service = SiteAccountSyncService()
    db = _FakeSession(providers=[], site_accounts=[])
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://same.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "tok_1", "user_id": "u1"},
                },
                {
                    "site_url": "https://same.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "tok_2", "user_id": "u2"},
                },
            ]
        },
    }

    result = service.apply_snapshot(
        db,
        snapshot=backup,
        apply_policy="matched_and_unmatched",
        source_snapshot_id="snap-3",
    )

    assert result.total_accounts == 2
    assert result.created_accounts == 2
    assert len(db.site_accounts) == 2
    encrypted_tokens = {
        str((account.credentials or {}).get("api_key") or "") for account in db.site_accounts
    }
    assert "tok_1" not in encrypted_tokens
    assert "tok_2" not in encrypted_tokens


def test_apply_snapshot_matches_existing_account_by_domain_auth_and_user_id() -> None:
    crypto = CryptoService()
    existing = SiteAccount(
        domain="same.example.com",
        source_type="all_api_hub_webdav",
        auth_type="access_token",
        credentials={"api_key": crypto.encrypt("old_token"), "user_id": "u1"},
    )
    service = SiteAccountSyncService()
    db = _FakeSession(providers=[], site_accounts=[existing])
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://same.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "new_token", "user_id": "u1"},
                }
            ]
        },
    }

    result = service.apply_snapshot(
        db,
        snapshot=backup,
        apply_policy="matched_and_unmatched",
        source_snapshot_id="snap-4",
    )

    assert result.total_accounts == 1
    assert result.created_accounts == 0
    assert result.updated_accounts == 1
    assert len(db.site_accounts) == 1
    assert str((existing.credentials or {}).get("api_key") or "") != "new_token"


def test_apply_snapshot_keeps_account_unchanged_when_credentials_semantically_same() -> None:
    crypto = CryptoService()
    old_updated_at = datetime(2026, 3, 7, 0, 0, 0, tzinfo=timezone.utc)
    existing_ciphertext = crypto.encrypt("same_token")
    existing = SiteAccount(
        domain="same.example.com",
        site_url="https://same.example.com",
        base_url="https://same.example.com",
        source_type="all_api_hub_webdav",
        source_snapshot_id="snap-keep",
        architecture_id="new_api",
        auth_type="access_token",
        credentials={"api_key": existing_ciphertext, "user_id": "u1"},
        updated_at=old_updated_at,
    )
    service = SiteAccountSyncService()
    db = _FakeSession(providers=[], site_accounts=[existing])
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://same.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "same_token", "user_id": "u1"},
                }
            ]
        },
    }

    result = service.apply_snapshot(
        db,
        snapshot=backup,
        apply_policy="matched_and_unmatched",
        source_snapshot_id="snap-keep",
    )

    assert result.total_accounts == 1
    assert result.created_accounts == 0
    assert result.updated_accounts == 0
    assert existing.updated_at == old_updated_at
    assert (existing.credentials or {}).get("api_key") == existing_ciphertext
