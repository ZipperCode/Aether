from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.utils.auth_utils import require_admin


def _build_client(tmp_path, monkeypatch) -> TestClient:
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_CRYPTO_KEY", "search-pool-gateway-test-key")

    from src.modules.search_pool_gateway.sqlite import get_engine
    from src.modules.search_pool_gateway.routes_admin import router

    get_engine(reset=True)

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


def test_admin_usage_sync_updates_usage_fields(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-test-key-1234567890"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    payload = sync_resp.json()
    assert "result" in payload
    assert payload["result"]["service"] == "tavily"
    assert payload["result"]["synced_keys"] >= 1


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
