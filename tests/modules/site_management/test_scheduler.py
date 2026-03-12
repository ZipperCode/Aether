"""Tests for SiteManagementScheduler."""
from __future__ import annotations

import sys
from types import ModuleType
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.modules.site_management.services.account_sync_service import AccountSyncResult
from src.modules.site_management.services.scheduler import SiteManagementScheduler
from src.services.provider_ops.types import ActionStatus


# ---------------------------------------------------------------------------
# Ensure src.services.system.config is importable.
#
# The __init__.py of src.services.system triggers a deep import chain that
# may be broken in the test environment.  We attempt the import once; if it
# fails we seed sys.modules so that subsequent ``from … import`` inside the
# code under test resolves to a stub that the individual tests will patch.
# ---------------------------------------------------------------------------

try:
    from src.services.system.config import SystemConfigService as _RealCfg  # noqa: F401
except Exception:
    _cfg_mod = ModuleType("src.services.system.config")
    _cfg_mod.SystemConfigService = MagicMock()  # type: ignore[attr-defined]
    sys.modules.setdefault("src.services.system", ModuleType("src.services.system"))
    sys.modules.setdefault("src.services.system.config", _cfg_mod)


# ---------------------------------------------------------------------------
# Patch targets
# ---------------------------------------------------------------------------

_CREATE_SESSION = "src.database.database.create_session"
_SYSTEM_CONFIG = "src.services.system.config.SystemConfigService"
_SNAPSHOT_SVC = "src.modules.site_management.services.snapshot_service.SiteSnapshotService"
_SYNC_SVC = "src.modules.site_management.services.account_sync_service.AccountSyncService"
_LOG_SVC = "src.modules.site_management.services.log_service.SiteManagementLogService"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_source(
    source_id: str,
    name: str,
    *,
    is_active: bool = True,
    sync_enabled: bool = True,
    checkin_enabled: bool = True,
    checkin_time: str = "04:00",
) -> MagicMock:
    """Create a mock WebDavSource."""
    src = MagicMock()
    src.id = source_id
    src.name = name
    src.is_active = is_active
    src.sync_enabled = sync_enabled
    src.checkin_enabled = checkin_enabled
    src.checkin_time = checkin_time
    src.last_sync_at = None
    src.last_sync_status = None
    return src


def _make_snapshot_result(payload: dict | None = None) -> MagicMock:
    """Create a mock SnapshotFetchResult."""
    result = MagicMock()
    result.payload = payload or {}
    return result


def _make_sync_result(total: int = 0, created: int = 0, updated: int = 0) -> AccountSyncResult:
    return AccountSyncResult(
        total_accounts=total,
        created_accounts=created,
        updated_accounts=updated,
    )


# ---------------------------------------------------------------------------
# Tests: _perform_site_account_sync
# ---------------------------------------------------------------------------


class TestSyncIteratesAllActiveSources:
    """Verify that sync visits every active & sync-enabled source."""

    @pytest.mark.asyncio
    async def test_sync_iterates_all_active_sources(self) -> None:
        sources = [
            _make_source("s1", "Source-1"),
            _make_source("s2", "Source-2"),
            _make_source("s3", "Source-3"),
        ]

        mock_db = MagicMock()
        mock_db.query.return_value.filter.return_value.all.return_value = sources

        mock_snapshot_instance = AsyncMock()
        mock_snapshot_instance.get_webdav_snapshot = AsyncMock(
            return_value=_make_snapshot_result()
        )

        mock_sync_instance = MagicMock()
        mock_sync_instance.apply_snapshot = MagicMock(
            return_value=_make_sync_result(total=5, created=2, updated=3)
        )

        with (
            patch(_CREATE_SESSION, return_value=mock_db),
            patch(_SYSTEM_CONFIG) as mock_config_cls,
            patch(_SNAPSHOT_SVC, return_value=mock_snapshot_instance),
            patch(_SYNC_SVC, return_value=mock_sync_instance),
            patch(_LOG_SVC) as mock_log_svc,
        ):
            mock_config_cls.get_config.return_value = 300

            scheduler = SiteManagementScheduler()
            await scheduler._perform_site_account_sync()

            # Snapshot + sync called once per source
            assert mock_snapshot_instance.get_webdav_snapshot.call_count == 3
            assert mock_sync_instance.apply_snapshot.call_count == 3

            # Log service called once per source
            assert mock_log_svc.record_sync_run.call_count == 3

        # Each source should have been marked success
        for src in sources:
            assert src.last_sync_status == "success"


