"""Tavily 账号池维护服务。"""

from __future__ import annotations

from datetime import datetime, timezone

from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyMaintenanceItem, TavilyMaintenanceRun


class TavilyMaintenanceService:
    def __init__(self, db: Session) -> None:
        self.db = db

    def run_maintenance(self, *, job_name: str = "manual") -> TavilyMaintenanceRun:
        run = TavilyMaintenanceRun(
            job_name=job_name,
            status="success",
            started_at=datetime.now(timezone.utc),
        )
        self.db.add(run)
        self.db.flush()

        accounts = self.db.query(TavilyAccount).all()
        run.total = len(accounts)

        for account in accounts:
            # 简化策略：连续失败 >=3 标记为 disabled
            if int(account.fail_count or 0) >= 3:
                account.status = "disabled"
                item_status = "success"
                message = "account disabled due to continuous health check failures"
                run.success += 1
            else:
                item_status = "skipped"
                message = "no maintenance action required"
                run.skipped += 1

            self.db.add(
                TavilyMaintenanceItem(
                    run_id=run.id,
                    account_id=account.id,
                    status=item_status,
                    message=message,
                )
            )

        run.finished_at = datetime.now(timezone.utc)
        self.db.commit()
        self.db.refresh(run)
        return run
