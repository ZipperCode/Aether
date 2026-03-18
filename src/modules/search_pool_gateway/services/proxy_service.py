"""Proxy forwarding service for search pool gateway."""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any

import httpx
from fastapi import HTTPException, Request
from fastapi.responses import JSONResponse, Response

from src.modules.search_pool_gateway.models import GatewayUsageLog
from src.modules.search_pool_gateway.repositories.token_repo import GatewayTokenRepository
from src.modules.search_pool_gateway.services.crypto import GatewayCryptoService
from src.modules.search_pool_gateway.services.key_pool import ServiceKeyPool
from src.modules.search_pool_gateway.sqlite import get_session_factory

TAVILY_API_BASE = "https://api.tavily.com"
FIRECRAWL_API_BASE = "https://api.firecrawl.dev"

_http_client = httpx.AsyncClient(timeout=60)
_key_pool: ServiceKeyPool | None = None
_key_pool_session_factory = None


async def forward_http(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    params: dict[str, Any] | None = None,
    content: bytes | None = None,
    json_body: dict[str, Any] | None = None,
) -> httpx.Response:
    return await _http_client.request(
        method,
        url,
        headers=headers,
        params=params,
        content=content,
        json=json_body,
    )


def get_key_pool() -> ServiceKeyPool:
    global _key_pool
    global _key_pool_session_factory
    session_factory = get_session_factory()
    if _key_pool is None or _key_pool_session_factory is not session_factory:
        _key_pool = ServiceKeyPool(session_factory)
        _key_pool_session_factory = session_factory
    return _key_pool


def _extract_bearer_token(request: Request, body: dict[str, Any] | None = None) -> str | None:
    auth = request.headers.get("Authorization", "")
    if auth.startswith("Bearer "):
        return auth[7:].strip()
    if body and isinstance(body.get("api_key"), str):
        return body["api_key"].strip()
    return None


def _record_usage(service: str, token_id: str | None, key_id: str | None, endpoint: str, success: bool, error: str = "") -> None:
    session_factory = get_session_factory()
    with session_factory() as db:
        row = GatewayUsageLog(
            service=service,
            token_id=token_id,
            api_key_id=key_id,
            endpoint=endpoint,
            success=success,
            error_message=error or None,
            created_at=datetime.now(timezone.utc),
        )
        db.add(row)
        db.commit()


async def _authenticate_gateway_token(request: Request, service: str, body: dict[str, Any] | None = None):
    token_value = _extract_bearer_token(request, body)
    if not token_value:
        raise HTTPException(status_code=401, detail="Missing API token")

    session_factory = get_session_factory()
    with session_factory() as db:
        token_row = GatewayTokenRepository(db).get_by_token(token_value)
        if token_row is None or token_row.service != service:
            raise HTTPException(status_code=401, detail="Invalid token")
        return token_row


def _response_from_upstream(resp: httpx.Response) -> Response:
    content_type = (resp.headers.get("content-type") or "").lower()
    if "application/json" in content_type:
        return JSONResponse(content=resp.json(), status_code=resp.status_code)
    return Response(content=resp.content, status_code=resp.status_code, media_type=content_type or None)


async def proxy_tavily(request: Request, endpoint: str) -> Response:
    body = await request.json()
    token_row = await _authenticate_gateway_token(request, "tavily", body)

    leased = get_key_pool().get_next_key("tavily")
    if leased is None:
        raise HTTPException(status_code=503, detail="No available API keys")

    raw_key = GatewayCryptoService().decrypt(leased.key_encrypted)
    body["api_key"] = raw_key
    try:
        resp = await forward_http("POST", f"{TAVILY_API_BASE}/{endpoint}", json_body=body)
    except Exception as exc:  # pragma: no cover
        get_key_pool().report_result("tavily", leased.id, success=False)
        _record_usage("tavily", token_row.id, leased.id, f"/api/{endpoint}", False, str(exc))
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    ok = resp.status_code < 400
    get_key_pool().report_result("tavily", leased.id, success=ok)
    _record_usage("tavily", token_row.id, leased.id, f"/api/{endpoint}", ok, "" if ok else resp.text[:200])
    return _response_from_upstream(resp)


async def proxy_firecrawl(path: str, request: Request) -> Response:
    raw_body = await request.body()
    parsed_body = None
    if raw_body and "application/json" in request.headers.get("content-type", "").lower():
        try:
            parsed_body = json.loads(raw_body.decode("utf-8"))
        except Exception:
            parsed_body = None
    token_row = await _authenticate_gateway_token(request, "firecrawl", parsed_body)

    leased = get_key_pool().get_next_key("firecrawl")
    if leased is None:
        raise HTTPException(status_code=503, detail="No available API keys")

    raw_key = GatewayCryptoService().decrypt(leased.key_encrypted)
    forward_content = raw_body
    if isinstance(parsed_body, dict) and "api_key" in parsed_body:
        parsed_body["api_key"] = raw_key
        forward_content = json.dumps(parsed_body).encode("utf-8")

    headers = {
        key: value
        for key, value in request.headers.items()
        if key.lower() not in {"authorization", "content-length", "host"}
    }
    headers["Authorization"] = f"Bearer {raw_key}"

    try:
        resp = await forward_http(
            request.method,
            f"{FIRECRAWL_API_BASE}/{path}",
            headers=headers,
            params=dict(request.query_params),
            content=forward_content if request.method != "GET" else None,
        )
    except Exception as exc:  # pragma: no cover
        get_key_pool().report_result("firecrawl", leased.id, success=False)
        _record_usage("firecrawl", token_row.id, leased.id, f"/firecrawl/{path}", False, str(exc))
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    ok = resp.status_code < 400
    get_key_pool().report_result("firecrawl", leased.id, success=ok)
    _record_usage("firecrawl", token_row.id, leased.id, f"/firecrawl/{path}", ok, "" if ok else resp.text[:200])
    return _response_from_upstream(resp)