class TestSyncSkipsInactiveSources:
    """Source with is_active=False should not appear in the query results."""

    @pytest.mark.asyncio
    async def test_sync_skips_when_no_active_sources(self) -> None:
        mock_db = MagicMock()
        # The DB query already filters inactive sources; returning empty list
        mock_db.query.return_value.filter.return_value.all.return_value = []

        mock_snapshot_instance = AsyncMock()
        mock_sync_instance = MagicMock()

        with (
            patch(_CREATE_SESSION, return_value=mock_db),
            patch(_SYSTEM_CONFIG),
            patch(_SNAPSHOT_SVC, return_value=mock_snapshot_instance),
            patch(_SYNC_SVC, return_value=mock_sync_instance),
        ):
            scheduler = SiteManagementScheduler()
            await scheduler._perform_site_account_sync()

            # Nothing should have been called
            mock_snapshot_instance.get_webdav_snapshot.assert_not_called()
            mock_sync_instance.apply_snapshot.assert_not_called()


class TestSingleSourceFailureContinues:
    """If one source raises, others should still be processed."""

    @pytest.mark.asyncio
    async def test_single_source_failure_continues(self) -> None:
        sources = [
            _make_source("s1", "Source-1"),
            _make_source("s2", "Source-2-FAIL"),
            _make_source("s3", "Source-3"),
        ]

        mock_db = MagicMock()
        mock_db.query.return_value.filter.return_value.all.return_value = sources

        call_count = 0

        async def _snapshot_side_effect(db, *, webdav_source_id, cache_ttl_seconds, force_refresh):
            nonlocal call_count
            call_count += 1
            if webdav_source_id == "s2":
                raise RuntimeError("WebDav download failed")
            return _make_snapshot_result()

        mock_snapshot_instance = AsyncMock()
        mock_snapshot_instance.get_webdav_snapshot = AsyncMock(side_effect=_snapshot_side_effect)

        mock_sync_instance = MagicMock()
        mock_sync_instance.apply_snapshot = MagicMock(
            return_value=_make_sync_result(total=1, created=1)
        )

        with (
            patch(_CREATE_SESSION, return_value=mock_db),
            patch(_SYSTEM_CONFIG) as mock_config_cls,
            patch(_SNAPSHOT_SVC, return_value=mock_snapshot_instance),
            patch(_SYNC_SVC, return_value=mock_sync_instance),
            patch(_LOG_SVC) as mock_log_svc,
        ):
            mock_config_cls.get_config.return_value = 300

            scheduler = SiteManagementScheduler()
            await scheduler._perform_site_account_sync()

            # All 3 sources attempted
            assert call_count == 3

            # Only 2 succeeded (s1, s3)
            assert mock_sync_instance.apply_snapshot.call_count == 2
            assert mock_log_svc.record_sync_run.call_count == 2

        # The failed source should be marked failed
        assert sources[1].last_sync_status == "failed"

        # Successful sources should be marked success
        assert sources[0].last_sync_status == "success"
        assert sources[2].last_sync_status == "success"


