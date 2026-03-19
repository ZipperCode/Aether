from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.modules.search_pool_gateway.models import GatewayApiKey
from src.utils.auth_utils import require_admin


def _build_client(tmp_path, monkeypatch) -> TestClient:
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))

    from src.modules.search_pool_gateway.sqlite import get_engine
    from src.modules.search_pool_gateway.routes_admin import router

    get_engine(reset=True)

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


def _update_tavily_key_usage(tmp_path, monkeypatch, key_id: str) -> None:
    from src.modules.search_pool_gateway.sqlite import get_session_factory

    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))

    session_factory = get_session_factory()
    with session_factory() as db:
        row = db.query(GatewayApiKey).filter(GatewayApiKey.id == key_id).one()
        row.usage_key_limit = 2147483647
        row.usage_key_used = 18
        row.usage_key_remaining = 2147483629
        row.usage_account_plan = "development"
        row.usage_sync_error = ""
        db.commit()


def test_admin_usage_sync_returns_placeholder_for_tavily(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    result = sync_resp.json()["result"]
    assert result["service"] == "tavily"
    assert result["synced_keys"] == 0
    assert result["errors"] == 0
    assert "暂未启用" in result["message"]
    assert result["synced_at"] is not None


def test_admin_usage_sync_tavily_keeps_existing_usage_fields(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-xxx"},
    )
    assert create_key.status_code == 200
    key_id = create_key.json()["id"]

    _update_tavily_key_usage(tmp_path, monkeypatch, key_id)

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    result = sync_resp.json()["result"]
    assert result["service"] == "tavily"
    assert result["synced_keys"] == 0
    assert result["errors"] == 0

    keys_resp = client.get("/api/admin/search-pool/keys?service=tavily")
    assert keys_resp.status_code == 200
    key = keys_resp.json()["keys"][0]
    assert key["usage_key_limit"] == 2147483647
    assert key["usage_key_used"] == 18
    assert key["usage_key_remaining"] == 2147483629
    assert key["usage_account_plan"] == "development"
    assert key["usage_sync_error"] == ""


def test_admin_stats_overview_contains_basic_counts(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    key_resp = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "firecrawl", "key": "fc-test-key-1234567890"},
    )
    assert key_resp.status_code == 200

    token_resp = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "firecrawl", "name": "ops"},
    )
    assert token_resp.status_code == 200

    stats_resp = client.get("/api/admin/search-pool/stats/overview?service=firecrawl")
    assert stats_resp.status_code == 200
    stats = stats_resp.json()
    assert stats["service"] == "firecrawl"
    assert stats["keys_total"] >= 1
    assert stats["tokens_total"] >= 1
