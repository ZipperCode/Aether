"""Usage sync and stats service for Search Pool Gateway."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.models import GatewayApiKey, GatewayToken, GatewayUsageLog


class GatewayUsageService:
    def __init__(self, db: Session) -> None:
        self.db = db

    def sync(self, service: str | None = None, force: bool = False) -> dict[str, Any]:
        _ = force
        query = self.db.query(GatewayApiKey)
        if service:
            query = query.filter(GatewayApiKey.service == service)
        keys = query.all()

        now = datetime.now(timezone.utc)
        for row in keys:
            row.usage_synced_at = now
            row.usage_sync_error = ""
        self.db.commit()

        return {
            "service": service or "all",
            "synced_keys": len(keys),
            "synced_at": now.isoformat(),
        }

    def stats_overview(self, service: str | None = None) -> dict[str, Any]:
        key_query = self.db.query(GatewayApiKey)
        token_query = self.db.query(GatewayToken)
        log_query = self.db.query(GatewayUsageLog)

        if service:
            key_query = key_query.filter(GatewayApiKey.service == service)
            token_query = token_query.filter(GatewayToken.service == service)
            log_query = log_query.filter(GatewayUsageLog.service == service)

        keys_total = key_query.count()
        keys_active = key_query.filter(GatewayApiKey.active.is_(True)).count()
        tokens_total = token_query.count()
        requests_total = log_query.count()
        requests_success = log_query.filter(GatewayUsageLog.success.is_(True)).count()
        requests_failed = requests_total - requests_success
        success_rate = (requests_success / requests_total) if requests_total else 0.0

        return {
            "service": service or "all",
            "keys_total": keys_total,
            "keys_active": keys_active,
            "tokens_total": tokens_total,
            "requests_total": requests_total,
            "requests_success": requests_success,
            "requests_failed": requests_failed,
            "success_rate": round(success_rate, 4),
        }
