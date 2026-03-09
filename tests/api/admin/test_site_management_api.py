from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.api.admin import site_management as site_management_api
from src.api.admin.site_management import (
    ApplySiteAccountsSyncRequest,
    SiteManagementAccount,
    SyncSiteAccountsRequest,
    TriggerSiteSyncRequest,
)
from src.core.crypto import CryptoService
from src.models.database import SiteAccount
from src.services.system.config import SystemConfigService


def _make_query(*, all_result=None, first_result=None) -> MagicMock:
    query = MagicMock()
    query.filter.return_value = query
    query.limit.return_value = query
    query.order_by.return_value = query
    query.all.return_value = [] if all_result is None else all_result
    query.first.return_value = first_result
    return query


@pytest.mark.asyncio
async def test_trigger_site_sync_passes_auto_create_toggle(monkeypatch) -> None:
    fake_db = MagicMock()
    captured: dict[str, object] = {}

    def fake_get_config(cls, _db, key, default=None):
        if key == "enable_all_api_hub_auto_create_provider_ops":
            return False
        return default

    def fake_sync_from_backup(self, db, backup, dry_run=False, auto_create_provider_ops=True):
        captured["auto_create_provider_ops"] = auto_create_provider_ops
        return SimpleNamespace(
            total_accounts=1,
            total_providers=1,
            matched_providers=1,
            updated_providers=0,
            skipped_no_provider_ops=1,
            skipped_no_cookie=0,
            skipped_not_changed=0,
            dry_run=dry_run,
            item_results=[],
        )

    monkeypatch.setattr(SystemConfigService, "get_config", classmethod(fake_get_config))
    monkeypatch.setattr(
        site_management_api.AllApiHubSyncService,
        "sync_from_backup_object",
        fake_sync_from_backup,
    )
    monkeypatch.setattr(
        site_management_api.SiteManagementLogService,
        "record_sync_run",
        staticmethod(lambda **kwargs: SimpleNamespace(id="run-1")),
    )

    resp = await site_management_api.trigger_site_sync(
        TriggerSiteSyncRequest(backup={"version": "2.0", "accounts": {"accounts": []}}, dry_run=True),
        db=fake_db,
        _=object(),
    )

    assert resp["run_id"] == "run-1"
    assert captured["auto_create_provider_ops"] is False


@pytest.mark.asyncio
async def test_trigger_checkin_calls_scheduler_manual_mode(monkeypatch) -> None:
    scheduler = SimpleNamespace(
        _perform_provider_checkin=AsyncMock(),
        _perform_site_account_checkin=AsyncMock(),
    )
    fake_db = MagicMock()
    fake_db.query.return_value = _make_query(first_result=("run-1",))

    monkeypatch.setattr(site_management_api, "get_maintenance_scheduler", lambda: scheduler)

    resp = await site_management_api.trigger_site_checkin(db=fake_db, _=object())

    assert resp["ok"] is True
    assert resp["latest_run_id"] == "run-1"
    scheduler._perform_provider_checkin.assert_awaited_once_with(
        trigger_source="manual",
        ignore_enabled=True,
    )
    scheduler._perform_site_account_checkin.assert_awaited_once_with(ignore_enabled=True)
    assert resp["site_account_triggered"] is True


