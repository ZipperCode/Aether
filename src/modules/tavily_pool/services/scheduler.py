"""Tavily Pool 调度任务。"""

from __future__ import annotations

import os

from src.core.logger import logger
from src.modules.tavily_pool.services.health_service import TavilyHealthService
from src.modules.tavily_pool.services.maintenance_service import TavilyMaintenanceService
from src.modules.tavily_pool.sqlite import get_session_factory
from src.services.system.scheduler import get_scheduler

HEALTH_JOB_ID = "tavily_pool_health_check"
MAINTENANCE_JOB_ID = "tavily_pool_maintenance"


class TavilyPoolScheduler:
    def start(self) -> None:
        scheduler = get_scheduler()
        health_interval = int(os.getenv("TAVILY_POOL_HEALTH_CHECK_INTERVAL_MINUTES", "30"))
        maintenance_interval = int(os.getenv("TAVILY_POOL_MAINTENANCE_INTERVAL_MINUTES", "60"))

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
        logger.info("Tavily Pool scheduler started")

    def stop(self) -> None:
        scheduler = get_scheduler()
        for job_id in (HEALTH_JOB_ID, MAINTENANCE_JOB_ID):
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
