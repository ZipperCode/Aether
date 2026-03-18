"""Usage sync and stats service for Search Pool Gateway."""

from __future__ import annotations

import os
from datetime import datetime, timezone
from typing import Any

import httpx
from sqlalchemy.orm import Session

from src.core.logger import logger
from src.modules.search_pool_gateway.models import GatewayApiKey, GatewayToken, GatewayUsageLog

SERVICE_WORKSPACE_META: dict[str, dict[str, str]] = {
    "tavily": {
        "title": "Tavily",
        "description": "管理 Tavily 搜索与提取代理池。",
        "service_badge": "Tavily",
        "route_label": "POST /api/search, POST /api/extract",
        "route_path": "/api/search",
    },
    "firecrawl": {
        "title": "Firecrawl",
        "description": "管理 Firecrawl 抓取代理池。",
        "service_badge": "Firecrawl",
        "route_label": "ANY /firecrawl/{path}",
        "route_path": "/firecrawl/v2/scrape",
    },
}

TAVILY_SYNC_URL = "https://app.tavily.com/api/keys"
TAVILY_SYNC_COOKIE_ENV = "SEARCH_POOL_TAVILY_SYNC_COOKIE"
TAVILY_SYNC_USER_AGENT_ENV = "SEARCH_POOL_TAVILY_SYNC_USER_AGENT"
TAVILY_SYNC_REFERER_ENV = "SEARCH_POOL_TAVILY_SYNC_REFERER"
DEFAULT_TAVILY_SYNC_USER_AGENT = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
    "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36"
)
DEFAULT_TAVILY_SYNC_REFERER = "https://app.tavily.com/home"


def _format_sync_value(value: Any) -> str:
    if value is None:
        return "-"
    if isinstance(value, bool):
        return str(value).lower()
    return str(value).replace("\n", "\\n")


def _log_sync_event(level: str, **fields: Any) -> None:
    ordered_fields = {
        "service": fields.get("service"),
        "stage": fields.get("stage"),
        "cookie_present": fields.get("cookie_present"),
        "fetched_keys": fields.get("fetched_keys"),
        "matched_keys": fields.get("matched_keys"),
        "unmatched_keys": fields.get("unmatched_keys"),
        "synced_keys": fields.get("synced_keys"),
        "errors": fields.get("errors"),
        "status_code": fields.get("status_code"),
        "error_summary": fields.get("error_summary"),
    }
    message = "[search-pool-sync] " + " ".join(
        f"{key}={_format_sync_value(value)}" for key, value in ordered_fields.items()
    )
    getattr(logger, level)(message)


def _coerce_int(value: Any) -> int | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    if isinstance(value, str):
        text = value.strip()
        if not text:
            return None
        try:
            return int(float(text))
        except ValueError:
            return None
    return None


def _fetch_tavily_console_keys(cookie_header: str) -> list[dict[str, Any]]:
    headers = {
        "accept": "*/*",
        "cookie": cookie_header,
        "user-agent": os.getenv(TAVILY_SYNC_USER_AGENT_ENV, DEFAULT_TAVILY_SYNC_USER_AGENT),
        "referer": os.getenv(TAVILY_SYNC_REFERER_ENV, DEFAULT_TAVILY_SYNC_REFERER),
    }
    with httpx.Client(timeout=30.0, headers=headers) as client:
        response = client.get(TAVILY_SYNC_URL)
    response.raise_for_status()
    payload = response.json()
    if not isinstance(payload, list):
        raise ValueError("Unexpected Tavily /api/keys response payload")
    return [item for item in payload if isinstance(item, dict)]


