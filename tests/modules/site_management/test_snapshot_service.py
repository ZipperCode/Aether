"""Tests for SiteSnapshotService."""

from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.modules.site_management.services.snapshot_service import (
    SiteSnapshotService,
    SnapshotFetchResult,
)
from src.modules.site_management.webdav_client import WebDavDownloadResult


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

SAMPLE_PAYLOAD = {"accounts": {"accounts": [{"domain": "a.com"}, {"domain": "b.com"}]}}
SAMPLE_TEXT = json.dumps(SAMPLE_PAYLOAD)


def _make_source(*, url: str = "https://dav.example.com", username: str = "user", password: str = "enc_secret"):
    source = MagicMock()
    source.url = url
    source.username = username
    source.password = password
    return source


def _make_snapshot(*, fetched_at: datetime, payload: dict | None = None, payload_hash: str = "abc123"):
    snap = MagicMock()
    snap.id = "snap-1"
    snap.raw_payload = payload or SAMPLE_PAYLOAD
    snap.payload_hash = payload_hash
    snap.fetched_at = fetched_at
    return snap


@pytest.fixture
def mock_db():
    return MagicMock()


@pytest.fixture
def downloader():
    dl = AsyncMock()
    dl.return_value = WebDavDownloadResult(
        text=SAMPLE_TEXT,
        resolved_url="https://dav.example.com/backup.json",
        etag='"etag-1"',
        last_modified="Mon, 01 Jan 2024 00:00:00 GMT",
    )
    return dl


@pytest.fixture
def service(downloader):
    return SiteSnapshotService(downloader=downloader)


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_fresh_fetch_stores_snapshot(service, mock_db, downloader):
    """No cache -- downloads, stores, returns from_cache=False."""
    source = _make_source()

    # db.query(WebDavSource)...first() => source
    # db.query(SiteSourceSnapshot)...first() => None  (no cache)
    query_mock = mock_db.query.return_value
    filter_mock = query_mock.filter.return_value

    # We need separate return values for the two filter() calls:
    # 1st call: WebDavSource lookup -> returns source
    # 2nd call: SiteSourceSnapshot lookup -> returns None (via .order_by().first())
    source_filter = MagicMock()
    source_filter.first.return_value = source

    snapshot_filter = MagicMock()
    snapshot_filter.order_by.return_value.first.return_value = None

    query_mock.filter.side_effect = [source_filter, snapshot_filter]

    with patch("src.modules.site_management.services.snapshot_service.CryptoService") as MockCrypto, \
         patch("src.modules.site_management.services.snapshot_service.SiteSourceSnapshot") as MockSnap:
        MockCrypto.return_value.decrypt.return_value = "secret"
        mock_snap_instance = MagicMock()
        mock_snap_instance.id = "new-snap"
        MockSnap.return_value = mock_snap_instance

        result = await service.get_webdav_snapshot(mock_db, webdav_source_id="src-1")

    assert isinstance(result, SnapshotFetchResult)
    assert result.from_cache is False
    assert result.payload == SAMPLE_PAYLOAD
    assert result.source_url == "https://dav.example.com"
    downloader.assert_awaited_once_with("https://dav.example.com", "user", "secret")
    mock_db.add.assert_called_once()
    mock_db.commit.assert_called_once()


@pytest.mark.asyncio
async def test_cache_hit_within_ttl(service, mock_db, downloader):
    """Existing snapshot within TTL returns cached result."""
    source = _make_source()
    recent_time = datetime.now(timezone.utc) - timedelta(seconds=60)
    cached_snap = _make_snapshot(fetched_at=recent_time)

    source_filter = MagicMock()
    source_filter.first.return_value = source

    snapshot_filter = MagicMock()
    snapshot_filter.order_by.return_value.first.return_value = cached_snap

    mock_db.query.return_value.filter.side_effect = [source_filter, snapshot_filter]

    with patch("src.modules.site_management.services.snapshot_service.CryptoService") as MockCrypto:
        MockCrypto.return_value.decrypt.return_value = "secret"

        result = await service.get_webdav_snapshot(
            mock_db, webdav_source_id="src-1", cache_ttl_seconds=300
        )

    assert result.from_cache is True
    assert result.snapshot_id == "snap-1"
    downloader.assert_not_awaited()
    mock_db.add.assert_not_called()


