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
        from src.modules.tavily_pool.services.health_service import TavilyHealthService
        from src.modules.tavily_pool.services.usage_service import TavilyUsageService

        def _fake_probe(self, _token):  # noqa: ANN001
            return True, "", 11

        def _fake_sync(self):  # noqa: ANN001
            return {"total_accounts": 1, "synced_accounts": 1, "failed_accounts": 0}

        monkeypatch.setattr(TavilyHealthService, "_probe_token", _fake_probe)
        monkeypatch.setattr(TavilyUsageService, "run_usage_sync", _fake_sync)

        client = _build_client(tmp_path, monkeypatch)

        create_resp = client.post(
            "/api/admin/tavily-pool/accounts",
            json={
                "email": "admin@example.com",
                "password": "pass-001",
                "api_key": "tvly-route-create-0001",
                "source": "manual",
            },
        )
        assert create_resp.status_code == 200
        account_id = create_resp.json()["id"]

        seeded_tokens_resp = client.get(f"/api/admin/tavily-pool/accounts/{account_id}/tokens")
        assert seeded_tokens_resp.status_code == 200
        assert len(seeded_tokens_resp.json()) == 1

        token_resp = client.post(
            f"/api/admin/tavily-pool/accounts/{account_id}/tokens",
            json={"token": "tvly-route-token-0001"},
        )
        assert token_resp.status_code == 200
        token_id = token_resp.json()["id"]

        activate_resp = client.post(f"/api/admin/tavily-pool/tokens/{token_id}/activate")
        assert activate_resp.status_code == 200
        assert activate_resp.json()["id"] == token_id

        disable_resp = client.put(
            f"/api/admin/tavily-pool/accounts/{account_id}/status",
            json={"status": "disabled"},
        )
        assert disable_resp.status_code == 200
        assert disable_resp.json()["status"] == "disabled"

        enable_resp = client.put(
            f"/api/admin/tavily-pool/accounts/{account_id}/status",
            json={"status": "active"},
        )
        assert enable_resp.status_code == 200
        assert enable_resp.json()["status"] == "active"

        list_resp = client.get(f"/api/admin/tavily-pool/accounts/{account_id}/tokens")
        assert list_resp.status_code == 200
        assert len(list_resp.json()) == 2
        active_count = sum(1 for item in list_resp.json() if item["is_active"])
        assert active_count == 1

        lease_resp = client.post("/api/admin/tavily-pool/pool/lease")
        assert lease_resp.status_code == 200
        lease_payload = lease_resp.json()
        assert lease_payload["token_id"] == token_id
        assert lease_payload["token"].startswith("tvly-")

        report_resp = client.post(
            "/api/admin/tavily-pool/pool/report",
            json={"token_id": token_id, "success": True, "endpoint": "/search", "latency_ms": 80},
        )
        assert report_resp.status_code == 200
        assert report_resp.json()["success"] is True

        stats_resp = client.get("/api/admin/tavily-pool/stats/overview")
        assert stats_resp.status_code == 200
        assert stats_resp.json()["total_requests"] >= 1

        health_resp = client.post("/api/admin/tavily-pool/health-check/run")
        assert health_resp.status_code == 200
        assert health_resp.json()["total"] >= 1

        maintenance_resp = client.post("/api/admin/tavily-pool/maintenance/run")
        assert maintenance_resp.status_code == 200
        assert maintenance_resp.json()["total"] >= 1

        sync_resp = client.post("/api/admin/tavily-pool/usage/sync")
        assert sync_resp.status_code == 200
        assert sync_resp.json()["synced_accounts"] == 1

        delete_token_resp = client.delete(f"/api/admin/tavily-pool/tokens/{token_id}")
        assert delete_token_resp.status_code == 200

        history_resp = client.get("/api/admin/tavily-pool/maintenance/runs")
        assert history_resp.status_code == 200
        assert len(history_resp.json()) >= 1
    finally:
        reset_engine()


def test_admin_can_import_accounts_json(tmp_path, monkeypatch):
    from src.modules.tavily_pool.sqlite import reset_engine

    try:
        client = _build_client(tmp_path, monkeypatch)
        resp = client.post(
            "/api/admin/tavily-pool/accounts/import",
            json={
                "file_type": "json",
                "merge_mode": "skip",
                "content": (
                    '[{"email":"route-import@example.com","password":"pwd",'
                    '"api_key":"tvly-route-import-0001"}]'
                ),
            },
        )
        assert resp.status_code == 200
        payload = resp.json()
        assert payload["stats"]["total"] == 1
        assert payload["stats"]["created"] == 1
        assert payload["stats"]["failed"] == 0
    finally:
        reset_engine()
