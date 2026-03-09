from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock

import pytest

from src.models.database import SiteSourceSnapshot
from src.services.provider_sync.webdav_client import WebDavDownloadResult
from src.services.site_management.snapshot_service import SiteSnapshotService


class _FakeQuery:
    def __init__(self, session: "_FakeSession") -> None:
        self._session = session

    def filter(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def order_by(self, *_args, **_kwargs) -> "_FakeQuery":
        return self

    def first(self) -> SiteSourceSnapshot | None:
        return self._session.latest_snapshot


class _FakeSession:
    def __init__(self, latest_snapshot: SiteSourceSnapshot | None = None) -> None:
        self.latest_snapshot = latest_snapshot
        self.added: list[SiteSourceSnapshot] = []
        self.commit_calls = 0

    def query(self, model: type[SiteSourceSnapshot]) -> _FakeQuery:
        assert model is SiteSourceSnapshot
        return _FakeQuery(self)

    def add(self, snapshot: SiteSourceSnapshot) -> None:
        self.latest_snapshot = snapshot
        self.added.append(snapshot)

    def commit(self) -> None:
        self.commit_calls += 1


@pytest.mark.asyncio
async def test_fetch_snapshot_uses_cache_when_not_expired() -> None:
    raw_payload = {"version": "2.0", "accounts": {"accounts": []}}
    snapshot = SiteSourceSnapshot(
        source_url="https://dav.example.com/backup.json",
        payload_hash="hash-1",
        raw_payload=raw_payload,
        account_count=0,
        fetched_at=datetime.now(timezone.utc) - timedelta(seconds=10),
    )
    db = _FakeSession(latest_snapshot=snapshot)
    downloader = AsyncMock()
    service = SiteSnapshotService(downloader=downloader)

    result = await service.get_webdav_snapshot(
        db,
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        cache_ttl_seconds=300,
    )

    assert result.from_cache is True
    assert result.payload == raw_payload
    downloader.assert_not_awaited()


@pytest.mark.asyncio
async def test_fetch_snapshot_force_refresh_downloads_and_persists() -> None:
    db = _FakeSession()
    payload = {"version": "2.0", "accounts": {"accounts": [{"site_url": "https://x.com"}]}}
    downloader = AsyncMock(return_value=json.dumps(payload))
    service = SiteSnapshotService(downloader=downloader)

    result = await service.get_webdav_snapshot(
        db,
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        force_refresh=True,
    )

    assert result.from_cache is False
    assert result.payload == payload
    assert db.commit_calls == 1
    assert len(db.added) == 1
    assert db.added[0].source_url == "https://dav.example.com/backup.json"


@pytest.mark.asyncio
async def test_snapshot_cache_still_hits_when_downloader_returns_different_resolved_url() -> None:
    db = _FakeSession()
    payload = {"version": "2.0", "accounts": {"accounts": [{"site_url": "https://x.com"}]}}
    downloader = AsyncMock(
        return_value=WebDavDownloadResult(
            text=json.dumps(payload),
            resolved_url="https://dav.example.com/all-api-hub-backup/all-api-hub-1-0.json",
            etag="etag-1",
            last_modified="Mon, 01 Jan 2026 00:00:00 GMT",
        )
    )
    service = SiteSnapshotService(downloader=downloader)

    first = await service.get_webdav_snapshot(
        db,
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        cache_ttl_seconds=300,
    )
    second = await service.get_webdav_snapshot(
        db,
        url="https://dav.example.com/backup.json",
        username="u",
        password="p",
        cache_ttl_seconds=300,
    )

    assert first.from_cache is False
    assert second.from_cache is True
    assert db.latest_snapshot is not None
    assert db.latest_snapshot.source_url == "https://dav.example.com/backup.json"
    downloader.assert_awaited_once()
