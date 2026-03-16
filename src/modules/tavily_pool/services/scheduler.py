"""Tavily Pool 调度任务。"""

from __future__ import annotations

import os

from src.core.logger import logger
from src.modules.tavily_pool.services.health_service import TavilyHealthService
from src.modules.tavily_pool.services.maintenance_service import TavilyMaintenanceService
from src.modules.tavily_pool.services.blacklist_service import TavilyBlacklistService
from src.modules.tavily_pool.services.usage_service import TavilyUsageService
from src.modules.tavily_pool.sqlite import get_session_factory
from src.services.system.scheduler import get_scheduler

HEALTH_JOB_ID = "tavily_pool_health_check"
MAINTENANCE_JOB_ID = "tavily_pool_maintenance"
USAGE_SYNC_JOB_ID = "tavily_pool_usage_sync"
BLACKLIST_SCAN_JOB_ID = "tavily_pool_blacklist_scan"


class TavilyPoolScheduler:
    def start(self) -> None:
        scheduler = get_scheduler()
        health_interval = int(os.getenv("TAVILY_POOL_HEALTH_CHECK_INTERVAL_MINUTES", "30"))
        maintenance_interval = int(os.getenv("TAVILY_POOL_MAINTENANCE_INTERVAL_MINUTES", "60"))
        usage_sync_interval = int(os.getenv("TAVILY_POOL_USAGE_SYNC_INTERVAL_MINUTES", "120"))
        blacklist_scan_interval = int(os.getenv("TAVILY_POOL_BLACKLIST_SCAN_INTERVAL_MINUTES", "1440"))

        scheduler.add_interval_job(
            self._scheduled_health_check,
            minutes=max(1, health_interval),
            job_id=HEALTH_JOB_ID,
            name="Tavily 健康检查",
        )
        scheduler.add_interval_job(
            self._scheduled_maintenance,
            minutes=max(1, maintenance_interval),
            job_id=MAINTENANCE_JOB_ID,
            name="Tavily 维护任务",
        )
        scheduler.add_interval_job(
            self._scheduled_usage_sync,
            minutes=max(1, usage_sync_interval),
            job_id=USAGE_SYNC_JOB_ID,
            name="Tavily 额度同步",
        )
        scheduler.add_interval_job(
            self._scheduled_blacklist_scan,
            minutes=max(1, blacklist_scan_interval),
            job_id=BLACKLIST_SCAN_JOB_ID,
            name="Tavily 黑名单扫描",
        )
        logger.info("Tavily Pool scheduler started")

    def stop(self) -> None:
        scheduler = get_scheduler()
        for job_id in (HEALTH_JOB_ID, MAINTENANCE_JOB_ID, USAGE_SYNC_JOB_ID, BLACKLIST_SCAN_JOB_ID):
            try:
                scheduler.remove_job(job_id)
            except Exception:
                pass
        logger.info("Tavily Pool scheduler stopped")

    @staticmethod
    async def _scheduled_health_check() -> None:
        TavilyPoolScheduler().run_health_check_once()

    @staticmethod
    async def _scheduled_maintenance() -> None:
        TavilyPoolScheduler().run_maintenance_once()

    @staticmethod
    async def _scheduled_usage_sync() -> None:
        TavilyPoolScheduler().run_usage_sync_once()

    @staticmethod
    async def _scheduled_blacklist_scan() -> None:
        TavilyPoolScheduler().run_blacklist_scan_once()

    def run_health_check_once(self) -> dict[str, int]:
        session_factory = get_session_factory()
        with session_factory() as db:
            return TavilyHealthService(db).run_health_check()

    def run_maintenance_once(self) -> dict[str, int | str]:
        session_factory = get_session_factory()
        with session_factory() as db:
            run = TavilyMaintenanceService(db).run_maintenance(job_name="scheduler")
            return {
                "run_id": run.id,
                "total": run.total,
                "success": run.success,
                "failed": run.failed,
                "skipped": run.skipped,
                "status": run.status,
            }

    def run_usage_sync_once(self) -> dict[str, int]:
        session_factory = get_session_factory()
        with session_factory() as db:
            return TavilyUsageService(db).run_usage_sync()

    def run_blacklist_scan_once(self) -> dict[str, int]:
        session_factory = get_session_factory()
        with session_factory() as db:
            return TavilyBlacklistService(db).scan_and_cleanup()
