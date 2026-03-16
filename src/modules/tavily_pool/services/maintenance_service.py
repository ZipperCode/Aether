"""Tavily 账号池维护服务。"""

from __future__ import annotations

from datetime import datetime, timezone

from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyBlacklistState, TavilyMaintenanceItem, TavilyMaintenanceRun


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
        blacklisted_account_ids = {
            row[0]
            for row in (
                self.db.query(TavilyBlacklistState.account_id)
                .filter(TavilyBlacklistState.status == "active")
                .all()
            )
        }
        run.total = len(accounts)

        for account in accounts:
            # 当前维护阶段不再按连续失败直接禁用账号，交由黑名单策略处理。
            if account.id in blacklisted_account_ids:
                item_status = "success"
                message = "account is in blacklist and managed by blacklist scanner"
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
