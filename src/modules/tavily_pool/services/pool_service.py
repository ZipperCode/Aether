"""Tavily token 池服务：轮询、失败摘除与调用统计。"""

from __future__ import annotations

import os
from threading import Lock
from typing import Any

from sqlalchemy import func, or_
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyAccountRuntimeState, TavilyToken, TavilyUsageLog
from src.modules.tavily_pool.schemas import TavilyPoolLeaseRead
from src.modules.tavily_pool.services.blacklist_service import TavilyBlacklistService
from src.modules.tavily_pool.services.crypto import TavilyCryptoService

_cursor_lock = Lock()
_next_index = 0


class TavilyPoolService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.crypto = TavilyCryptoService()
        self.max_fails = int(os.getenv("TAVILY_POOL_TOKEN_MAX_FAILS", "3"))
        self.blacklist = TavilyBlacklistService(db)

    def lease_token(self) -> TavilyPoolLeaseRead:
        global _next_index

        active_rows = (
            self.db.query(TavilyToken, TavilyAccount, TavilyAccountRuntimeState)
            .join(TavilyAccount, TavilyAccount.id == TavilyToken.account_id)
            .outerjoin(TavilyAccountRuntimeState, TavilyAccountRuntimeState.account_id == TavilyAccount.id)
            .filter(
                TavilyToken.is_active.is_(True),
                or_(
                    TavilyAccountRuntimeState.account_id.is_(None),
                    TavilyAccountRuntimeState.status == "active",
                ),
            )
            .order_by(TavilyToken.created_at.asc())
            .all()
        )
        if not active_rows:
            raise ValueError("No active token available")

        with _cursor_lock:
            index = _next_index % len(active_rows)
            _next_index = (index + 1) % len(active_rows)

        token, account, _ = active_rows[index]
        return TavilyPoolLeaseRead(
            account_id=account.id,
            token_id=token.id,
            token=self.crypto.decrypt(token.token_encrypted),
            token_masked=token.token_masked,
        )

    def report_result(
        self,
        *,
        token_id: str,
        success: bool,
        endpoint: str = "",
        latency_ms: int | None = None,
        error_message: str | None = None,
    ) -> dict[str, Any]:
        token = self.db.get(TavilyToken, token_id)
        if token is None:
            raise ValueError("Token not found")

        log = TavilyUsageLog(
            account_id=token.account_id,
            token_id=token.id,
            endpoint=endpoint,
            success=success,
            latency_ms=latency_ms,
            error_message=error_message,
        )
        self.db.add(log)

        account = self.db.get(TavilyAccount, token.account_id)
        runtime = self._get_or_create_runtime_state(token.account_id)
        if success:
            token.consecutive_fail_count = 0
            runtime.health_status = "ok"
        else:
            token.consecutive_fail_count = int(token.consecutive_fail_count or 0) + 1
            token.last_error = error_message
            if token.consecutive_fail_count >= self.max_fails:
                token.is_active = False
            runtime.health_status = "fail"
            runtime.fail_count = int(runtime.fail_count or 0) + 1
            if account is not None:
                if self.blacklist.blacklist_enabled():
                    if self.blacklist.should_blacklist_by_quota(error_message):
                        self.blacklist.mark_blacklisted(account, reason="quota_exhausted")
                    elif (
                        self.blacklist.blacklist_on_continuous_fail()
                        and int(token.consecutive_fail_count or 0) >= self.blacklist.fail_threshold()
                    ):
                        self.blacklist.mark_blacklisted(account, reason="continuous_failures")

        self.db.commit()
        return {
            "token_id": token.id,
            "success": success,
            "is_active": token.is_active,
            "consecutive_fail_count": int(token.consecutive_fail_count or 0),
        }

    def _get_or_create_runtime_state(self, account_id: str) -> TavilyAccountRuntimeState:
        state = self.db.get(TavilyAccountRuntimeState, account_id)
        if state is not None:
            return state
        state = TavilyAccountRuntimeState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    def stats_overview(self) -> dict[str, Any]:
        total_requests = int(self.db.query(func.count(TavilyUsageLog.id)).scalar() or 0)
        success_requests = int(
            self.db.query(func.count(TavilyUsageLog.id))
            .filter(TavilyUsageLog.success.is_(True))
            .scalar()
            or 0
        )
        failed_requests = total_requests - success_requests
        avg_latency = int(
            self.db.query(func.avg(TavilyUsageLog.latency_ms))
            .filter(TavilyUsageLog.latency_ms.is_not(None))
            .scalar()
            or 0
        )
        success_rate = round(success_requests / total_requests, 4) if total_requests > 0 else 0.0
        return {
            "total_requests": total_requests,
            "success_requests": success_requests,
            "failed_requests": failed_requests,
            "success_rate": success_rate,
            "avg_latency_ms": avg_latency,
        }
