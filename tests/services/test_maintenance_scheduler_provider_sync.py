from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock

import pytest

from src.services.system.maintenance_scheduler import MaintenanceScheduler


@pytest.mark.asyncio
async def test_provider_sync_disabled(monkeypatch):
    scheduler = MaintenanceScheduler()

    mock_db = MagicMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.create_session",
        lambda: mock_db,
    )

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_all_api_hub_sync":
            return False
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    sync_mock = AsyncMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.AllApiHubSyncService.sync_from_webdav",
        sync_mock,
    )

    await scheduler._perform_all_api_hub_sync()

    sync_mock.assert_not_called()


@pytest.mark.asyncio
async def test_provider_sync_enabled_runs(monkeypatch):
    scheduler = MaintenanceScheduler()

    mock_db = MagicMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.create_session",
        lambda: mock_db,
    )

    def fake_get_config(cls, db, key, default=None):
        data = {
            "enable_all_api_hub_sync": True,
            "all_api_hub_webdav_url": "https://dav.example.com/backup.json",
            "all_api_hub_webdav_username": "u",
            "all_api_hub_webdav_password": "p",
        }
        return data.get(key, default)

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    sync_mock = AsyncMock(
        return_value=MagicMock(
            total_accounts=1,
            matched_providers=1,
            updated_providers=1,
            skipped_no_provider_ops=0,
            skipped_no_cookie=0,
            skipped_not_changed=0,
        )
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.AllApiHubSyncService.sync_from_webdav",
        sync_mock,
    )

    await scheduler._perform_all_api_hub_sync()

    sync_mock.assert_awaited_once()
    _, kwargs = sync_mock.await_args
    assert kwargs["url"] == "https://dav.example.com/backup.json"
    assert kwargs["username"] == "u"
    assert kwargs["password"] == "p"
    assert kwargs["dry_run"] is False
