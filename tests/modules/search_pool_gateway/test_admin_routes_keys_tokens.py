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


def test_admin_can_create_list_toggle_delete_key_and_token(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-test-key-1234567890", "email": "a@example.com"},
    )
    assert create_key.status_code == 200
    key_id = create_key.json()["id"]

    list_keys = client.get("/api/admin/search-pool/keys?service=tavily")
    assert list_keys.status_code == 200
    assert len(list_keys.json()["keys"]) == 1

    toggle = client.put(f"/api/admin/search-pool/keys/{key_id}/toggle", json={"active": False})
    assert toggle.status_code == 200
    assert toggle.json()["active"] is False

    create_token = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "tavily", "name": "dev", "hourly_limit": 10, "daily_limit": 100, "monthly_limit": 1000},
    )
    assert create_token.status_code == 200
    token_id = create_token.json()["id"]

    list_tokens = client.get("/api/admin/search-pool/tokens?service=tavily")
    assert list_tokens.status_code == 200
    assert len(list_tokens.json()["tokens"]) == 1

    delete_token = client.delete(f"/api/admin/search-pool/tokens/{token_id}")
    assert delete_token.status_code == 200

    delete_key = client.delete(f"/api/admin/search-pool/keys/{key_id}")
    assert delete_key.status_code == 200
