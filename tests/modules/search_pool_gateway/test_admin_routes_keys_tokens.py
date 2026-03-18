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


def test_admin_create_key_persists_plaintext_raw_key(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-test-key-plaintext-1234567890", "email": "a@example.com"},
    )
    assert create_key.status_code == 200
    key_id = create_key.json()["id"]

    from src.modules.search_pool_gateway.models import GatewayApiKey
    from src.modules.search_pool_gateway.sqlite import get_session_factory

    session_factory = get_session_factory()
    with session_factory() as db:
        row = db.get(GatewayApiKey, key_id)
        assert row is not None
        assert row.raw_key == "tvly-test-key-plaintext-1234567890"
        assert row.key_masked != row.raw_key


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


def test_admin_can_import_keys_and_update_token_limits(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    import_resp = client.post(
        "/api/admin/search-pool/keys/import",
        json={
            "service": "firecrawl",
            "content": "\n".join(
                [
                    "alice@example.com,fc-key-001",
                    "bob@example.com,password,fc-key-002",
                    "fc-key-003",
                ]
            ),
        },
    )
    assert import_resp.status_code == 200
    import_payload = import_resp.json()
    assert import_payload["created"] == 3
    assert import_payload["service"] == "firecrawl"
    assert len(import_payload["keys"]) == 3
    assert import_payload["keys"][0]["email"] == "alice@example.com"

    list_keys = client.get("/api/admin/search-pool/keys?service=firecrawl")
    assert list_keys.status_code == 200
    keys = list_keys.json()["keys"]
    assert len(keys) == 3
    assert {
        "usage_key_used",
        "usage_key_limit",
        "usage_key_remaining",
        "usage_account_plan",
        "usage_account_used",
        "usage_account_limit",
        "usage_account_remaining",
        "usage_synced_at",
        "usage_sync_error",
        "last_used_at",
        "total_used",
        "total_failed",
        "consecutive_fails",
    }.issubset(keys[0].keys())

    create_token = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "firecrawl", "name": "ops", "hourly_limit": 10, "daily_limit": 100, "monthly_limit": 500},
    )
    assert create_token.status_code == 200
    token_id = create_token.json()["id"]

    update_token = client.put(
        f"/api/admin/search-pool/tokens/{token_id}",
        json={"name": "ops-updated", "hourly_limit": 20, "daily_limit": 120, "monthly_limit": 900},
    )
    assert update_token.status_code == 200
    updated = update_token.json()
    assert updated["name"] == "ops-updated"
    assert updated["hourly_limit"] == 20
    assert updated["daily_limit"] == 120
    assert updated["monthly_limit"] == 900

    list_tokens = client.get("/api/admin/search-pool/tokens?service=firecrawl")
    assert list_tokens.status_code == 200
    tokens = list_tokens.json()["tokens"]
    assert len(tokens) == 1
    assert {
        "created_at",
        "updated_at",
        "usage_success",
        "usage_failed",
        "usage_this_month",
    }.issubset(tokens[0].keys())
