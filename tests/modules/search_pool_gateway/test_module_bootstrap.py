from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.utils.auth_utils import require_admin


def _build_client(tmp_path, monkeypatch) -> TestClient:
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))

    from src.modules.search_pool_gateway.sqlite import get_engine

    get_engine(reset=True)

    from src.modules.search_pool_gateway.routes_admin import router as admin_router

    app = FastAPI()
    app.include_router(admin_router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


def test_search_pool_gateway_admin_router_registered(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)
    resp = client.get("/api/admin/search-pool/keys")
    assert resp.status_code == 200
    assert resp.json() == {"keys": []}
