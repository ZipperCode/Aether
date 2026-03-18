from __future__ import annotations

from typing import Any

import pytest
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


@pytest.fixture
def usage_service_module():
    from src.modules.search_pool_gateway.services import usage_service

    return usage_service


def _capture_sync_logger(monkeypatch, usage_service_module) -> dict[str, list[str]]:
    calls: dict[str, list[str]] = {
        "info": [],
        "warning": [],
        "error": [],
    }

    class _LoggerSpy:
        def info(self, message: str, *args: Any) -> None:
            calls["info"].append(message.format(*args))

        def warning(self, message: str, *args: Any) -> None:
            calls["warning"].append(message.format(*args))

        def error(self, message: str, *args: Any) -> None:
            calls["error"].append(message.format(*args))

    monkeypatch.setattr(usage_service_module, "logger", _LoggerSpy(), raising=False)
    return calls


def test_admin_usage_sync_updates_usage_fields_from_tavily_console(tmp_path, monkeypatch, usage_service_module):
    client = _build_client(tmp_path, monkeypatch)
    monkeypatch.setenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", "appSession=test-cookie")

    def _fake_fetch_keys(cookie_header: str) -> list[dict[str, Any]]:
        assert cookie_header == "appSession=test-cookie"
        return [
            {
                "key": "tvly-dev-xxx",
                "limit": 2147483647,
                "usage": 18,
                "key_type": "development",
                "search_egress_policy": "allow_external",
                "name": "default",
            }
        ]

    monkeypatch.setattr(
        usage_service_module,
        "_fetch_tavily_console_keys",
        _fake_fetch_keys,
        raising=False,
    )

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-xxx"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    payload = sync_resp.json()
    assert "result" in payload
    assert payload["result"]["service"] == "tavily"
    assert payload["result"]["synced_keys"] >= 1

    keys_resp = client.get("/api/admin/search-pool/keys?service=tavily")
    assert keys_resp.status_code == 200
    key = keys_resp.json()["keys"][0]
    assert key["usage_key_limit"] == 2147483647
    assert key["usage_key_used"] == 18
    assert key["usage_key_remaining"] == 2147483629
    assert key["usage_account_plan"] == "development"
    assert key["usage_sync_error"] == ""
    assert key["usage_synced_at"] is not None


def test_admin_usage_sync_matches_tavily_key_without_crypto_service(tmp_path, monkeypatch, usage_service_module):
    client = _build_client(tmp_path, monkeypatch)
    monkeypatch.setenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", "appSession=test-cookie")

    monkeypatch.setattr(
        usage_service_module,
        "_fetch_tavily_console_keys",
        lambda cookie_header: [  # noqa: ARG005
            {
                "key": "tvly-dev-plain-xxx",
                "limit": 100,
                "usage": 8,
                "key_type": "development",
            }
        ],
        raising=False,
    )

    assert not hasattr(usage_service_module, "GatewayCryptoService")

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-plain-xxx"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    result = sync_resp.json()["result"]
    assert result["synced_keys"] == 1
    assert result["errors"] == 0


def test_admin_usage_sync_reports_missing_tavily_cookie(tmp_path, monkeypatch):
    client = _build_client(tmp_path, monkeypatch)
    monkeypatch.delenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", raising=False)

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-xxx"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    result = sync_resp.json()["result"]
    assert result["service"] == "tavily"
    assert result["errors"] == 1
    assert "cookie" in result["message"].lower()


def test_admin_usage_sync_logs_tavily_lifecycle(tmp_path, monkeypatch, usage_service_module):
    client = _build_client(tmp_path, monkeypatch)
    monkeypatch.setenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", "appSession=test-cookie")
    log_calls = _capture_sync_logger(monkeypatch, usage_service_module)

    monkeypatch.setattr(
        usage_service_module,
        "_fetch_tavily_console_keys",
        lambda cookie_header: [  # noqa: ARG005
            {
                "key": "tvly-dev-xxx",
                "limit": 2147483647,
                "usage": 18,
                "key_type": "development",
            }
        ],
        raising=False,
    )

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-xxx"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    assert log_calls["info"]
    assert "stage=sync_started" in log_calls["info"][0]
    assert "cookie_present=true" in log_calls["info"][0]
    assert "stage=sync_completed" in log_calls["info"][-1]


def test_admin_usage_sync_marks_key_error_when_tavily_payload_is_incomplete(
    tmp_path, monkeypatch, usage_service_module
):
    client = _build_client(tmp_path, monkeypatch)
    monkeypatch.setenv("SEARCH_POOL_TAVILY_SYNC_COOKIE", "appSession=test-cookie")

    monkeypatch.setattr(
        usage_service_module,
        "_fetch_tavily_console_keys",
        lambda cookie_header: [  # noqa: ARG005
            {
                "key": "tvly-dev-xxx",
                "limit": 100,
                "key_type": "development",
            }
        ],
        raising=False,
    )

    create_key = client.post(
        "/api/admin/search-pool/keys",
        json={"service": "tavily", "key": "tvly-dev-xxx"},
    )
    assert create_key.status_code == 200

    sync_resp = client.post("/api/admin/search-pool/usage/sync", json={"service": "tavily", "force": True})
    assert sync_resp.status_code == 200
    result = sync_resp.json()["result"]
    assert result["errors"] == 1

    keys_resp = client.get("/api/admin/search-pool/keys?service=tavily")
    assert keys_resp.status_code == 200
    key = keys_resp.json()["keys"][0]
    assert key["usage_key_limit"] == 100
    assert key["usage_key_used"] is None
    assert key["usage_key_remaining"] is None
    assert "missing usage fields" in key["usage_sync_error"].lower()


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
