"""Proxy forwarding service for search pool gateway."""

from __future__ import annotations

import json
import time
from datetime import datetime, timezone
from typing import Any
from uuid import uuid4

import httpx
from fastapi import HTTPException, Request
from fastapi.responses import JSONResponse, Response

from src.core.logger import logger
from src.modules.search_pool_gateway.models import GatewayUsageLog
from src.modules.search_pool_gateway.repositories.token_repo import GatewayTokenRepository
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


def _mask_secret(value: str | None) -> str:
    if not value:
        return "-"
    trimmed = value.strip()
    if not trimmed:
        return "-"
    if len(trimmed) <= 6:
        return f"{trimmed[:2]}***"
    return f"{trimmed[:4]}***{trimmed[-4:]}"


def _summarize_error(value: Any) -> str:
    text = "" if value is None else str(value).strip()
    if not text:
        return "-"
    single_line = " ".join(text.split())
    return single_line[:200]


def _request_id(request: Request) -> str:
    header_value = request.headers.get("X-Request-ID") or request.headers.get("X-Request-Id")
    if header_value and header_value.strip():
        return header_value.strip()
    return uuid4().hex[:12]


def _format_log_value(value: Any) -> str:
    if value is None:
        return "-"
    if isinstance(value, bool):
        return str(value).lower()
    text = str(value)
    return text.replace("\n", "\\n")


def _log_proxy_event(level: str, **fields: Any) -> None:
    ordered_fields = {
        "service": fields.get("service"),
        "request_id": fields.get("request_id"),
        "stage": fields.get("stage"),
        "gateway_endpoint": fields.get("gateway_endpoint"),
        "upstream_url": fields.get("upstream_url"),
        "method": fields.get("method"),
        "gateway_token_masked": fields.get("gateway_token_masked"),
        "api_key_masked": fields.get("api_key_masked"),
        "status_code": fields.get("status_code"),
        "latency_ms": fields.get("latency_ms"),
        "success": fields.get("success"),
        "error_summary": fields.get("error_summary"),
    }
    message = "[search-pool] " + " ".join(
        f"{key}={_format_log_value(value)}" for key, value in ordered_fields.items()
    )
    getattr(logger, level)(message)


def _record_usage(
    service: str,
    token_id: str | None,
    key_id: str | None,
    endpoint: str,
    success: bool,
    error: str = "",
    latency_ms: int | None = None,
) -> None:
    session_factory = get_session_factory()
    with session_factory() as db:
        row = GatewayUsageLog(
            service=service,
            token_id=token_id,
            api_key_id=key_id,
            endpoint=endpoint,
            success=success,
            latency_ms=latency_ms,
            error_message=error or None,
            created_at=datetime.now(timezone.utc),
        )
        db.add(row)
        db.commit()


async def _authenticate_gateway_token(
    request: Request,
    service: str,
    body: dict[str, Any] | None = None,
    *,
    request_id: str,
    gateway_endpoint: str,
):
    token_value = _extract_bearer_token(request, body)
    if not token_value:
        _log_proxy_event(
            "warning",
            service=service,
            request_id=request_id,
            stage="auth_failed",
            gateway_endpoint=gateway_endpoint,
            method=request.method,
            error_summary="missing gateway token",
        )
        raise HTTPException(status_code=401, detail="Missing API token")

    session_factory = get_session_factory()
    with session_factory() as db:
        token_row = GatewayTokenRepository(db).get_by_token(token_value)
        if token_row is None or token_row.service != service:
            _log_proxy_event(
                "warning",
                service=service,
                request_id=request_id,
                stage="auth_failed",
                gateway_endpoint=gateway_endpoint,
                method=request.method,
                gateway_token_masked=_mask_secret(token_value),
                error_summary="invalid gateway token",
            )
            raise HTTPException(status_code=401, detail="Invalid token")
        return token_row, token_value


def _response_from_upstream(resp: httpx.Response) -> Response:
    content_type = (resp.headers.get("content-type") or "").lower()
    if "application/json" in content_type:
        return JSONResponse(content=resp.json(), status_code=resp.status_code)
    return Response(content=resp.content, status_code=resp.status_code, media_type=content_type or None)