class GatewayUsageService:
    def __init__(self, db: Session) -> None:
        self.db = db

    def sync(self, service: str | None = None, force: bool = False) -> dict[str, Any]:
        _ = force
        service_norm = (service or "all").strip().lower()
        if service_norm == "tavily":
            return self._sync_tavily_usage()

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
            "errors": 0,
            "message": "sync completed",
            "synced_at": now.isoformat(),
        }

    def _sync_tavily_usage(self) -> dict[str, Any]:
        keys = (
            self.db.query(GatewayApiKey)
            .filter(GatewayApiKey.service == "tavily")
            .order_by(GatewayApiKey.created_at.asc())
            .all()
        )
        now = datetime.now(timezone.utc)
        cookie_header = (os.getenv(TAVILY_SYNC_COOKIE_ENV, "") or "").strip()
        _log_sync_event(
            "info",
            service="tavily",
            stage="sync_started",
            cookie_present=bool(cookie_header),
            synced_keys=len(keys),
            errors=0,
        )
        if not cookie_header:
            for row in keys:
                row.usage_synced_at = now
                row.usage_sync_error = "Missing Tavily sync cookie"
            self.db.commit()
            _log_sync_event(
                "error",
                service="tavily",
                stage="sync_failed",
                synced_keys=0,
                errors=1,
                error_summary="missing tavily sync cookie",
            )
            return {
                "service": "tavily",
                "synced_keys": 0,
                "errors": 1,
                "message": "Missing Tavily sync cookie",
                "synced_at": now.isoformat(),
            }

        try:
            remote_keys = _fetch_tavily_console_keys(cookie_header)
        except httpx.HTTPStatusError as exc:
            status_code = exc.response.status_code if exc.response is not None else None
            error_summary = f"tavily console returned HTTP {status_code or 'unknown'}"
            for row in keys:
                row.usage_synced_at = now
                row.usage_sync_error = error_summary
            self.db.commit()
            _log_sync_event(
                "error",
                service="tavily",
                stage="fetch_keys_failed",
                status_code=status_code,
                synced_keys=0,
                errors=1,
                error_summary=error_summary,
            )
            return {
                "service": "tavily",
                "synced_keys": 0,
                "errors": 1,
                "message": error_summary,
                "synced_at": now.isoformat(),
            }
        except Exception as exc:
            error_summary = f"failed to fetch tavily keys: {exc}"
            for row in keys:
                row.usage_synced_at = now
                row.usage_sync_error = error_summary[:255]
            self.db.commit()
            _log_sync_event(
                "error",
                service="tavily",
                stage="fetch_keys_failed",
                synced_keys=0,
                errors=1,
                error_summary=error_summary,
            )
            return {
                "service": "tavily",
                "synced_keys": 0,
                "errors": 1,
                "message": error_summary,
                "synced_at": now.isoformat(),
            }

        _log_sync_event(
            "info",
            service="tavily",
            stage="fetch_keys_succeeded",
            fetched_keys=len(remote_keys),
            errors=0,
        )

        decrypted_map: dict[str, GatewayApiKey] = {}
        for row in keys:
            raw_key = (row.raw_key or "").strip()
            if not raw_key:
                row.usage_synced_at = now
                row.usage_sync_error = "Missing raw key"[:255]
                continue
            decrypted_map[raw_key] = row

        matched_keys = 0
        unmatched_keys = 0
        errors = 0

        for remote_item in remote_keys:
            raw_key = str(remote_item.get("key") or "").strip()
            if not raw_key:
                unmatched_keys += 1
                continue
            row = decrypted_map.get(raw_key)
            if row is None:
                unmatched_keys += 1
                _log_sync_event(
                    "warning",
                    service="tavily",
                    stage="key_unmatched",
                    unmatched_keys=unmatched_keys,
                    error_summary=f"unmatched key {raw_key[:12]}...",
                )
                continue

            limit = _coerce_int(remote_item.get("limit"))
            usage = _coerce_int(remote_item.get("usage"))
            remaining = None
            if limit is not None and usage is not None:
                remaining = max(limit - usage, 0)

            row.usage_key_limit = limit
            row.usage_key_used = usage
            row.usage_key_remaining = remaining
            row.usage_account_plan = str(remote_item.get("key_type") or "").strip()
            row.usage_synced_at = now
            if limit is None or usage is None:
                errors += 1
                row.usage_sync_error = "Missing usage fields from Tavily payload"[:255]
                _log_sync_event(
                    "warning",
                    service="tavily",
                    stage="key_payload_incomplete",
                    matched_keys=matched_keys + 1,
                    synced_keys=matched_keys + 1,
                    errors=errors,
                    error_summary=row.usage_sync_error,
                )
            else:
                row.usage_sync_error = ""
            matched_keys += 1
            _log_sync_event(
                "info",
                service="tavily",
                stage="key_matched",
                matched_keys=matched_keys,
                synced_keys=matched_keys,
                errors=errors,
            )

        self.db.commit()
        message = f"Synced {matched_keys} Tavily keys"
        _log_sync_event(
            "info",
            service="tavily",
            stage="sync_completed",
            fetched_keys=len(remote_keys),
            matched_keys=matched_keys,
            unmatched_keys=unmatched_keys,
            synced_keys=matched_keys,
            errors=errors,
            error_summary=message,
        )
        return {
            "service": "tavily",
            "synced_keys": matched_keys,
            "fetched_keys": len(remote_keys),
            "matched_keys": matched_keys,
            "unmatched_keys": unmatched_keys,
            "errors": errors,
            "message": message,
            "synced_at": now.isoformat(),
        }

    def build_token_usage_summary(self, service: str | None = None) -> dict[str, dict[str, int]]:
        query = self.db.query(GatewayUsageLog)
        if service:
            query = query.filter(GatewayUsageLog.service == service)

        now = datetime.now(timezone.utc)
        current_month = (now.year, now.month)
        summary: dict[str, dict[str, int]] = {}
        for row in query.all():
            if not row.token_id:
                continue
            item = summary.setdefault(
                row.token_id,
                {
                    "usage_success": 0,
                    "usage_failed": 0,
                    "usage_this_month": 0,
                },
            )
            if row.success:
                item["usage_success"] += 1
            else:
                item["usage_failed"] += 1
            created = row.created_at
            if created and (created.year, created.month) == current_month:
                item["usage_this_month"] += 1
        return summary

    def _sum_key_real_limit(self, keys: list[GatewayApiKey]) -> int:
        total = 0
        for row in keys:
            total += int(
                row.usage_account_limit
                or row.usage_key_limit
                or 0
            )
        return total

    def _sum_key_real_used(self, keys: list[GatewayApiKey]) -> int:
        total = 0
        for row in keys:
            total += int(
                row.usage_account_used
                or row.usage_key_used
                or 0
            )
        return total

    def _sum_key_real_remaining(self, keys: list[GatewayApiKey]) -> int:
        total = 0
        for row in keys:
            total += int(
                row.usage_account_remaining
                or row.usage_key_remaining
                or 0
            )
        return total

    def _max_synced_at(self, keys: list[GatewayApiKey]) -> str | None:
        values = [row.usage_synced_at for row in keys if row.usage_synced_at]
        if not values:
            return None
        return max(values).isoformat()

    def _request_windows(self, service: str) -> tuple[int, int]:
        now = datetime.now(timezone.utc)
        today = now.date()
        current_month = (now.year, now.month)
        logs = self.db.query(GatewayUsageLog).filter(GatewayUsageLog.service == service).all()
        today_count = 0
        month_count = 0
        for row in logs:
            created = row.created_at
            if not created:
                continue
            if created.date() == today:
                today_count += 1
            if (created.year, created.month) == current_month:
                month_count += 1
        return today_count, month_count

    def list_service_summaries(self) -> dict[str, list[dict[str, Any]]]:
        services = []
        for service in SERVICE_WORKSPACE_META:
            services.append(self.build_workspace(service, include_lists=False))
        return {"services": services}

    def build_workspace(self, service: str, *, include_lists: bool = True) -> dict[str, Any]:
        service_norm = service.strip().lower()
        meta = SERVICE_WORKSPACE_META[service_norm]
        keys = self.db.query(GatewayApiKey).filter(GatewayApiKey.service == service_norm).order_by(GatewayApiKey.created_at.asc()).all()
        tokens = self.db.query(GatewayToken).filter(GatewayToken.service == service_norm).order_by(GatewayToken.created_at.asc()).all()
        token_usage = self.build_token_usage_summary(service_norm)
        overview = self.stats_overview(service_norm)
        requests_today, requests_this_month = self._request_windows(service_norm)
        real_limit = self._sum_key_real_limit(keys)
        real_used = self._sum_key_real_used(keys)
        real_remaining = self._sum_key_real_remaining(keys)
        synced_keys = sum(1 for row in keys if row.usage_synced_at)
        last_synced_at = self._max_synced_at(keys)

        payload = {
            "service": service_norm,
            "title": meta["title"],
            "description": meta["description"],
            "service_badge": meta["service_badge"],
            "keys_active": overview["keys_active"],
            "keys_total": overview["keys_total"],
            "tokens_total": overview["tokens_total"],
            "requests_today": requests_today,
            "real_remaining": real_remaining,
            "route_label": meta["route_label"],
            "route_path": meta["route_path"],
            "last_synced_at": last_synced_at,
            "route_summary": {
                "title": meta["title"],
                "description": meta["description"],
                "service_badge": meta["service_badge"],
                "route_label": meta["route_label"],
                "route_path": meta["route_path"],
            },
            "stats": {
                **overview,
                "keys_inactive": max(overview["keys_total"] - overview["keys_active"], 0),
                "requests_today": requests_today,
                "requests_this_month": requests_this_month,
                "real_used": real_used,
                "real_remaining": real_remaining,
                "real_limit": real_limit,
                "synced_keys": synced_keys,
                "last_synced_at": last_synced_at,
            },
            "usage_examples": {
                "base_url": meta["route_path"],
                "curl_examples": self._build_curl_examples(service_norm),
            },
        }
        if include_lists:
            payload["keys"] = keys
            payload["tokens"] = [
                {
                    **self.serialize_token(row),
                    **token_usage.get(
                        row.id,
                        {"usage_success": 0, "usage_failed": 0, "usage_this_month": 0},
                    ),
                }
                for row in tokens
            ]
        return payload

    def serialize_key(self, row: GatewayApiKey) -> dict[str, Any]:
        return {
            "id": row.id,
            "service": row.service,
            "key_masked": row.key_masked,
            "email": row.email,
            "active": row.active,
            "total_used": row.total_used,
            "total_failed": row.total_failed,
            "consecutive_fails": row.consecutive_fails,
            "last_used_at": row.last_used_at.isoformat() if row.last_used_at else None,
            "usage_key_used": row.usage_key_used,
            "usage_key_limit": row.usage_key_limit,
            "usage_key_remaining": row.usage_key_remaining,
            "usage_account_plan": row.usage_account_plan,
            "usage_account_used": row.usage_account_used,
            "usage_account_limit": row.usage_account_limit,
            "usage_account_remaining": row.usage_account_remaining,
            "usage_synced_at": row.usage_synced_at.isoformat() if row.usage_synced_at else None,
            "usage_sync_error": row.usage_sync_error,
            "created_at": row.created_at.isoformat() if row.created_at else None,
            "updated_at": row.updated_at.isoformat() if row.updated_at else None,
        }

    def serialize_token(self, row: GatewayToken) -> dict[str, Any]:
        return {
            "id": row.id,
            "service": row.service,
            "token": row.token,
            "name": row.name,
            "hourly_limit": row.hourly_limit,
            "daily_limit": row.daily_limit,
            "monthly_limit": row.monthly_limit,
            "created_at": row.created_at.isoformat() if row.created_at else None,
            "updated_at": row.updated_at.isoformat() if row.updated_at else None,
        }

    def _build_curl_examples(self, service: str) -> list[str]:
        if service == "firecrawl":
            return [
                "curl -X POST $BASE_URL/firecrawl/v2/scrape -H 'Authorization: Bearer <gateway-token>' -H 'Content-Type: application/json' -d '{\"url\":\"https://example.com\"}'",
            ]
        return [
            "curl -X POST $BASE_URL/api/search -H 'Authorization: Bearer <gateway-token>' -H 'Content-Type: application/json' -d '{\"query\":\"latest ai news\"}'",
            "curl -X POST $BASE_URL/api/extract -H 'Authorization: Bearer <gateway-token>' -H 'Content-Type: application/json' -d '{\"urls\":[\"https://example.com\"]}'",
        ]

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
