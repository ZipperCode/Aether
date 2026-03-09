from __future__ import annotations

from dataclasses import dataclass
from unittest.mock import AsyncMock, MagicMock

import pytest

from src.services.system.maintenance_scheduler import MaintenanceScheduler
from src.services.provider_ops.types import ActionResult, ActionStatus, ProviderActionType


class _FakeQuery:
    def __init__(self, rows):
        self._rows = rows

    def filter(self, *_args, **_kwargs):
        return self

    def all(self):
        return list(self._rows)


class _FakeSession:
    def __init__(self, rows=None):
        self.rows = rows or []
        self.closed = False

    def query(self, *_args, **_kwargs):
        return _FakeQuery(self.rows)

    def close(self):
        self.closed = True


@dataclass
class _ActionCollector:
    method_name: str
    calls: list[str]

    async def run(self, account_id: str) -> ActionResult:
        self.calls.append(account_id)
        action_type = (
            ProviderActionType.CHECKIN
            if self.method_name == "checkin"
            else ProviderActionType.QUERY_BALANCE
        )
        return ActionResult(
            status=ActionStatus.SUCCESS,
            action_type=action_type,
            data={"ok": True},
            message="ok",
        )


@pytest.mark.asyncio
async def test_site_account_sync_disabled(monkeypatch):
    scheduler = MaintenanceScheduler()

    mock_db = MagicMock()
    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", lambda: mock_db)

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_site_account_sync":
            return False
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    snapshot_mock = AsyncMock()
    apply_mock = MagicMock()
    provider_sync_mock = MagicMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteSnapshotService.get_webdav_snapshot",
        snapshot_mock,
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteAccountSyncService.apply_snapshot",
        apply_mock,
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.AllApiHubSyncService.sync_from_backup_object",
        provider_sync_mock,
    )

    await scheduler._perform_site_account_sync()

    snapshot_mock.assert_not_called()
    apply_mock.assert_not_called()
    provider_sync_mock.assert_not_called()


@pytest.mark.asyncio
async def test_site_account_sync_enabled_runs_snapshot_and_apply(monkeypatch):
    scheduler = MaintenanceScheduler()

    main_db = MagicMock()
    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", lambda: main_db)

    def fake_get_config(cls, db, key, default=None):
        data = {
            "enable_site_account_sync": True,
            "all_api_hub_webdav_url": "https://dav.example.com/backup.json",
            "all_api_hub_webdav_username": "u",
            "all_api_hub_webdav_password": "enc",
            "site_account_snapshot_cache_ttl_seconds": 120,
            "site_account_sync_apply_policy": "matched_only",
            "enable_site_account_sync_to_provider": True,
            "enable_all_api_hub_auto_create_provider_ops": False,
        }
        return data.get(key, default)

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteManagementLogService.resolve_system_password",
        staticmethod(lambda _raw: "plain-password"),
    )

    snapshot_mock = AsyncMock(
        return_value=MagicMock(
            payload={"accounts": {"accounts": []}},
            snapshot_id="snap-1",
            from_cache=True,
        )
    )
    apply_mock = MagicMock(
        return_value=MagicMock(
            total_accounts=0,
            matched_accounts=0,
            unmatched_accounts=0,
            created_accounts=0,
            updated_accounts=0,
            skipped_by_policy=0,
        )
    )
    provider_sync_mock = MagicMock(
        return_value=MagicMock(
            updated_providers=0,
        )
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteSnapshotService.get_webdav_snapshot",
        snapshot_mock,
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteAccountSyncService.apply_snapshot",
        apply_mock,
    )
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.AllApiHubSyncService.sync_from_backup_object",
        provider_sync_mock,
    )

    await scheduler._perform_site_account_sync()

    snapshot_mock.assert_awaited_once()
    _, snapshot_kwargs = snapshot_mock.await_args
    assert snapshot_kwargs["url"] == "https://dav.example.com/backup.json"
    assert snapshot_kwargs["cache_ttl_seconds"] == 120
    assert snapshot_kwargs["force_refresh"] is False

    apply_mock.assert_called_once()
    _, apply_kwargs = apply_mock.call_args
    assert apply_kwargs["apply_policy"] == "matched_only"
    assert apply_kwargs["source_snapshot_id"] == "snap-1"
    provider_sync_mock.assert_called_once()
    _, provider_kwargs = provider_sync_mock.call_args
    assert provider_kwargs["dry_run"] is False
    assert provider_kwargs["auto_create_provider_ops"] is False