@pytest.mark.asyncio
async def test_cache_expired_refetches(service, mock_db, downloader):
    """Expired TTL triggers a new download."""
    source = _make_source()
    old_time = datetime.now(timezone.utc) - timedelta(seconds=600)
    stale_snap = _make_snapshot(fetched_at=old_time)

    source_filter = MagicMock()
    source_filter.first.return_value = source

    snapshot_filter = MagicMock()
    snapshot_filter.order_by.return_value.first.return_value = stale_snap

    mock_db.query.return_value.filter.side_effect = [source_filter, snapshot_filter]

    with patch("src.modules.site_management.services.snapshot_service.CryptoService") as MockCrypto, \
         patch("src.modules.site_management.services.snapshot_service.SiteSourceSnapshot") as MockSnap:
        MockCrypto.return_value.decrypt.return_value = "secret"
        MockSnap.return_value = MagicMock(id="new-snap")

        result = await service.get_webdav_snapshot(
            mock_db, webdav_source_id="src-1", cache_ttl_seconds=300
        )

    assert result.from_cache is False
    downloader.assert_awaited_once()
    mock_db.add.assert_called_once()
    mock_db.commit.assert_called_once()


@pytest.mark.asyncio
async def test_force_refresh_bypasses_cache(service, mock_db, downloader):
    """force_refresh=True always downloads regardless of cache."""
    source = _make_source()
    recent_time = datetime.now(timezone.utc) - timedelta(seconds=10)
    cached_snap = _make_snapshot(fetched_at=recent_time)

    source_filter = MagicMock()
    source_filter.first.return_value = source

    snapshot_filter = MagicMock()
    snapshot_filter.order_by.return_value.first.return_value = cached_snap

    mock_db.query.return_value.filter.side_effect = [source_filter, snapshot_filter]

    with patch("src.modules.site_management.services.snapshot_service.CryptoService") as MockCrypto, \
         patch("src.modules.site_management.services.snapshot_service.SiteSourceSnapshot") as MockSnap:
        MockCrypto.return_value.decrypt.return_value = "secret"
        MockSnap.return_value = MagicMock(id="new-snap")

        result = await service.get_webdav_snapshot(
            mock_db, webdav_source_id="src-1", force_refresh=True
        )

    assert result.from_cache is False
    downloader.assert_awaited_once()
    mock_db.add.assert_called_once()
    mock_db.commit.assert_called_once()


@pytest.mark.asyncio
async def test_source_not_found_raises(service, mock_db):
    """Raises ValueError when WebDavSource does not exist."""
    mock_db.query.return_value.filter.return_value.first.return_value = None

    with patch("src.modules.site_management.services.snapshot_service.CryptoService"):
        with pytest.raises(ValueError, match="WebDavSource not found"):
            await service.get_webdav_snapshot(mock_db, webdav_source_id="nonexistent")


@pytest.mark.asyncio
async def test_extract_account_count(service, mock_db, downloader):
    """account_count is correctly extracted from payload."""
    source = _make_source()

    source_filter = MagicMock()
    source_filter.first.return_value = source

    snapshot_filter = MagicMock()
    snapshot_filter.order_by.return_value.first.return_value = None

    mock_db.query.return_value.filter.side_effect = [source_filter, snapshot_filter]

    with patch("src.modules.site_management.services.snapshot_service.CryptoService") as MockCrypto:
        MockCrypto.return_value.decrypt.return_value = "secret"
        with patch(
            "src.modules.site_management.services.snapshot_service.SiteSourceSnapshot"
        ) as MockSnap:
            mock_instance = MagicMock()
            mock_instance.id = "new-snap"
            MockSnap.return_value = mock_instance

            await service.get_webdav_snapshot(mock_db, webdav_source_id="src-1")

            kwargs = MockSnap.call_args[1]
            assert kwargs["account_count"] == 2
            assert kwargs["webdav_source_id"] == "src-1"
            assert kwargs["source_type"] == "all_api_hub_webdav"
