"""Tavily 账号池健康检查服务。"""

from __future__ import annotations

import json
import os
import time
from datetime import datetime, timezone

import httpx
from sqlalchemy.orm import Session

from src.modules.tavily_pool.services.crypto import TavilyCryptoService
from src.modules.tavily_pool.services.blacklist_service import TavilyBlacklistService
from src.modules.tavily_pool.models import TavilyAccount, TavilyAccountRuntimeState, TavilyHealthCheck, TavilyToken


class TavilyHealthService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.crypto = TavilyCryptoService()
        self.blacklist = TavilyBlacklistService(db)
        self.base_url = os.getenv("TAVILY_POOL_API_BASE", "https://api.tavily.com").rstrip("/")
        self.timeout = float(os.getenv("TAVILY_POOL_HEALTH_TIMEOUT_SECONDS", "8"))

    def run_health_check(self) -> dict[str, int]:
        accounts = self.db.query(TavilyAccount).all()
        success = 0
        failed = 0
        now = datetime.now(timezone.utc)

        for account in accounts:
            runtime = self._get_or_create_runtime_state(account.id)
            token = (
                self.db.query(TavilyToken)
                .filter(
                    TavilyToken.account_id == account.id,
                    TavilyToken.is_active.is_(True),
                )
                .first()
            )
            if token is None:
                self.db.add(
                    TavilyHealthCheck(
                        account_id=account.id,
                        check_type="account",
                        status="failed",
                        error_message="missing active token",
                        details_json=json.dumps({"reason": "missing_active_token"}),
                        checked_at=now,
                    )
                )
                runtime.health_checked_at = now
                runtime.health_status = "fail"
                runtime.fail_count = int(runtime.fail_count or 0) + 1
                failed += 1
                continue

            raw_token = self.crypto.decrypt(token.token_encrypted)
            ok, error_message, response_ms = self._probe_token(raw_token)
            status = "success" if ok else "failed"
            self.db.add(
                TavilyHealthCheck(
                    account_id=account.id,
                    token_id=token.id,
                    check_type="token",
                    status=status,
                    response_ms=response_ms,
                    error_message=error_message or None,
                    details_json=json.dumps({"token_id": token.id, "response_ms": response_ms}),
                    checked_at=now,
                )
            )

            token.last_checked_at = now
            token.last_response_ms = response_ms
            token.last_error = error_message or None
            if ok:
                token.consecutive_fail_count = 0
                token.last_success_at = now
                runtime.health_status = "ok"
                runtime.fail_count = 0
                success += 1
            else:
                token.consecutive_fail_count = int(token.consecutive_fail_count or 0) + 1
                if token.consecutive_fail_count >= 3:
                    token.is_active = False
                runtime.health_status = "fail"
                runtime.fail_count = int(runtime.fail_count or 0) + 1
                if self.blacklist.blacklist_enabled():
                    if self.blacklist.should_blacklist_by_quota(error_message):
                        self.blacklist.mark_blacklisted(account, reason="quota_exhausted", now=now)
                    elif (
                        self.blacklist.blacklist_on_continuous_fail()
                        and int(runtime.fail_count or 0) >= self.blacklist.fail_threshold()
                    ):
                        self.blacklist.mark_blacklisted(account, reason="continuous_failures", now=now)
                failed += 1

            runtime.health_checked_at = now

        self.db.commit()
        return {"total": len(accounts), "success": success, "failed": failed}

    def _get_or_create_runtime_state(self, account_id: str) -> TavilyAccountRuntimeState:
        state = self.db.get(TavilyAccountRuntimeState, account_id)
        if state is not None:
            return state
        state = TavilyAccountRuntimeState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    def _probe_token(self, token: str) -> tuple[bool, str, int]:
        payload = {"query": "health check", "max_results": 1}
        started = time.perf_counter()
        try:
            with httpx.Client(timeout=self.timeout) as client:
                response = client.post(
                    f"{self.base_url}/search",
                    headers={"Authorization": f"Bearer {token}"},
                    json=payload,
                )
            cost_ms = int((time.perf_counter() - started) * 1000)
            if response.status_code == 200:
                return True, "", cost_ms
            message = response.text.strip()[:200] or ""
            message = f"HTTP {response.status_code}: {message}".strip()
            return False, message, cost_ms
        except Exception as exc:
            cost_ms = int((time.perf_counter() - started) * 1000)
            return False, str(exc), cost_ms
