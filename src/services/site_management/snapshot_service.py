from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Awaitable, Callable

from src.models.database import SiteSourceSnapshot
from src.services.provider_sync.webdav_client import WebDavDownloadResult, download_backup_with_meta

DownloadWithMetaFn = Callable[[str, str, str], Awaitable[WebDavDownloadResult | str]]


@dataclass
class SnapshotFetchResult:
    payload: dict[str, Any]
    payload_hash: str
    source_url: str
    snapshot_id: str | None
    fetched_at: datetime
    from_cache: bool


class SiteSnapshotService:
    def __init__(self, downloader: DownloadWithMetaFn | None = None) -> None:
        self._downloader = downloader or download_backup_with_meta

    async def get_webdav_snapshot(
        self,
        db: Any,
        *,
        url: str,
        username: str,
        password: str,
        cache_ttl_seconds: int = 300,
        force_refresh: bool = False,
    ) -> SnapshotFetchResult:
        now = datetime.now(timezone.utc)
        normalized_url = str(url or "").strip()
        latest = self._get_latest_snapshot(db, normalized_url)

        if self._can_use_cache(latest, now=now, cache_ttl_seconds=cache_ttl_seconds, force_refresh=force_refresh):
            payload = latest.raw_payload if isinstance(latest.raw_payload, dict) else {}
            return SnapshotFetchResult(
                payload=payload,
                payload_hash=str(latest.payload_hash or ""),
                source_url=str(latest.source_url or normalized_url),
                snapshot_id=str(latest.id or "") or None,
                fetched_at=latest.fetched_at if latest.fetched_at else now,
                from_cache=True,
            )

        raw = await self._downloader(normalized_url, username, password)
        if isinstance(raw, WebDavDownloadResult):
            raw_text = raw.text
            etag = raw.etag
            last_modified = raw.last_modified
        else:
            raw_text = str(raw)
            etag = None
            last_modified = None

        payload = json.loads(raw_text)
        if not isinstance(payload, dict):
            raise ValueError("invalid site snapshot payload format")

        payload_hash = hashlib.sha256(raw_text.encode("utf-8")).hexdigest()
        snapshot = SiteSourceSnapshot(
            source_type="all_api_hub_webdav",
            source_url=normalized_url,
            etag=etag,
            last_modified=last_modified,
            payload_hash=payload_hash,
            raw_payload=payload,
            account_count=self._extract_account_count(payload),
            fetched_at=now,
            created_at=now,
        )
        db.add(snapshot)
        db.commit()

        return SnapshotFetchResult(
            payload=payload,
            payload_hash=payload_hash,
            source_url=normalized_url,
            snapshot_id=str(snapshot.id or "") or None,
            fetched_at=now,
            from_cache=False,
        )

    @staticmethod
    def _get_latest_snapshot(db: Any, source_url: str) -> SiteSourceSnapshot | None:
        return (
            db.query(SiteSourceSnapshot)
            .filter(
                SiteSourceSnapshot.source_type == "all_api_hub_webdav",
                SiteSourceSnapshot.source_url == source_url,
            )
            .order_by(SiteSourceSnapshot.fetched_at.desc())
            .first()
        )

    @staticmethod
    def _can_use_cache(
        latest: SiteSourceSnapshot | None,
        *,
        now: datetime,
        cache_ttl_seconds: int,
        force_refresh: bool,
    ) -> bool:
        if force_refresh or latest is None or cache_ttl_seconds <= 0:
            return False
        fetched_at = latest.fetched_at
        if not isinstance(fetched_at, datetime):
            return False
        if fetched_at.tzinfo is None:
            fetched_at = fetched_at.replace(tzinfo=timezone.utc)
        return (now - fetched_at).total_seconds() < cache_ttl_seconds

    @staticmethod
    def _extract_account_count(payload: dict[str, Any]) -> int:
        accounts = payload.get("accounts")
        if not isinstance(accounts, dict):
            return 0
        account_list = accounts.get("accounts")
        if not isinstance(account_list, list):
            return 0
        return len(account_list)
