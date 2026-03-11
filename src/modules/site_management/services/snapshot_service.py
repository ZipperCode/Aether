"""Site snapshot service -- fetch & cache WebDAV snapshots."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Awaitable, Callable

from src.core.crypto import CryptoService
from src.modules.site_management.models import SiteSourceSnapshot, WebDavSource
from src.modules.site_management.webdav_client import (
    WebDavDownloadResult,
    download_backup_with_meta,
)

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
    """Download a WebDAV backup JSON and persist / cache the snapshot."""

    def __init__(self, downloader: DownloadWithMetaFn | None = None) -> None:
        self._downloader = downloader or download_backup_with_meta

    # ------------------------------------------------------------------
    # public
    # ------------------------------------------------------------------

    async def get_webdav_snapshot(
        self,
        db: Any,
        *,
        webdav_source_id: str,
        cache_ttl_seconds: int = 300,
        force_refresh: bool = False,
    ) -> SnapshotFetchResult:
        """Fetch a snapshot for the given *webdav_source_id*.

        Looks up ``WebDavSource`` from *db*, decrypts the password via
        ``CryptoService``, then downloads or returns a cached snapshot.
        """
        now = datetime.now(timezone.utc)

        # --- resolve source ---
        source = db.query(WebDavSource).filter(WebDavSource.id == webdav_source_id).first()
        if source is None:
            raise ValueError(f"WebDavSource not found: {webdav_source_id}")

        password = CryptoService().decrypt(source.password)
        url = str(source.url or "").strip()

        # --- cache check ---
        latest = self._get_latest_snapshot(db, webdav_source_id)
        if self._can_use_cache(
            latest,
            now=now,
            cache_ttl_seconds=cache_ttl_seconds,
            force_refresh=force_refresh,
        ):
            payload = latest.raw_payload if isinstance(latest.raw_payload, dict) else {}
            return SnapshotFetchResult(
                payload=payload,
                payload_hash=str(latest.payload_hash or ""),
                source_url=url,
                snapshot_id=str(latest.id or "") or None,
                fetched_at=latest.fetched_at if latest.fetched_at else now,
                from_cache=True,
            )

        # --- download ---
        raw = await self._downloader(url, source.username, password)
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
            webdav_source_id=webdav_source_id,
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
            source_url=url,
            snapshot_id=str(snapshot.id or "") or None,
            fetched_at=now,
            from_cache=False,
        )

    # ------------------------------------------------------------------
    # helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _get_latest_snapshot(db: Any, webdav_source_id: str) -> SiteSourceSnapshot | None:
        return (
            db.query(SiteSourceSnapshot)
            .filter(SiteSourceSnapshot.webdav_source_id == webdav_source_id)
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
