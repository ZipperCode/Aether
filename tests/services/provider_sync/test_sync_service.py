import json

import pytest
from src.core.crypto import crypto_service
from src.models.database import Provider
from src.services.provider_sync.sync_service import AllApiHubSyncService


class _FakeQuery:
    def __init__(self, providers: list[Provider]) -> None:
        self._providers = providers

    def all(self) -> list[Provider]:
        return self._providers


class _FakeSession:
    def __init__(self, providers: list[Provider]) -> None:
        self.providers = providers
        self.commit_calls = 0

    def query(self, model: type[Provider]) -> _FakeQuery:
        assert model is Provider
        return _FakeQuery(self.providers)

    def commit(self) -> None:
        self.commit_calls += 1


def _provider(provider_id: str, website: str, config: dict | None) -> Provider:
    return Provider(
        id=provider_id,
        name=f"provider-{provider_id}",
        website=website,
        provider_type="custom",
        config=config,
    )


def test_sync_updates_provider_ops_credentials_by_domain() -> None:
    provider = _provider(
        "p1",
        "https://anyrouter.top",
        {
            "provider_ops": {
                "architecture_id": "anyrouter",
                "connector": {
                    "auth_type": "cookie",
                    "config": {},
                    "credentials": {"session_cookie": crypto_service.encrypt("session=old")},
                },
            }
        },
    )
    db = _FakeSession([provider])

    service = AllApiHubSyncService()
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://anyrouter.top/path",
                    "cookieAuth": {"sessionCookie": "session=new"},
                }
            ]
        },
    }

    result = service.sync_from_backup_object(db, backup)

    assert result.matched_providers == 1
    assert result.updated_providers == 1
    assert db.commit_calls == 1

    encrypted = provider.config["provider_ops"]["connector"]["credentials"]["session_cookie"]
    assert crypto_service.decrypt(encrypted) == "session=new"


def test_sync_dry_run_does_not_commit_or_mutate() -> None:
    original = crypto_service.encrypt("session=old")
    provider = _provider(
        "p1",
        "https://anyrouter.top",
        {
            "provider_ops": {
                "architecture_id": "anyrouter",
                "connector": {
                    "auth_type": "cookie",
                    "config": {},
                    "credentials": {"session_cookie": original},
                },
            }
        },
    )
    db = _FakeSession([provider])

    service = AllApiHubSyncService()
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://anyrouter.top/path",
                    "cookieAuth": {"sessionCookie": "session=new"},
                }
            ]
        },
    }

    result = service.sync_from_backup_object(db, backup, dry_run=True)

    assert result.updated_providers == 1
    assert db.commit_calls == 0
    assert provider.config["provider_ops"]["connector"]["credentials"]["session_cookie"] == original


@pytest.mark.asyncio
async def test_sync_from_webdav_uses_downloader() -> None:
    provider = _provider(
        "p1",
        "https://anyrouter.top",
        {
            "provider_ops": {
                "architecture_id": "anyrouter",
                "connector": {
                    "auth_type": "cookie",
                    "config": {},
                    "credentials": {"session_cookie": crypto_service.encrypt("session=old")},
                },
            }
        },
    )
    db = _FakeSession([provider])

    async def _downloader(*_args: str, **_kwargs: str) -> str:
        return json.dumps(
            {
                "version": "2.0",
                "accounts": {
                    "accounts": [
                        {
                            "site_url": "https://anyrouter.top/path",
                            "cookieAuth": {"sessionCookie": "session=from-webdav"},
                        }
                    ]
                },
            }
        )

    service = AllApiHubSyncService(downloader=_downloader)

    result = await service.sync_from_webdav(db, url="u", username="n", password="p")

    assert result.updated_providers == 1
    encrypted = provider.config["provider_ops"]["connector"]["credentials"]["session_cookie"]
    assert crypto_service.decrypt(encrypted) == "session=from-webdav"


def test_sync_updates_access_token_credentials_by_domain() -> None:
    provider = _provider(
        "p2",
        "https://example.com",
        {
            "provider_ops": {
                "architecture_id": "new_api",
                "connector": {
                    "auth_type": "access_token",
                    "config": {},
                    "credentials": {"access_token": crypto_service.encrypt("old-token")},
                },
            }
        },
    )
    db = _FakeSession([provider])

    service = AllApiHubSyncService()
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "new-token"},
                }
            ]
        },
    }

    result = service.sync_from_backup_object(db, backup)

    assert result.matched_providers == 1
    assert result.updated_providers == 1
    assert db.commit_calls == 1
    encrypted = provider.config["provider_ops"]["connector"]["credentials"]["access_token"]
    assert crypto_service.decrypt(encrypted) == "new-token"


def test_sync_auto_creates_provider_ops_for_access_token_account() -> None:
    provider = _provider(
        "p3",
        "https://auto.example.com",
        {
            "some_other_config": True,
        },
    )
    db = _FakeSession([provider])

    service = AllApiHubSyncService()
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://auto.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "auto-token"},
                }
            ]
        },
    }

    result = service.sync_from_backup_object(db, backup)

    assert result.matched_providers == 1
    assert result.updated_providers == 1
    assert result.skipped_no_provider_ops == 0
    assert db.commit_calls == 1
    provider_ops = provider.config["provider_ops"]
    assert provider_ops["architecture_id"] == "new_api"
    encrypted = provider_ops["connector"]["credentials"]["api_key"]
    assert crypto_service.decrypt(encrypted) == "auto-token"


def test_sync_skips_auto_create_when_disabled() -> None:
    provider = _provider(
        "p4",
        "https://auto.example.com",
        {
            "some_other_config": True,
        },
    )
    db = _FakeSession([provider])

    service = AllApiHubSyncService()
    backup = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://auto.example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "auto-token"},
                }
            ]
        },
    }

    result = service.sync_from_backup_object(db, backup, auto_create_provider_ops=False)

    assert result.matched_providers == 1
    assert result.updated_providers == 0
    assert result.skipped_no_provider_ops == 1
    assert db.commit_calls == 0
    assert provider.config.get("provider_ops") is None
