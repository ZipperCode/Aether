"""Tavily 账号额度同步服务。"""

from __future__ import annotations

import os
from datetime import datetime, timezone

import httpx
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyAccountRuntimeState, TavilyToken
from src.modules.tavily_pool.services.crypto import TavilyCryptoService


def _to_int(value: object) -> int | None:
    if value in (None, ""):
        return None
    try:
        return int(value)  # type: ignore[arg-type]
    except Exception:
        try:
            return int(float(value))  # type: ignore[arg-type]
        except Exception:
            return None


class TavilyUsageService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.crypto = TavilyCryptoService()
        self.base_url = os.getenv("TAVILY_POOL_API_BASE", "https://api.tavily.com").rstrip("/")
        self.timeout = float(os.getenv("TAVILY_POOL_USAGE_TIMEOUT_SECONDS", "10"))

    def run_usage_sync(self) -> dict[str, int]:
        accounts = self.db.query(TavilyAccount).all()
        synced_accounts = 0
        failed_accounts = 0
        now = datetime.now(timezone.utc)

        for account in accounts:
            runtime = self._get_or_create_runtime_state(account.id)
            token = (
                self.db.query(TavilyToken)
                .filter(TavilyToken.account_id == account.id, TavilyToken.is_active.is_(True))
                .first()
            )
            if token is None:
                runtime.usage_synced_at = now
                runtime.usage_sync_error = "missing active token"
                failed_accounts += 1
                continue

            raw_token = self.crypto.decrypt(token.token_encrypted)
            try:
                payload = self._fetch_usage(raw_token)
                runtime.usage_plan = str((payload.get("account") or {}).get("current_plan") or "")
                runtime.usage_account_used = _to_int((payload.get("account") or {}).get("plan_usage"))
                runtime.usage_account_limit = _to_int((payload.get("account") or {}).get("plan_limit"))
                if (
                    runtime.usage_account_limit is not None
                    and runtime.usage_account_used is not None
                ):
                    runtime.usage_account_remaining = max(
                        0, runtime.usage_account_limit - runtime.usage_account_used
                    )
                else:
                    runtime.usage_account_remaining = None
                runtime.usage_synced_at = now
                runtime.usage_sync_error = None
                synced_accounts += 1
            except Exception as exc:
                runtime.usage_synced_at = now
                runtime.usage_sync_error = str(exc)[:200]
                failed_accounts += 1

        self.db.commit()
        return {
            "total_accounts": len(accounts),
            "synced_accounts": synced_accounts,
            "failed_accounts": failed_accounts,
        }

    def _get_or_create_runtime_state(self, account_id: str) -> TavilyAccountRuntimeState:
        state = self.db.get(TavilyAccountRuntimeState, account_id)
        if state is not None:
            return state
        state = TavilyAccountRuntimeState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    def _fetch_usage(self, token: str) -> dict:
        with httpx.Client(timeout=self.timeout) as client:
            response = client.get(
                f"{self.base_url}/usage",
                headers={"Authorization": f"Bearer {token}"},
            )
        if response.status_code != 200:
            message = response.text.strip()[:200] or f"HTTP {response.status_code}"
            raise RuntimeError(message)
        data = response.json()
        if not isinstance(data, dict):
            raise RuntimeError("invalid usage payload")
        return data
