"""Route tests for the site management module."""
from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.database import get_db
from src.modules.site_management.routes import router
from src.modules.site_management.services.webdav_source_service import WebDavSourceService
from src.utils.auth_utils import require_admin


def _build_app(db: MagicMock) -> TestClient:
    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    app.dependency_overrides[get_db] = lambda: db
    return TestClient(app)


def _make_source(
    source_id: str,
    *,
    name: str = "Source-1",
    checkin_enabled: bool = True,
    checkin_time: str = "04:00",
) -> SimpleNamespace:
    now = datetime(2026, 3, 12, 8, 0, tzinfo=timezone.utc)
    return SimpleNamespace(
        id=source_id,
        name=name,
        url="https://dav.example.com/backup.json",
        username="user",
        is_active=True,
        sync_enabled=True,
        checkin_enabled=checkin_enabled,
        checkin_time=checkin_time,
        last_sync_at=None,
        last_sync_status=None,
        created_at=now,
        updated_at=now,
    )


def test_create_source_refreshes_checkin_job() -> None:
    db = MagicMock()
    client = _build_app(db)
    source = _make_source("source-1", checkin_enabled=False, checkin_time="08:30")

    with (
        patch.object(WebDavSourceService, "create", return_value=source),
        patch("src.modules.site_management.routes.SiteManagementScheduler", create=True) as scheduler_cls,
    ):
        resp = client.post(
            "/api/admin/site-management/sources",
            json={
                "name": "Source-1",
                "url": "https://dav.example.com/backup.json",
                "username": "user",
                "password": "secret",
                "checkin_enabled": False,
                "checkin_time": "08:30",
            },
        )

    assert resp.status_code == 200
    assert resp.json()["checkin_enabled"] is False
    assert resp.json()["checkin_time"] == "08:30"
    db.commit.assert_called_once()
    scheduler_cls.return_value.refresh_source_checkin_job.assert_called_once_with("source-1")


def test_create_source_invalid_checkin_time_returns_400() -> None:
    db = MagicMock()
    client = _build_app(db)

    resp = client.post(
        "/api/admin/site-management/sources",
        json={
            "name": "Source-1",
            "url": "https://dav.example.com/backup.json",
            "username": "user",
            "password": "secret",
            "checkin_enabled": True,
            "checkin_time": "99:99",
        },
    )

    assert resp.status_code == 400
    assert "checkin_time" in resp.json()["detail"]


def test_update_source_refreshes_checkin_job() -> None:
    db = MagicMock()
    client = _build_app(db)
    source = _make_source("source-1", checkin_enabled=True, checkin_time="09:15")

    with (
        patch.object(WebDavSourceService, "update", return_value=source),
        patch("src.modules.site_management.routes._account_count_for_source", return_value=3),
        patch("src.modules.site_management.routes.SiteManagementScheduler", create=True) as scheduler_cls,
    ):
        resp = client.put(
            "/api/admin/site-management/sources/source-1",
            json={"checkin_enabled": True, "checkin_time": "09:15"},
        )

    assert resp.status_code == 200
    assert resp.json()["account_count"] == 3
    assert resp.json()["checkin_time"] == "09:15"
    db.commit.assert_called_once()
    scheduler_cls.return_value.refresh_source_checkin_job.assert_called_once_with("source-1")


def test_update_source_invalid_checkin_time_returns_400() -> None:
    db = MagicMock()
    client = _build_app(db)

    resp = client.put(
        "/api/admin/site-management/sources/source-1",
        json={"checkin_time": "99:99"},
    )

    assert resp.status_code == 400
    assert "checkin_time" in resp.json()["detail"]


def test_delete_source_refreshes_checkin_job() -> None:
    db = MagicMock()
    client = _build_app(db)

    with (
        patch.object(WebDavSourceService, "delete", return_value=True),
        patch("src.modules.site_management.routes.SiteManagementScheduler", create=True) as scheduler_cls,
    ):
        resp = client.delete("/api/admin/site-management/sources/source-1")

    assert resp.status_code == 200
    assert resp.json() == {"ok": True}
    db.commit.assert_called_once()
    scheduler_cls.return_value.refresh_source_checkin_job.assert_called_once_with("source-1")


def test_list_checkin_runs_supports_source_filter() -> None:
    db = MagicMock()
    client = _build_app(db)
    now = datetime(2026, 3, 12, 9, 0, tzinfo=timezone.utc)
    run = SimpleNamespace(
        id="run-1",
        webdav_source_id="source-1",
        trigger_source="scheduled",
        status="success",
        error_message=None,
        total_providers=1,
        success_count=1,
        failed_count=0,
        skipped_count=0,
        started_at=now,
        finished_at=now,
        created_at=now,
    )

    base_query = MagicMock()
    filtered_query = MagicMock()
    ordered_query = MagicMock()
    db.query.return_value = base_query
    base_query.filter.return_value = filtered_query
    filtered_query.order_by.return_value = ordered_query
    ordered_query.count.return_value = 1
    ordered_query.offset.return_value.limit.return_value.all.return_value = [run]

    resp = client.get(
        "/api/admin/site-management/checkin-runs",
        params={"source_id": "source-1"},
    )

    assert resp.status_code == 200
    assert resp.json()["items"][0]["webdav_source_id"] == "source-1"
    base_query.filter.assert_called_once()


def test_get_checkin_run_items_includes_account_fields() -> None:
    db = MagicMock()
    client = _build_app(db)
    now = datetime(2026, 3, 12, 9, 0, tzinfo=timezone.utc)
    item = SimpleNamespace(
        id="item-1",
        run_id="run-1",
        provider_id=None,
        provider_name=None,
        provider_domain="demo.example",
        account_id="account-1",
        account_domain="demo.example",
        account_site_url="https://demo.example",
        status="success",
        message="checked in",
        balance_total=None,
        balance_currency=None,
        created_at=now,
    )

    exists_query = MagicMock()
    items_query = MagicMock()
    ordered_query = MagicMock()
    db.query.side_effect = [exists_query, items_query]
    exists_query.filter.return_value.first.return_value = object()
    items_query.filter.return_value.order_by.return_value = ordered_query
    ordered_query.count.return_value = 1
    ordered_query.offset.return_value.limit.return_value.all.return_value = [item]

    resp = client.get("/api/admin/site-management/checkin-runs/run-1/items")

    assert resp.status_code == 200
    payload = resp.json()["items"][0]
    assert payload["account_id"] == "account-1"
    assert payload["account_domain"] == "demo.example"
    assert payload["account_site_url"] == "https://demo.example"
