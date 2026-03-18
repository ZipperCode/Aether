from __future__ import annotations

from fastapi import FastAPI
from fastapi.testclient import TestClient

from src.utils.auth_utils import require_admin


class _FakeResponse:
    def __init__(self, status_code: int, payload: dict, content_type: str = "application/json") -> None:
        self.status_code = status_code
        self._payload = payload
        self.headers = {"content-type": content_type}
        self.content = b"{}"
        self.text = "{}"

    def json(self):
        return self._payload


def _build_client(tmp_path, monkeypatch) -> TestClient:
    db_path = tmp_path / "search_pool_gateway.db"
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_DB_PATH", str(db_path))
    monkeypatch.setenv("SEARCH_POOL_GATEWAY_CRYPTO_KEY", "search-pool-gateway-test-key")

    from src.modules.search_pool_gateway.sqlite import get_engine
    from src.modules.search_pool_gateway.routes_admin import router as admin_router
    from src.modules.search_pool_gateway.routes_proxy import router as proxy_router

    get_engine(reset=True)

    app = FastAPI()
    app.include_router(admin_router)
    app.include_router(proxy_router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


def test_proxy_search_uses_gateway_token_and_forwards(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    key_resp = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-key-abcdef1234567890"},
    )
    assert key_resp.status_code == 200

    token_resp = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "tavily", "name": "tavily-client"},
    )
    assert token_resp.status_code == 200
    token = token_resp.json()["token"]

    from src.modules.search_pool_gateway.services import proxy_service

    async def _fake_forward(*args, **kwargs):  # noqa: ANN002, ANN003
        return _FakeResponse(200, {"ok": True, "service": "tavily"})

    monkeypatch.setattr(proxy_service, "forward_http", _fake_forward)

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )
    assert resp.status_code == 200
    assert resp.json()["ok"] is True


def test_proxy_firecrawl_route_uses_firecrawl_token(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    key_resp = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "firecrawl", "key": "fc-key-abcdef1234567890"},
    )
    assert key_resp.status_code == 200

    token_resp = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": "firecrawl", "name": "firecrawl-client"},
    )
    assert token_resp.status_code == 200
    token = token_resp.json()["token"]

    from src.modules.search_pool_gateway.services import proxy_service

    async def _fake_forward(*args, **kwargs):  # noqa: ANN002, ANN003
        return _FakeResponse(200, {"ok": True, "service": "firecrawl"})

    monkeypatch.setattr(proxy_service, "forward_http", _fake_forward)

    resp = client.post(
        "/firecrawl/v2/scrape",
        headers={"Authorization": f"Bearer {token}"},
        json={"url": "https://example.com"},
    )
    assert resp.status_code == 200
    assert resp.json()["service"] == "firecrawl"