class TestCheckinJobsPerSource:
    """Checkin jobs should be scheduled per active webdav source."""

    def test_start_registers_source_specific_checkin_jobs(self) -> None:
        sources = [
            _make_source("s1", "Source-1", checkin_enabled=True, checkin_time="06:10"),
            _make_source("s2", "Source-2", checkin_enabled=True, checkin_time="07:20"),
        ]

        mock_db = MagicMock()
        mock_db.query.return_value.filter.return_value.all.return_value = sources

        mock_scheduler = MagicMock()

        with (
            patch("src.services.system.scheduler.get_scheduler", return_value=mock_scheduler),
            patch(_CREATE_SESSION, return_value=mock_db),
            patch.object(SiteManagementScheduler, "_get_time_config", return_value=(3, 0)),
        ):
            scheduler = SiteManagementScheduler()
            scheduler.start()

        job_ids = [call.kwargs.get("job_id") for call in mock_scheduler.add_cron_job.call_args_list]
        assert "site_account_checkin:s1" in job_ids
        assert "site_account_checkin:s2" in job_ids
        assert "site_account_checkin" not in job_ids

    def test_refresh_source_checkin_job_rebuilds_existing_job(self) -> None:
        source = _make_source("s1", "Source-1", checkin_enabled=True, checkin_time="08:45")

        mock_db = MagicMock()
        mock_db.query.return_value.filter.return_value.first.return_value = source

        mock_scheduler = MagicMock()

        with (
            patch("src.services.system.scheduler.get_scheduler", return_value=mock_scheduler),
            patch(_CREATE_SESSION, return_value=mock_db),
        ):
            scheduler = SiteManagementScheduler()
            scheduler.refresh_source_checkin_job("s1")

        mock_scheduler.scheduler.remove_job.assert_called_once_with("site_account_checkin:s1")
        mock_scheduler.add_cron_job.assert_called_once()
        assert mock_scheduler.add_cron_job.call_args.kwargs["job_id"] == "site_account_checkin:s1"
        assert mock_scheduler.add_cron_job.call_args.kwargs["hour"] == 8
        assert mock_scheduler.add_cron_job.call_args.kwargs["minute"] == 45
        assert mock_scheduler.add_cron_job.call_args.kwargs["source_id"] == "s1"

    @pytest.mark.asyncio
    async def test_perform_checkin_only_processes_accounts_for_source(self) -> None:
        select_db = MagicMock()
        base_query = MagicMock()
        active_query = MagicMock()
        source_query = MagicMock()
        account = MagicMock()
        account.id = "a-source"
        account.domain = "demo.example"
        account.site_url = "https://demo.example"
        account.webdav_source_id = "source-1"
        select_db.query.return_value = base_query
        base_query.filter.return_value = active_query
        active_query.filter.return_value = source_query
        source_query.all.return_value = [account]

        task_db = MagicMock()
        log_db = MagicMock()
        ops_service = MagicMock()
        ops_service.checkin = AsyncMock(return_value=MagicMock(status=ActionStatus.SUCCESS))

        with patch(_CREATE_SESSION, side_effect=[select_db, task_db, log_db]), patch(
            "src.modules.site_management.services.account_ops_service.AccountOpsService"
        ) as account_ops_cls:
            account_ops_cls.return_value = ops_service
            account_ops_cls._extract_balance_total_and_currency.return_value = (None, None)
            scheduler = SiteManagementScheduler()
            await scheduler._perform_site_account_checkin(source_id="source-1")

        active_query.filter.assert_called_once()
        ops_service.checkin.assert_awaited_once_with("a-source")

    @pytest.mark.asyncio
    async def test_perform_checkin_records_source_scoped_log(self) -> None:
        select_db = MagicMock()
        base_query = MagicMock()
        active_query = MagicMock()
        source_query = MagicMock()
        account = MagicMock()
        account.id = "a-source"
        account.domain = "demo.example"
        account.site_url = "https://demo.example"
        account.webdav_source_id = "source-1"
        select_db.query.return_value = base_query
        base_query.filter.return_value = active_query
        active_query.filter.return_value = source_query
        source_query.all.return_value = [account]

        task_db = MagicMock()
        log_db = MagicMock()
        ops_service = MagicMock()
        ops_service.checkin = AsyncMock(
            return_value=MagicMock(status=ActionStatus.SUCCESS, message="checked in", data=None)
        )

        with (
            patch(_CREATE_SESSION, side_effect=[select_db, task_db, log_db]),
            patch("src.modules.site_management.services.account_ops_service.AccountOpsService") as account_ops_cls,
            patch(_LOG_SVC) as mock_log_svc,
        ):
            account_ops_cls.return_value = ops_service
            account_ops_cls._extract_balance_total_and_currency.return_value = (None, None)
            scheduler = SiteManagementScheduler()
            await scheduler._perform_site_account_checkin(source_id="source-1")

        mock_log_svc.record_checkin_run.assert_called_once()
        kwargs = mock_log_svc.record_checkin_run.call_args.kwargs
        assert kwargs["webdav_source_id"] == "source-1"
        assert kwargs["items"][0].account_id == "a-source"
        assert kwargs["items"][0].account_domain == "demo.example"
        assert kwargs["items"][0].account_site_url == "https://demo.example"