@pytest.mark.asyncio
async def test_site_account_checkin_disabled(monkeypatch):
    scheduler = MaintenanceScheduler()

    mock_db = MagicMock()
    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", lambda: mock_db)

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_site_account_checkin":
            return False
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    checkin_mock = AsyncMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteAccountOpsService.checkin",
        checkin_mock,
    )

    await scheduler._perform_site_account_checkin()

    checkin_mock.assert_not_called()


@pytest.mark.asyncio
async def test_site_account_checkin_enabled_runs_for_all_accounts(monkeypatch):
    scheduler = MaintenanceScheduler()
    account_rows = [("acc-1",), ("acc-2",)]

    main_db = _FakeSession(rows=account_rows)
    worker_dbs: list[_FakeSession] = []
    create_calls = {"count": 0}

    def fake_create_session():
        create_calls["count"] += 1
        if create_calls["count"] == 1:
            return main_db
        worker = _FakeSession()
        worker_dbs.append(worker)
        return worker

    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", fake_create_session)

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_site_account_checkin":
            return True
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    collector = _ActionCollector(method_name="checkin", calls=[])

    class _FakeOpsService:
        def __init__(self, db):
            self._db = db

        async def checkin(self, account_id: str):
            return await collector.run(account_id)

    monkeypatch.setattr("src.services.system.maintenance_scheduler.SiteAccountOpsService", _FakeOpsService)

    await scheduler._perform_site_account_checkin()

    assert sorted(collector.calls) == ["acc-1", "acc-2"]
    assert main_db.closed is True
    assert len(worker_dbs) == 2
    assert all(db.closed for db in worker_dbs)


@pytest.mark.asyncio
async def test_site_account_balance_sync_disabled(monkeypatch):
    scheduler = MaintenanceScheduler()

    mock_db = MagicMock()
    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", lambda: mock_db)

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_site_account_balance_sync":
            return False
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    balance_mock = AsyncMock()
    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SiteAccountOpsService.query_balance",
        balance_mock,
    )

    await scheduler._perform_site_account_balance_sync()

    balance_mock.assert_not_called()


@pytest.mark.asyncio
async def test_site_account_balance_sync_enabled_runs_for_all_accounts(monkeypatch):
    scheduler = MaintenanceScheduler()
    account_rows = [("acc-1",), ("acc-2",)]

    main_db = _FakeSession(rows=account_rows)
    worker_dbs: list[_FakeSession] = []
    create_calls = {"count": 0}

    def fake_create_session():
        create_calls["count"] += 1
        if create_calls["count"] == 1:
            return main_db
        worker = _FakeSession()
        worker_dbs.append(worker)
        return worker

    monkeypatch.setattr("src.services.system.maintenance_scheduler.create_session", fake_create_session)

    def fake_get_config(cls, db, key, default=None):
        if key == "enable_site_account_balance_sync":
            return True
        return default

    monkeypatch.setattr(
        "src.services.system.maintenance_scheduler.SystemConfigService.get_config",
        classmethod(fake_get_config),
    )

    collector = _ActionCollector(method_name="query_balance", calls=[])

    class _FakeOpsService:
        def __init__(self, db):
            self._db = db

        async def query_balance(self, account_id: str):
            return await collector.run(account_id)

    monkeypatch.setattr("src.services.system.maintenance_scheduler.SiteAccountOpsService", _FakeOpsService)

    await scheduler._perform_site_account_balance_sync()

    assert sorted(collector.calls) == ["acc-1", "acc-2"]
    assert main_db.closed is True
    assert len(worker_dbs) == 2
    assert all(db.closed for db in worker_dbs)
