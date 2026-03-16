"""Tavily 账号黑名单服务。"""

from __future__ import annotations

import os
import time
from datetime import datetime, timedelta, timezone

import httpx
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount, TavilyAccountRuntimeState, TavilyBlacklistState, TavilyToken
from src.modules.tavily_pool.services.crypto import TavilyCryptoService


class TavilyBlacklistService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.crypto = TavilyCryptoService()
        self.base_url = os.getenv("TAVILY_POOL_API_BASE", "https://api.tavily.com").rstrip("/")
        self.timeout = float(os.getenv("TAVILY_POOL_HEALTH_TIMEOUT_SECONDS", "8"))

    def blacklist_enabled(self) -> bool:
        return self._env_bool("TAVILY_POOL_BLACKLIST_ENABLE", default=True)

    def blacklist_on_quota_exhausted(self) -> bool:
        return self._env_bool("TAVILY_POOL_BLACKLIST_ON_QUOTA_EXHAUSTED", default=True)

    def blacklist_on_continuous_fail(self) -> bool:
        return self._env_bool("TAVILY_POOL_BLACKLIST_ON_CONTINUOUS_FAIL", default=False)

    def fail_threshold(self) -> int:
        return int(os.getenv("TAVILY_POOL_BLACKLIST_FAIL_THRESHOLD", "3"))

    def retention_days(self) -> int:
        return int(os.getenv("TAVILY_POOL_BLACKLIST_RETENTION_DAYS", "31"))

    def cleanup_enabled(self) -> bool:
        return self._env_bool("TAVILY_POOL_BLACKLIST_CLEANUP_ENABLED", default=True)

    def should_blacklist_by_quota(self, error_message: str | None) -> bool:
        if not self.blacklist_on_quota_exhausted() or not error_message:
            return False

        text = error_message.lower()
        keywords = (
            "http 429",
            "quota",
            "limit",
            "monthly",
            "exceeded",
            "insufficient",
            "too many requests",
        )
        return any(keyword in text for keyword in keywords)

    def mark_blacklisted(self, account: TavilyAccount, *, reason: str, now: datetime | None = None) -> None:
        ts = now or datetime.now(timezone.utc)
        state = self._get_or_create_state(account.id)
        if state.status != "active":
            state.blacklisted_at = ts
            state.fail_count = 0
        state.status = "active"
        state.reason = reason
        state.last_check_at = ts
        runtime = self._get_or_create_runtime_state(account.id)
        runtime.status = "disabled"
        runtime.health_status = "fail"

    def release_blacklist(self, account: TavilyAccount, *, now: datetime | None = None) -> None:
        ts = now or datetime.now(timezone.utc)
        state = self._get_or_create_state(account.id)
        state.status = "released"
        state.blacklisted_at = None
        state.reason = None
        state.fail_count = 0
        state.last_check_at = ts
        runtime = self._get_or_create_runtime_state(account.id)
        runtime.status = "active"
        runtime.health_status = "ok"
        runtime.fail_count = 0

    def scan_and_cleanup(self) -> dict[str, int]:
        if not self.blacklist_enabled():
            return {"total": 0, "released": 0, "deleted": 0, "failed": 0}

        now = datetime.now(timezone.utc)
        total = 0
        released = 0
        deleted = 0
        failed = 0

        rows = (
            self.db.query(TavilyAccount, TavilyBlacklistState)
            .join(TavilyBlacklistState, TavilyBlacklistState.account_id == TavilyAccount.id)
            .filter(TavilyBlacklistState.status == "active")
            .all()
        )
        for account, state in rows:
            total += 1
            token = (
                self.db.query(TavilyToken)
                .filter(TavilyToken.account_id == account.id, TavilyToken.is_active.is_(True))
                .first()
            )
            if token is None:
                self._handle_failed_blacklist_probe(state, now)
                failed += 1
                if self._should_delete_after_retention(state, now):
                    self.db.delete(account)
                    deleted += 1
                continue

            raw_token = self.crypto.decrypt(token.token_encrypted)
            ok, _, _ = self._probe_api_key(raw_token)
            if ok:
                self.release_blacklist(account, now=now)
                released += 1
                continue

            self._handle_failed_blacklist_probe(state, now)
            failed += 1
            if self._should_delete_after_retention(state, now):
                self.db.delete(account)
                deleted += 1

        self.db.commit()
        return {"total": total, "released": released, "deleted": deleted, "failed": failed}

    def _handle_failed_blacklist_probe(self, state: TavilyBlacklistState, now: datetime) -> None:
        state.fail_count = int(state.fail_count or 0) + 1
        state.last_check_at = now

    def _should_delete_after_retention(self, state: TavilyBlacklistState, now: datetime) -> bool:
        if not self.cleanup_enabled():
            return False
        if state.blacklisted_at is None:
            return False
        blacklisted_at = self._as_utc_aware(state.blacklisted_at)
        return now - blacklisted_at > timedelta(days=self.retention_days())

    def _get_or_create_state(self, account_id: str) -> TavilyBlacklistState:
        state = self.db.get(TavilyBlacklistState, account_id)
        if state is not None:
            return state
        state = TavilyBlacklistState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    def _get_or_create_runtime_state(self, account_id: str) -> TavilyAccountRuntimeState:
        state = self.db.get(TavilyAccountRuntimeState, account_id)
        if state is not None:
            return state
        state = TavilyAccountRuntimeState(account_id=account_id)
        self.db.add(state)
        self.db.flush()
        return state

    def _probe_api_key(self, api_key: str) -> tuple[bool, str, int]:
        payload = {"query": "blacklist probe", "max_results": 1}
        started = time.perf_counter()
        try:
            with httpx.Client(timeout=self.timeout) as client:
                response = client.post(
                    f"{self.base_url}/search",
                    headers={"Authorization": f"Bearer {api_key}"},
                    json=payload,
                )
            cost_ms = int((time.perf_counter() - started) * 1000)
            if response.status_code == 200:
                return True, "", cost_ms
            message = response.text.strip()[:200] or ""
            return False, f"HTTP {response.status_code}: {message}", cost_ms
        except Exception as exc:  # noqa: BLE001
            cost_ms = int((time.perf_counter() - started) * 1000)
            return False, str(exc), cost_ms

    @staticmethod
    def _as_utc_aware(value: datetime) -> datetime:
        if value.tzinfo is None:
            return value.replace(tzinfo=timezone.utc)
        return value.astimezone(timezone.utc)

    @staticmethod
    def _env_bool(key: str, *, default: bool) -> bool:
        value = os.getenv(key)
        if value is None:
            return default
        return value.strip().lower() in {"1", "true", "yes", "on"}
