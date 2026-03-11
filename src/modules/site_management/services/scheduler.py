"""Site management scheduled tasks."""
from __future__ import annotations

import asyncio
from datetime import datetime, timezone

from src.core.logger import logger


SITE_ACCOUNT_SYNC_JOB_ID = "site_account_sync"
SITE_ACCOUNT_CHECKIN_JOB_ID = "site_account_checkin"
SITE_ACCOUNT_BALANCE_SYNC_JOB_ID = "site_account_balance_sync"


class SiteManagementScheduler:
    """Manages scheduled tasks for the site management module."""

    def start(self) -> None:
        """Register all site management cron jobs."""
        from src.services.system.scheduler import get_scheduler

        scheduler = get_scheduler()

        sync_hour, sync_minute = self._get_time_config("site_account_sync_time", 3, 0)
        scheduler.add_cron_job(
            self._scheduled_site_account_sync,
            hour=sync_hour, minute=sync_minute,
            job_id=SITE_ACCOUNT_SYNC_JOB_ID,
            name="站点账号同步",
        )

        checkin_hour, checkin_minute = self._get_time_config("site_account_checkin_time", 4, 0)
        scheduler.add_cron_job(
            self._scheduled_site_account_checkin,
            hour=checkin_hour, minute=checkin_minute,
            job_id=SITE_ACCOUNT_CHECKIN_JOB_ID,
            name="站点账号签到",
        )

        balance_hour, balance_minute = self._get_time_config("site_account_balance_sync_time", 5, 0)
        scheduler.add_cron_job(
            self._scheduled_site_account_balance_sync,
            hour=balance_hour, minute=balance_minute,
            job_id=SITE_ACCOUNT_BALANCE_SYNC_JOB_ID,
            name="站点账号余额同步",
        )

    def stop(self) -> None:
        """Remove all site management cron jobs."""
        from src.services.system.scheduler import get_scheduler

        scheduler = get_scheduler()
        for job_id in (SITE_ACCOUNT_SYNC_JOB_ID, SITE_ACCOUNT_CHECKIN_JOB_ID, SITE_ACCOUNT_BALANCE_SYNC_JOB_ID):
            try:
                scheduler.scheduler.remove_job(job_id)
            except Exception:
                pass

    @staticmethod
    def _get_time_config(key: str, default_hour: int, default_minute: int) -> tuple[int, int]:
        """Read time config from SystemConfig."""
        try:
            from src.database.database import create_session
            from src.services.system.config import SystemConfigService

            db = create_session()
            try:
                value = SystemConfigService.get_config(db, key)
                if value and isinstance(value, str) and ":" in value:
                    parts = value.split(":")
                    return int(parts[0]), int(parts[1])
            finally:
                db.close()
        except Exception:
            pass
        return default_hour, default_minute

    @staticmethod
    async def _scheduled_site_account_sync() -> None:
        """Wrapper for scheduled sync execution."""
        scheduler = SiteManagementScheduler()
        await scheduler._perform_site_account_sync()

    @staticmethod
    async def _scheduled_site_account_checkin() -> None:
        scheduler = SiteManagementScheduler()
        await scheduler._perform_site_account_checkin()

    @staticmethod
    async def _scheduled_site_account_balance_sync() -> None:
        scheduler = SiteManagementScheduler()
        await scheduler._perform_site_account_balance_sync()

    async def _perform_site_account_sync(self) -> None:
        """Execute site account sync for all active WebDav sources."""
        from src.database.database import create_session
        from src.services.system.config import SystemConfigService
        from src.modules.site_management.models import WebDavSource
        from src.modules.site_management.services.snapshot_service import SiteSnapshotService
        from src.modules.site_management.services.account_sync_service import AccountSyncService
        from src.modules.site_management.services.log_service import SiteManagementLogService

        db = create_session()
        try:
            sources = (
                db.query(WebDavSource)
                .filter(
                    WebDavSource.is_active.is_(True),
                    WebDavSource.sync_enabled.is_(True),
                )
                .all()
            )
            if not sources:
                logger.info("无活跃的 WebDav 源，跳过站点账号同步")
                return

            cache_ttl = int(
                SystemConfigService.get_config(db, "site_account_snapshot_cache_ttl_seconds", 300)
            )

            for source in sources:
                started_at = datetime.now(timezone.utc)
                try:
                    snapshot = await SiteSnapshotService().get_webdav_snapshot(
                        db,
                        webdav_source_id=str(source.id),
                        cache_ttl_seconds=max(0, cache_ttl),
                        force_refresh=False,
                    )
                    result = AccountSyncService().apply_snapshot(
                        db,
                        snapshot=snapshot.payload,
                        webdav_source_id=str(source.id),
                    )

                    source.last_sync_at = datetime.now(timezone.utc)
                    source.last_sync_status = "success"
                    db.commit()

                    SiteManagementLogService.record_sync_run(
                        db,
                        trigger_source="scheduled",
                        status="success",
                        webdav_source_id=str(source.id),
                        total_accounts=result.total_accounts,
                        created_accounts=result.created_accounts,
                        updated_accounts=result.updated_accounts,
                        started_at=started_at,
                    )

                    logger.info(
                        "WebDav 源 {} 同步完成: total={}, created={}, updated={}",
                        source.name, result.total_accounts,
                        result.created_accounts, result.updated_accounts,
                    )
                except Exception as e:
                    logger.exception("WebDav 源 {} 同步失败: {}", source.name, e)
                    try:
                        source.last_sync_status = "failed"
                        db.commit()
                    except Exception:
                        db.rollback()
        except Exception as e:
            logger.exception("站点账号同步任务执行失败: {}", e)
        finally:
            db.close()

    async def _perform_site_account_checkin(self, ignore_enabled: bool = False) -> None:
        """Execute checkin for all enabled site accounts."""
        from src.database.database import create_session
        from src.services.system.config import SystemConfigService
        from src.modules.site_management.models import SiteAccount
        from src.modules.site_management.services.account_ops_service import AccountOpsService
        from src.services.provider_ops.types import ActionStatus

        db = create_session()
        try:
            if (
                not ignore_enabled
                and not SystemConfigService.get_config(db, "enable_site_account_checkin", False)
            ):
                logger.info("站点账号签到任务未启用，跳过")
                return

            account_ids = [
                account_id
                for (account_id,) in (
                    db.query(SiteAccount.id)
                    .filter(
                        SiteAccount.is_active.is_(True),
                        SiteAccount.checkin_enabled.is_(True),
                    )
                    .all()
                )
            ]
            if not account_ids:
                logger.info("无可签到的站点账号，跳过")
                return
        finally:
            db.close()

        semaphore = asyncio.Semaphore(3)

        async def _run(account_id: str) -> None:
            async with semaphore:
                task_db = create_session()
                try:
                    service = AccountOpsService(task_db)
                    result = await service.checkin(account_id)
                    if result.status != ActionStatus.SUCCESS:
                        logger.debug(
                            "站点账号签到结果: account_id={}, status={}, message={}",
                            account_id, result.status.value, result.message,
                        )
                except Exception as exc:
                    logger.warning("站点账号签到失败: account_id={}, error={}", account_id, exc)
                finally:
                    try:
                        task_db.close()
                    except Exception:
                        pass

        await asyncio.gather(*[_run(aid) for aid in account_ids])
        logger.info("站点账号签到任务完成: total={}", len(account_ids))

    async def _perform_site_account_balance_sync(self) -> None:
        """Execute balance sync for all enabled site accounts."""
        from src.database.database import create_session
        from src.services.system.config import SystemConfigService
        from src.modules.site_management.models import SiteAccount
        from src.modules.site_management.services.account_ops_service import AccountOpsService
        from src.services.provider_ops.types import ActionStatus

        db = create_session()
        try:
            if not SystemConfigService.get_config(db, "enable_site_account_balance_sync", False):
                logger.info("站点账号余额同步任务未启用，跳过")
                return

            account_ids = [
                account_id
                for (account_id,) in (
                    db.query(SiteAccount.id)
                    .filter(
                        SiteAccount.is_active.is_(True),
                        SiteAccount.balance_sync_enabled.is_(True),
                    )
                    .all()
                )
            ]
            if not account_ids:
                logger.info("无可同步余额的站点账号，跳过")
                return
        finally:
            db.close()

        semaphore = asyncio.Semaphore(3)

        async def _run(account_id: str) -> None:
            async with semaphore:
                task_db = create_session()
                try:
                    service = AccountOpsService(task_db)
                    result = await service.query_balance(account_id)
                    if result.status != ActionStatus.SUCCESS:
                        logger.debug(
                            "站点账号余额同步结果: account_id={}, status={}, message={}",
                            account_id, result.status.value, result.message,
                        )
                except Exception as exc:
                    logger.warning("站点账号余额同步失败: account_id={}, error={}", account_id, exc)
                finally:
                    try:
                        task_db.close()
                    except Exception:
                        pass

        await asyncio.gather(*[_run(aid) for aid in account_ids])
        logger.info("站点账号余额同步任务完成: total={}", len(account_ids))