async def proxy_tavily(request: Request, endpoint: str) -> Response:
    body = await request.json()
    request_id = _request_id(request)
    gateway_endpoint = f"/api/{endpoint}"
    upstream_url = f"{TAVILY_API_BASE}/{endpoint}"
    token_row, token_value = await _authenticate_gateway_token(
        request,
        "tavily",
        body,
        request_id=request_id,
        gateway_endpoint=gateway_endpoint,
    )

    leased = get_key_pool().get_next_key("tavily")
    if leased is None:
        _log_proxy_event(
            "warning",
            service="tavily",
            request_id=request_id,
            stage="key_unavailable",
            gateway_endpoint=gateway_endpoint,
            upstream_url=upstream_url,
            method="POST",
            gateway_token_masked=_mask_secret(token_value),
            error_summary="no available upstream api keys",
        )
        raise HTTPException(status_code=503, detail="No available API keys")

    raw_key = leased.raw_key
    body["api_key"] = raw_key
    gateway_token_masked = _mask_secret(token_value)
    api_key_masked = _mask_secret(raw_key)
    _log_proxy_event(
        "info",
        service="tavily",
        request_id=request_id,
        stage="key_selected",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method="POST",
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
    )
    _log_proxy_event(
        "info",
        service="tavily",
        request_id=request_id,
        stage="upstream_request_started",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method="POST",
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
    )
    started_at = time.perf_counter()
    try:
        resp = await forward_http("POST", upstream_url, json_body=body)
    except Exception as exc:  # pragma: no cover
        latency_ms = int((time.perf_counter() - started_at) * 1000)
        get_key_pool().report_result("tavily", leased.id, success=False)
        _record_usage(
            "tavily",
            token_row.id,
            leased.id,
            gateway_endpoint,
            False,
            str(exc),
            latency_ms=latency_ms,
        )
        _log_proxy_event(
            "error",
            service="tavily",
            request_id=request_id,
            stage="upstream_request_failed",
            gateway_endpoint=gateway_endpoint,
            upstream_url=upstream_url,
            method="POST",
            gateway_token_masked=gateway_token_masked,
            api_key_masked=api_key_masked,
            latency_ms=latency_ms,
            success=False,
            error_summary=_summarize_error(exc),
        )
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    ok = resp.status_code < 400
    latency_ms = int((time.perf_counter() - started_at) * 1000)
    get_key_pool().report_result("tavily", leased.id, success=ok)
    _record_usage(
        "tavily",
        token_row.id,
        leased.id,
        gateway_endpoint,
        ok,
        "" if ok else resp.text[:200],
        latency_ms=latency_ms,
    )
    _log_proxy_event(
        "info",
        service="tavily",
        request_id=request_id,
        stage="upstream_response_received",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method="POST",
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
        status_code=resp.status_code,
        latency_ms=latency_ms,
        success=ok,
        error_summary="-" if ok else _summarize_error(resp.text),
    )
    return _response_from_upstream(resp)


async def proxy_firecrawl(path: str, request: Request) -> Response:
    raw_body = await request.body()
    parsed_body = None
    if raw_body and "application/json" in request.headers.get("content-type", "").lower():
        try:
            parsed_body = json.loads(raw_body.decode("utf-8"))
        except Exception:
            parsed_body = None
    request_id = _request_id(request)
    gateway_endpoint = f"/firecrawl/{path}"
    upstream_url = f"{FIRECRAWL_API_BASE}/{path}"
    token_row, token_value = await _authenticate_gateway_token(
        request,
        "firecrawl",
        parsed_body,
        request_id=request_id,
        gateway_endpoint=gateway_endpoint,
    )

    leased = get_key_pool().get_next_key("firecrawl")
    if leased is None:
        _log_proxy_event(
            "warning",
            service="firecrawl",
            request_id=request_id,
            stage="key_unavailable",
            gateway_endpoint=gateway_endpoint,
            upstream_url=upstream_url,
            method=request.method,
            gateway_token_masked=_mask_secret(token_value),
            error_summary="no available upstream api keys",
        )
        raise HTTPException(status_code=503, detail="No available API keys")

    raw_key = leased.raw_key
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
    gateway_token_masked = _mask_secret(token_value)
    api_key_masked = _mask_secret(raw_key)
    _log_proxy_event(
        "info",
        service="firecrawl",
        request_id=request_id,
        stage="key_selected",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method=request.method,
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
    )
    _log_proxy_event(
        "info",
        service="firecrawl",
        request_id=request_id,
        stage="upstream_request_started",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method=request.method,
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
    )
    started_at = time.perf_counter()

    try:
        resp = await forward_http(
            request.method,
            upstream_url,
            headers=headers,
            params=dict(request.query_params),
            content=forward_content if request.method != "GET" else None,
        )
    except Exception as exc:  # pragma: no cover
        latency_ms = int((time.perf_counter() - started_at) * 1000)
        get_key_pool().report_result("firecrawl", leased.id, success=False)
        _record_usage(
            "firecrawl",
            token_row.id,
            leased.id,
            gateway_endpoint,
            False,
            str(exc),
            latency_ms=latency_ms,
        )
        _log_proxy_event(
            "error",
            service="firecrawl",
            request_id=request_id,
            stage="upstream_request_failed",
            gateway_endpoint=gateway_endpoint,
            upstream_url=upstream_url,
            method=request.method,
            gateway_token_masked=gateway_token_masked,
            api_key_masked=api_key_masked,
            latency_ms=latency_ms,
            success=False,
            error_summary=_summarize_error(exc),
        )
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    ok = resp.status_code < 400
    latency_ms = int((time.perf_counter() - started_at) * 1000)
    get_key_pool().report_result("firecrawl", leased.id, success=ok)
    _record_usage(
        "firecrawl",
        token_row.id,
        leased.id,
        gateway_endpoint,
        ok,
        "" if ok else resp.text[:200],
        latency_ms=latency_ms,
    )
    _log_proxy_event(
        "info",
        service="firecrawl",
        request_id=request_id,
        stage="upstream_response_received",
        gateway_endpoint=gateway_endpoint,
        upstream_url=upstream_url,
        method=request.method,
        gateway_token_masked=gateway_token_masked,
        api_key_masked=api_key_masked,
        status_code=resp.status_code,
        latency_ms=latency_ms,
        success=ok,
        error_summary="-" if ok else _summarize_error(resp.text),
    )
    return _response_from_upstream(resp)
