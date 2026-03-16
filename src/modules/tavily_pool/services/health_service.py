"""Tavily 账号池健康检查服务。"""

from __future__ import annotations

from datetime import datetime, timezone

from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyHealthCheck, TavilyToken


class TavilyHealthService:
    def __init__(self, db: Session) -> None:
        self.db = db

    def run_health_check(self) -> dict[str, int]:
        accounts = self.db.query(TavilyAccount).all()
        success = 0
        failed = 0

        for account in accounts:
            has_active_token = (
                self.db.query(TavilyToken)
                .filter(
                    TavilyToken.account_id == account.id,
                    TavilyToken.is_active.is_(True),
                )
                .first()
                is not None
            )

            status = "success" if has_active_token else "failed"
            item = TavilyHealthCheck(
                account_id=account.id,
                check_type="account",
                status=status,
                error_message=None if has_active_token else "missing active token",
                checked_at=datetime.now(timezone.utc),
            )
            self.db.add(item)

            account.health_checked_at = datetime.now(timezone.utc)
            if has_active_token:
                account.health_status = "ok"
                account.fail_count = 0
                success += 1
            else:
                account.health_status = "fail"
                account.fail_count = int(account.fail_count or 0) + 1
                failed += 1

        self.db.commit()
        return {"total": len(accounts), "success": success, "failed": failed}