@pytest.mark.asyncio
async def test_list_site_accounts_falls_back_to_webdav_when_cache_empty(monkeypatch) -> None:
    fake_db = MagicMock()
    fake_db.query.side_effect = [_make_query(all_result=[]), _make_query(all_result=[])]
    backup_text = """
    {
      "accounts": {
        "accounts": [
          {
            "site_url": "https://api.usegemini.xyz",
            "authType": "access_token",
            "account_info": {
              "access_token": "tok_123",
              "user_id": "u-001"
            }
          }
        ]
      }
    }
    """.strip()

    def fake_get_config(cls, _db, key, default=None):
        data = {
            "all_api_hub_webdav_url": "https://dav.example.com/backup.json",
            "all_api_hub_webdav_username": "user",
            "all_api_hub_webdav_password": "enc",
        }
        return data.get(key, default)

    monkeypatch.setattr(SystemConfigService, "get_config", classmethod(fake_get_config))
    monkeypatch.setattr(
        site_management_api.SiteManagementLogService,
        "resolve_system_password",
        staticmethod(lambda _raw: "plain-password"),
    )
    monkeypatch.setattr(
        site_management_api,
        "download_backup",
        AsyncMock(return_value=backup_text),
    )

    data = await site_management_api.list_site_accounts(refresh=False, db=fake_db, _=object())
    assert len(data) == 1
    assert data[0]["domain"] == "api.usegemini.xyz"
    assert data[0]["auth_type"] == "access_token"
    assert data[0]["user_id"] == "u-001"
    assert data[0]["access_token"] == "tok_123"


@pytest.mark.asyncio
async def test_list_site_accounts_prefers_cached_records(monkeypatch) -> None:
    crypto = CryptoService()
    cached = SiteAccount(
        id="account-1",
        site_url="https://api.usegemini.xyz",
        domain="api.usegemini.xyz",
        provider_id="provider-1",
        architecture_id="new_api",
        auth_type="access_token",
        checkin_enabled=True,
        balance_sync_enabled=True,
        is_active=True,
        credentials={"api_key": crypto.encrypt("tok_123"), "user_id": "u-001"},
        last_checkin_status="success",
        last_checkin_message="ok",
        last_balance_status="success",
        last_balance_message="ok",
        last_balance_total=12.5,
        last_balance_currency="USD",
        updated_at=datetime.now(timezone.utc),
    )
    fake_db = MagicMock()
    fake_db.query.side_effect = [
        _make_query(all_result=[cached]),
        _make_query(all_result=[("provider-1", "Provider One")]),
    ]

    download_mock = AsyncMock()
    monkeypatch.setattr(site_management_api, "download_backup", download_mock)

    data = await site_management_api.list_site_accounts(refresh=False, db=fake_db, _=object())
    assert len(data) == 1
    assert data[0]["id"] == "account-1"
    assert data[0]["provider_name"] == "Provider One"
    assert data[0]["access_token"] == "tok_123"
    download_mock.assert_not_awaited()


@pytest.mark.asyncio
async def test_apply_site_accounts_sync_with_manual_edit_payload(monkeypatch) -> None:
    fake_db = MagicMock()
    captured: dict[str, object] = {}

    def fake_get_config(cls, _db, key, default=None):
        if key == "enable_all_api_hub_auto_create_provider_ops":
            return True
        return default

    def fake_sync_from_backup(self, db, backup, dry_run=False, auto_create_provider_ops=True):
        captured["backup"] = backup
        captured["dry_run"] = dry_run
        captured["auto_create_provider_ops"] = auto_create_provider_ops
        return SimpleNamespace(
            total_accounts=1,
            total_providers=1,
            matched_providers=1,
            updated_providers=1,
            skipped_no_provider_ops=0,
            skipped_no_cookie=0,
            skipped_not_changed=0,
            dry_run=dry_run,
            item_results=[],
        )

    monkeypatch.setattr(SystemConfigService, "get_config", classmethod(fake_get_config))
    monkeypatch.setattr(
        site_management_api.AllApiHubSyncService,
        "sync_from_backup_object",
        fake_sync_from_backup,
    )
    monkeypatch.setattr(
        site_management_api.SiteManagementLogService,
        "record_sync_run",
        staticmethod(lambda **kwargs: SimpleNamespace(id="run-edit-1")),
    )

    payload = ApplySiteAccountsSyncRequest(
        dry_run=True,
        accounts=[
            SiteManagementAccount(
                site_url="https://api.usegemini.xyz",
                domain="api.usegemini.xyz",
                auth_type="access_token",
                user_id="u-001",
                access_token="tok_123",
                cookie="sid=abc",
            )
        ],
    )
    resp = await site_management_api.apply_site_accounts_sync(payload, db=fake_db, _=object())

    assert resp["run_id"] == "run-edit-1"
    assert captured["dry_run"] is True
    assert captured["auto_create_provider_ops"] is True
    backup = captured["backup"]
    assert isinstance(backup, dict)
    accounts = backup["accounts"]["accounts"]
    assert accounts[0]["site_url"] == "https://api.usegemini.xyz"
    assert accounts[0]["account_info"]["access_token"] == "tok_123"
    assert accounts[0]["account_info"]["user_id"] == "u-001"
    assert accounts[0]["cookieAuth"]["cookie"] == "sid=abc"


