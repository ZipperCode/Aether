from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

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


def test_admin_can_fetch_service_summary_and_workspace(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    assert client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-test-key-1234567890", "email": "a@example.com"},
    ).status_code == 200
    assert client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "tavily", "name": "team-a", "hourly_limit": 10, "daily_limit": 100, "monthly_limit": 1000},
    ).status_code == 200
    assert client.post(
        "/api/admin/search-pool/keys",
        json={"service": "firecrawl", "key": "fc-test-key-1234567890", "email": "b@example.com"},
    ).status_code == 200

    summary_resp = client.get("/api/admin/search-pool/services/summary")
    assert summary_resp.status_code == 200
    summary_payload = summary_resp.json()
    assert "services" in summary_payload
    services = {item["service"]: item for item in summary_payload["services"]}
    assert {"tavily", "firecrawl"}.issubset(services.keys())
    assert {
        "service",
        "title",
        "description",
        "service_badge",
        "keys_active",
        "keys_total",
        "tokens_total",
        "requests_today",
        "real_remaining",
        "route_label",
        "route_path",
        "last_synced_at",
    }.issubset(services["tavily"].keys())
    assert services["tavily"]["keys_total"] >= 1

    workspace_resp = client.get("/api/admin/search-pool/services/tavily/workspace")
    assert workspace_resp.status_code == 200
    workspace = workspace_resp.json()
    assert workspace["service"] == "tavily"
    assert {"title", "description", "service_badge", "route_label", "route_path"}.issubset(
        workspace["route_summary"].keys()
    )
    assert {
        "keys_total",
        "keys_active",
        "keys_inactive",
        "tokens_total",
        "requests_total",
        "requests_success",
        "requests_failed",
        "requests_today",
        "requests_this_month",
        "success_rate",
        "real_used",
        "real_remaining",
        "real_limit",
        "synced_keys",
        "last_synced_at",
    }.issubset(workspace["stats"].keys())
    assert "curl_examples" in workspace["usage_examples"]
    assert len(workspace["tokens"]) == 1
    assert len(workspace["keys"]) == 1
