from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.utils.auth_utils import require_admin


def _build_client(tmp_path, monkeypatch) -> TestClient:
    db_path = tmp_path / "tavilies.db"
    monkeypatch.setenv("TAVILY_POOL_DB_PATH", str(db_path))
    monkeypatch.setenv("TAVILY_POOL_CRYPTO_KEY", "tavily-route-key")

    from src.modules.tavily_pool.sqlite import get_engine

    get_engine(reset=True)

    from src.modules.tavily_pool.routes import router

    app = FastAPI()
    app.include_router(router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


def test_admin_can_crud_account_and_manage_tokens(tmp_path, monkeypatch):
    from src.modules.tavily_pool.sqlite import reset_engine

    try:
        client = _build_client(tmp_path, monkeypatch)

        create_resp = client.post(
            "/api/admin/tavily-pool/accounts",
            json={"email": "admin@example.com", "password": "pass-001", "source": "manual"},
        )
        assert create_resp.status_code == 200
        account_id = create_resp.json()["id"]

        token_resp = client.post(
            f"/api/admin/tavily-pool/accounts/{account_id}/tokens",
            json={"token": "tvly-route-token-0001"},
        )
        assert token_resp.status_code == 200
        token_id = token_resp.json()["id"]

        activate_resp = client.post(f"/api/admin/tavily-pool/tokens/{token_id}/activate")
        assert activate_resp.status_code == 200
        assert activate_resp.json()["id"] == token_id

        list_resp = client.get(f"/api/admin/tavily-pool/accounts/{account_id}/tokens")
        assert list_resp.status_code == 200
        assert len(list_resp.json()) == 1
        assert list_resp.json()[0]["is_active"] is True

        health_resp = client.post("/api/admin/tavily-pool/health-check/run")
        assert health_resp.status_code == 200
        assert health_resp.json()["total"] >= 1

        maintenance_resp = client.post("/api/admin/tavily-pool/maintenance/run")
        assert maintenance_resp.status_code == 200
        assert maintenance_resp.json()["total"] >= 1

        history_resp = client.get("/api/admin/tavily-pool/maintenance/runs")
        assert history_resp.status_code == 200
        assert len(history_resp.json()) >= 1
    finally:
        reset_engine()