@pytest.mark.asyncio
async def test_sync_site_accounts_applies_policy_and_cache_ttl(monkeypatch) -> None:
    fake_db = MagicMock()
    fetched_at = datetime(2026, 3, 7, 0, 0, 0, tzinfo=timezone.utc)

    def fake_get_config(cls, _db, key, default=None):
        data = {
            "all_api_hub_webdav_url": "https://dav.example.com/backup.json",
            "all_api_hub_webdav_username": "user",
            "all_api_hub_webdav_password": "enc",
        }
        return data.get(key, default)

    snapshot_mock = AsyncMock(
        return_value=SimpleNamespace(
            snapshot_id="snap-1",
            source_url="https://dav.example.com/backup.json",
            payload_hash="hash-1",
            from_cache=True,
            fetched_at=fetched_at,
            payload={"accounts": {"accounts": []}},
        )
    )
    apply_mock = MagicMock(
        return_value=SimpleNamespace(
            total_accounts=1,
            matched_accounts=1,
            unmatched_accounts=0,
            created_accounts=0,
            updated_accounts=1,
            skipped_by_policy=0,
        )
    )
    provider_sync_mock = MagicMock(
        return_value=SimpleNamespace(
            total_accounts=1,
            total_providers=1,
            matched_providers=1,
            updated_providers=1,
            skipped_no_provider_ops=0,
            skipped_no_cookie=0,
            skipped_not_changed=0,
        )
    )

    monkeypatch.setattr(SystemConfigService, "get_config", classmethod(fake_get_config))
    monkeypatch.setattr(
        site_management_api.SiteManagementLogService,
        "resolve_system_password",
        staticmethod(lambda _raw: "plain-password"),
    )
    monkeypatch.setattr(
        site_management_api.SiteSnapshotService,
        "get_webdav_snapshot",
        snapshot_mock,
    )
    monkeypatch.setattr(
        site_management_api.SiteAccountSyncService,
        "apply_snapshot",
        apply_mock,
    )
    monkeypatch.setattr(
        site_management_api.AllApiHubSyncService,
        "sync_from_backup_object",
        provider_sync_mock,
    )

    resp = await site_management_api.sync_site_accounts(
        SyncSiteAccountsRequest(
            force_refresh=True,
            cache_ttl_seconds=42,
            apply_policy="matched_only",
        ),
        db=fake_db,
        _=object(),
    )

    snapshot_mock.assert_awaited_once()
    _, snapshot_kwargs = snapshot_mock.await_args
    assert snapshot_kwargs["cache_ttl_seconds"] == 42
    assert snapshot_kwargs["force_refresh"] is True

    apply_mock.assert_called_once()
    _, apply_kwargs = apply_mock.call_args
    assert apply_kwargs["apply_policy"] == "matched_only"
    assert apply_kwargs["source_snapshot_id"] == "snap-1"
    provider_sync_mock.assert_called_once()
    _, provider_sync_kwargs = provider_sync_mock.call_args
    assert provider_sync_kwargs["dry_run"] is False
    assert provider_sync_kwargs["auto_create_provider_ops"] is True

    assert resp["snapshot_id"] == "snap-1"
    assert resp["apply_policy"] == "matched_only"
    assert resp["from_cache"] is True
    assert resp["sync_to_provider"] is True
    assert resp["provider_sync"]["updated_providers"] == 1
