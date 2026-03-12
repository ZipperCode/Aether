"""Site management scheduled tasks."""
from __future__ import annotations

import asyncio
from datetime import datetime, timezone

from src.core.logger import logger


SITE_ACCOUNT_SYNC_JOB_ID = "site_account_sync"
SITE_ACCOUNT_BALANCE_SYNC_JOB_ID = "site_account_balance_sync"
SITE_ACCOUNT_CHECKIN_JOB_PREFIX = "site_account_checkin:"


def _source_checkin_job_id(source_id: str) -> str:
    return f"{SITE_ACCOUNT_CHECKIN_JOB_PREFIX}{source_id}"


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

        balance_hour, balance_minute = self._get_time_config("site_account_balance_sync_time", 5, 0)
        scheduler.add_cron_job(
            self._scheduled_site_account_balance_sync,
            hour=balance_hour, minute=balance_minute,
            job_id=SITE_ACCOUNT_BALANCE_SYNC_JOB_ID,
            name="站点账号余额同步",
        )

        self.refresh_all_source_checkin_jobs()

    def stop(self) -> None:
        """Remove all site management cron jobs."""
        from src.services.system.scheduler import get_scheduler

        scheduler = get_scheduler()
        for job_id in (SITE_ACCOUNT_SYNC_JOB_ID, SITE_ACCOUNT_BALANCE_SYNC_JOB_ID):
            try:
                scheduler.scheduler.remove_job(job_id)
            except Exception:
                pass
        self._remove_source_checkin_jobs(scheduler)

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
    async def _scheduled_site_account_checkin_for_source(source_id: str) -> None:
        scheduler = SiteManagementScheduler()
        await scheduler._perform_site_account_checkin(source_id=source_id)

    @staticmethod
    async def _scheduled_site_account_balance_sync() -> None:
        scheduler = SiteManagementScheduler()
        await scheduler._perform_site_account_balance_sync()

    @staticmethod
    def _parse_time_string(time_str: object, default_hour: int, default_minute: int) -> tuple[int, int]:
        if not isinstance(time_str, str):
            return default_hour, default_minute
        raw = time_str.strip()
        if ":" not in raw:
            return default_hour, default_minute
        parts = raw.split(":", 1)
        try:
            hour = int(parts[0])
            minute = int(parts[1])
        except ValueError:
            return default_hour, default_minute
        if 0 <= hour <= 23 and 0 <= minute <= 59:
            return hour, minute
        return default_hour, default_minute

    @staticmethod
    def _remove_source_checkin_jobs(scheduler: object) -> None:
        jobs = getattr(getattr(scheduler, "scheduler", None), "get_jobs", lambda: [])()
        for job in jobs:
            job_id = getattr(job, "id", "")
            if isinstance(job_id, str) and job_id.startswith(SITE_ACCOUNT_CHECKIN_JOB_PREFIX):
                try:
                    scheduler.scheduler.remove_job(job_id)
                except Exception:
                    pass

    @staticmethod
    def _checkin_item_status(status: object) -> str:
        from src.services.provider_ops.types import ActionStatus

        if status == ActionStatus.SUCCESS:
            return "success"
        if status in {
            ActionStatus.ALREADY_DONE,
            ActionStatus.NOT_SUPPORTED,
            ActionStatus.NOT_CONFIGURED,
        }:
            return "skipped"
        return "failed"

    @staticmethod
    def _checkin_run_status(success_count: int, failed_count: int, skipped_count: int) -> str:
        if failed_count == 0 and skipped_count == 0:
            return "success"
        if success_count == 0 and skipped_count == 0 and failed_count > 0:
            return "failed"
        return "partial"

    def refresh_all_source_checkin_jobs(self) -> None:
        from src.database.database import create_session
        from src.modules.site_management.models import WebDavSource
        from src.services.system.scheduler import get_scheduler

        scheduler = get_scheduler()
        self._remove_source_checkin_jobs(scheduler)

        db = create_session()
        try:
            sources = (
                db.query(WebDavSource)
                .filter(
                    WebDavSource.is_active.is_(True),
                    WebDavSource.checkin_enabled.is_(True),
                )
                .all()
            )
            for source in sources:
                source_id = str(source.id)
                hour, minute = self._parse_time_string(source.checkin_time, 4, 0)
                scheduler.add_cron_job(
                    self._scheduled_site_account_checkin_for_source,
                    hour=hour,
                    minute=minute,
                    job_id=_source_checkin_job_id(source_id),
                    name=f"站点账号签到:{source.name}",
                    source_id=source_id,
                )
        finally:
            db.close()

    def refresh_source_checkin_job(self, source_id: str) -> None:
        from src.database.database import create_session
        from src.modules.site_management.models import WebDavSource
        from src.services.system.scheduler import get_scheduler

        scheduler = get_scheduler()
        job_id = _source_checkin_job_id(source_id)
        try:
            scheduler.scheduler.remove_job(job_id)
        except Exception:
            pass

        db = create_session()
        try:
            source = (
                db.query(WebDavSource)
                .filter(
                    WebDavSource.id == source_id,
                    WebDavSource.is_active.is_(True),
                    WebDavSource.checkin_enabled.is_(True),
                )
                .first()
            )
            if source is None:
                return

            hour, minute = self._parse_time_string(source.checkin_time, 4, 0)
            scheduler.add_cron_job(
                self._scheduled_site_account_checkin_for_source,
                hour=hour,
                minute=minute,
                job_id=job_id,
                name=f"站点账号签到:{source.name}",
                source_id=source_id,
            )
        finally:
            db.close()

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

    async def _perform_site_account_checkin(
        self,
        source_id: str | None = None,
        ignore_enabled: bool = False,
    ) -> None:
        """Execute checkin for all enabled site accounts."""
        from src.database.database import create_session
        from src.modules.site_management.models import SiteAccount
        from src.modules.site_management.services.account_ops_service import AccountOpsService
        from src.modules.site_management.services.log_service import (
            CheckinItemLog,
            SiteManagementLogService,
        )
        from src.services.provider_ops.types import ActionStatus

        db = create_session()
        try:
            query = db.query(SiteAccount).filter(
                SiteAccount.is_active.is_(True),
                SiteAccount.checkin_enabled.is_(True),
            )
            if source_id:
                query = query.filter(SiteAccount.webdav_source_id == source_id)

            accounts = query.all()
            if not accounts:
                logger.info("无可签到的站点账号，跳过 source_id={}", source_id or "all")
                return
            account_infos = [
                {
                    "account_id": str(account.id),
                    "account_domain": account.domain,
                    "account_site_url": account.site_url,
                    "webdav_source_id": str(account.webdav_source_id)
                    if account.webdav_source_id
                    else None,
                }
                for account in accounts
            ]
        finally:
            db.close()

        started_at = datetime.now(timezone.utc)
        semaphore = asyncio.Semaphore(3)

        async def _run(account_info: dict[str, str | None]) -> CheckinItemLog:
            async with semaphore:
                task_db = create_session()
                account_id = str(account_info["account_id"])
                try:
                    service = AccountOpsService(task_db)
                    result = await service.checkin(account_id)
                    item_status = self._checkin_item_status(result.status)
                    if result.status != ActionStatus.SUCCESS:
                        logger.debug(
                            "站点账号签到结果: account_id={}, status={}, message={}",
                            account_id, result.status.value, result.message,
                        )
                    balance_total, balance_currency = AccountOpsService._extract_balance_total_and_currency(
                        result.data
                    )
                    return CheckinItemLog(
                        provider_id=account_id,
                        provider_name=None,
                        provider_domain=account_info["account_domain"],
                        account_id=account_id,
                        account_domain=account_info["account_domain"],
                        account_site_url=account_info["account_site_url"],
                        status=item_status,
                        message=result.message or "",
                        balance_total=balance_total,
                        balance_currency=balance_currency,
                    )
                except Exception as exc:
                    logger.warning("站点账号签到失败: account_id={}, error={}", account_id, exc)
                    return CheckinItemLog(
                        provider_id=account_id,
                        provider_name=None,
                        provider_domain=account_info["account_domain"],
                        account_id=account_id,
                        account_domain=account_info["account_domain"],
                        account_site_url=account_info["account_site_url"],
                        status="failed",
                        message=str(exc),
                    )
                finally:
                    try:
                        task_db.close()
                    except Exception:
                        pass

        items = await asyncio.gather(*[_run(account_info) for account_info in account_infos])
        success_count = sum(1 for item in items if item.status == "success")
        failed_count = sum(1 for item in items if item.status == "failed")
        skipped_count = sum(1 for item in items if item.status == "skipped")

        log_db = create_session()
        try:
            SiteManagementLogService.record_checkin_run(
                log_db,
                trigger_source="scheduled",
                status=self._checkin_run_status(success_count, failed_count, skipped_count),
                webdav_source_id=source_id,
                total_providers=len(items),
                success_count=success_count,
                failed_count=failed_count,
                skipped_count=skipped_count,
                items=items,
                started_at=started_at,
            )
        except Exception as exc:
            logger.warning("记录站点账号签到日志失败: source_id={}, error={}", source_id or "all", exc)
        finally:
            try:
                log_db.close()
            except Exception:
                pass

        logger.info(
            "站点账号签到任务完成: source_id={}, total={}",
            source_id or "all",
            len(account_infos),
        )

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
