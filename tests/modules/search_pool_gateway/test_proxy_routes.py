from __future__ import annotations

from collections.abc import Callable

import pytest
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

    from src.modules.search_pool_gateway.sqlite import get_engine
    from src.modules.search_pool_gateway.routes_admin import router as admin_router
    from src.modules.search_pool_gateway.routes_proxy import router as proxy_router

    get_engine(reset=True)

    app = FastAPI()
    app.include_router(admin_router)
    app.include_router(proxy_router)
    app.dependency_overrides[require_admin] = lambda: object()
    return TestClient(app)


@pytest.fixture
def proxy_service_module():
    from src.modules.search_pool_gateway.services import proxy_service

    return proxy_service


def _capture_logger(monkeypatch, proxy_service_module) -> dict[str, list[str]]:
    calls: dict[str, list[str]] = {
        "info": [],
        "warning": [],
        "error": [],
    }

    class _LoggerSpy:
        def info(self, message: str, *args) -> None:  # noqa: ANN001
            calls["info"].append(message.format(*args))

        def warning(self, message: str, *args) -> None:  # noqa: ANN001
            calls["warning"].append(message.format(*args))

        def error(self, message: str, *args) -> None:  # noqa: ANN001
            calls["error"].append(message.format(*args))

    monkeypatch.setattr(proxy_service_module, "logger", _LoggerSpy())
    return calls


def _create_gateway_credentials(client: TestClient, *, service: str, key: str, token_name: str) -> str:
    key_resp = client.post(
        "/api/admin/search-pool/keys",
        json={"service": service, "key": key},
    )
    assert key_resp.status_code == 200

    token_resp = client.post(
        "/api/admin/search-pool/tokens",
        json={"service": service, "name": token_name},
    )
    assert token_resp.status_code == 200
    return token_resp.json()["token"]


def _fake_forward_response(payload: dict, *, status_code: int = 200) -> Callable:
    async def _fake_forward(*args, **kwargs):  # noqa: ANN002, ANN003
        return _FakeResponse(status_code, payload)

    return _fake_forward


def test_proxy_search_uses_gateway_token_and_forwards(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    token = _create_gateway_credentials(
        client,
        service="tavily",
        key="tvly-key-abcdef1234567890",
        token_name="tavily-client",
    )

    from src.modules.search_pool_gateway.services import proxy_service

    monkeypatch.setattr(
        proxy_service,
        "forward_http",
        _fake_forward_response({"ok": True, "service": "tavily"}),
    )

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )
    assert resp.status_code == 200
    assert resp.json()["ok"] is True


def test_proxy_search_uses_plaintext_key_without_crypto_service(tmp_path, monkeypatch, proxy_service_module):
    client = _build_client(tmp_path, monkeypatch)

    token = _create_gateway_credentials(
        client,
        service="tavily",
        key="tvly-key-plaintext-1234567890",
        token_name="tavily-client",
    )

    captured_body: dict[str, object] = {}

    async def _fake_forward(method: str, url: str, **kwargs):  # noqa: ANN001
        captured_body["method"] = method
        captured_body["url"] = url
        captured_body["json_body"] = kwargs.get("json_body")
        return _FakeResponse(200, {"ok": True, "service": "tavily"})

    monkeypatch.setattr(proxy_service_module, "forward_http", _fake_forward)
    assert not hasattr(proxy_service_module, "GatewayCryptoService")

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )
    assert resp.status_code == 200
    assert captured_body["json_body"]["api_key"] == "tvly-key-plaintext-1234567890"


def test_proxy_firecrawl_route_uses_firecrawl_token(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)

    token = _create_gateway_credentials(
        client,
        service="firecrawl",
        key="fc-key-abcdef1234567890",
        token_name="firecrawl-client",
    )

    from src.modules.search_pool_gateway.services import proxy_service

    monkeypatch.setattr(
        proxy_service,
        "forward_http",
        _fake_forward_response({"ok": True, "service": "firecrawl"}),
    )

    resp = client.post(
        "/firecrawl/v2/scrape",
        headers={"Authorization": f"Bearer {token}"},
        json={"url": "https://example.com"},
    )
    assert resp.status_code == 200
    assert resp.json()["service"] == "firecrawl"


def test_proxy_tavily_route_logs_masked_request_lifecycle(tmp_path, monkeypatch, proxy_service_module):
    client = _build_client(tmp_path, monkeypatch)
    token = _create_gateway_credentials(
        client,
        service="tavily",
        key="tvly-secret-123456",
        token_name="tavily-client",
    )
    log_calls = _capture_logger(monkeypatch, proxy_service_module)

    monkeypatch.setattr(
        proxy_service_module,
        "forward_http",
        _fake_forward_response({"ok": True, "service": "tavily"}),
    )

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )

    assert resp.status_code == 200
    assert len(log_calls["info"]) >= 3
    assert "stage=key_selected" in log_calls["info"][0]
    assert "stage=upstream_request_started" in log_calls["info"][1]
    assert "stage=upstream_response_received" in log_calls["info"][2]
    rendered_messages = "\n".join(log_calls["info"])
    assert token not in rendered_messages
    assert "tvly-secret-123456" not in rendered_messages


def test_proxy_tavily_route_logs_auth_failed(tmp_path, monkeypatch, proxy_service_module):
    client = _build_client(tmp_path, monkeypatch)
    log_calls = _capture_logger(monkeypatch, proxy_service_module)

    resp = client.post("/api/search", json={"query": "hello"})

    assert resp.status_code == 401
    assert log_calls["warning"]
    assert "stage=auth_failed" in log_calls["warning"][0]


def test_proxy_tavily_route_logs_key_unavailable(tmp_path, monkeypatch, proxy_service_module):
    client = _build_client(tmp_path, monkeypatch)
    token = _create_gateway_credentials(
        client,
        service="tavily",
        key="tvly-key-unused-123456",
        token_name="tavily-client",
    )
    log_calls = _capture_logger(monkeypatch, proxy_service_module)

    def _fake_get_next_key(service: str):  # noqa: ARG001
        return None

    monkeypatch.setattr(proxy_service_module.get_key_pool(), "get_next_key", _fake_get_next_key)

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )

    assert resp.status_code == 503
    assert log_calls["warning"]
    assert "stage=key_unavailable" in log_calls["warning"][0]


def test_proxy_tavily_route_logs_upstream_failure(tmp_path, monkeypatch, proxy_service_module):
    client = _build_client(tmp_path, monkeypatch)
    token = _create_gateway_credentials(
        client,
        service="tavily",
        key="tvly-secret-654321",
        token_name="tavily-client",
    )
    log_calls = _capture_logger(monkeypatch, proxy_service_module)

    async def _failing_forward(*args, **kwargs):  # noqa: ANN002, ANN003
        raise RuntimeError("upstream boom")

    monkeypatch.setattr(proxy_service_module, "forward_http", _failing_forward)

    resp = client.post(
        "/api/search",
        headers={"Authorization": f"Bearer {token}"},
        json={"query": "hello"},
    )

    assert resp.status_code == 502
    assert log_calls["error"]
    assert "stage=upstream_request_failed" in log_calls["error"][0]
    assert "upstream boom" in log_calls["error"][0]
