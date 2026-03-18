"""Search Pool Gateway admin routes."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException

from src.modules.search_pool_gateway.schemas import CreateKeyRequest, CreateTokenRequest, ToggleKeyRequest, UsageSyncRequest
from src.modules.search_pool_gateway.services.key_service import GatewayKeyService
from src.modules.search_pool_gateway.services.token_service import GatewayTokenService
from src.modules.search_pool_gateway.services.usage_service import GatewayUsageService
from src.modules.search_pool_gateway.sqlite import get_session_factory
from src.utils.auth_utils import require_admin

router = APIRouter(prefix="/api/admin/search-pool", tags=["Admin - Search Pool Gateway"])


@router.get("/keys")
async def list_keys(service: str | None = None, _: Any = Depends(require_admin)) -> dict[str, list[dict[str, Any]]]:
    session_factory = get_session_factory()
    with session_factory() as db:
        rows = GatewayKeyService(db).list_keys(service)
        return {
            "keys": [
                {
                    "id": row.id,
                    "service": row.service,
                    "key_masked": row.key_masked,
                    "email": row.email,
                    "active": row.active,
                }
                for row in rows
            ]
        }


@router.post("/keys")
async def create_key(payload: CreateKeyRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            row = GatewayKeyService(db).create_key(service=payload.service, raw_key=payload.key, email=payload.email)
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc
        return {
            "id": row.id,
            "service": row.service,
            "key_masked": row.key_masked,
            "email": row.email,
            "active": row.active,
        }


@router.put("/keys/{key_id}/toggle")
async def toggle_key(key_id: str, payload: ToggleKeyRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            row = GatewayKeyService(db).set_active(key_id, payload.active)
        except ValueError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc
        return {
            "id": row.id,
            "active": row.active,
        }


@router.delete("/keys/{key_id}")
async def delete_key(key_id: str, _: Any = Depends(require_admin)) -> dict[str, bool]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            GatewayKeyService(db).delete(key_id)
        except ValueError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc
        return {"ok": True}


@router.get("/tokens")
async def list_tokens(service: str | None = None, _: Any = Depends(require_admin)) -> dict[str, list[dict[str, Any]]]:
    session_factory = get_session_factory()
    with session_factory() as db:
        rows = GatewayTokenService(db).list_tokens(service)
        return {
            "tokens": [
                {
                    "id": row.id,
                    "service": row.service,
                    "token": row.token,
                    "name": row.name,
                    "hourly_limit": row.hourly_limit,
                    "daily_limit": row.daily_limit,
                    "monthly_limit": row.monthly_limit,
                }
                for row in rows
            ]
        }


@router.post("/tokens")
async def create_token(payload: CreateTokenRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            row = GatewayTokenService(db).create_token(
                service=payload.service,
                name=payload.name,
                hourly_limit=payload.hourly_limit,
                daily_limit=payload.daily_limit,
                monthly_limit=payload.monthly_limit,
            )
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc
        return {
            "id": row.id,
            "service": row.service,
            "token": row.token,
            "name": row.name,
            "hourly_limit": row.hourly_limit,
            "daily_limit": row.daily_limit,
            "monthly_limit": row.monthly_limit,
        }


@router.delete("/tokens/{token_id}")
async def delete_token(token_id: str, _: Any = Depends(require_admin)) -> dict[str, bool]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            GatewayTokenService(db).delete(token_id)
        except ValueError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc
        return {"ok": True}


@router.post("/usage/sync")
async def sync_usage(payload: UsageSyncRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        result = GatewayUsageService(db).sync(service=payload.service, force=payload.force)
        return {"result": result}


@router.get("/stats/overview")
async def stats_overview(service: str | None = None, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        return GatewayUsageService(db).stats_overview(service=service)
