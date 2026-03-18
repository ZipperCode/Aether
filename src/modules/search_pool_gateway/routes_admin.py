"""Search Pool Gateway admin routes."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException

from src.modules.search_pool_gateway.schemas import (
    CreateKeyRequest,
    CreateTokenRequest,
    ImportKeysRequest,
    ToggleKeyRequest,
    UpdateTokenRequest,
    UsageSyncRequest,
)
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
        usage_service = GatewayUsageService(db)
        return {"keys": [usage_service.serialize_key(row) for row in rows]}


@router.post("/keys")
async def create_key(payload: CreateKeyRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            row = GatewayKeyService(db).create_key(service=payload.service, raw_key=payload.key, email=payload.email)
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc
        return GatewayUsageService(db).serialize_key(row)


@router.post("/keys/import")
async def import_keys(payload: ImportKeysRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            rows = GatewayKeyService(db).import_keys(service=payload.service, content=payload.content, keys=payload.keys)
        except ValueError as exc:
            raise HTTPException(status_code=400, detail=str(exc)) from exc
        usage_service = GatewayUsageService(db)
        return {
            "service": payload.service.strip().lower(),
            "created": len(rows),
            "keys": [usage_service.serialize_key(row) for row in rows],
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
        usage = GatewayUsageService(db).build_token_usage_summary(service)
        return {
            "tokens": [
                {
                    **GatewayUsageService(db).serialize_token(row),
                    **usage.get(row.id, {"usage_success": 0, "usage_failed": 0, "usage_this_month": 0}),
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
        payload_out = GatewayUsageService(db).serialize_token(row)
        payload_out.update({"usage_success": 0, "usage_failed": 0, "usage_this_month": 0})
        return payload_out


@router.put("/tokens/{token_id}")
async def update_token(token_id: str, payload: UpdateTokenRequest, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            row = GatewayTokenService(db).update_token(
                token_id,
                name=payload.name,
                hourly_limit=payload.hourly_limit,
                daily_limit=payload.daily_limit,
                monthly_limit=payload.monthly_limit,
            )
        except ValueError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc
        payload_out = GatewayUsageService(db).serialize_token(row)
        payload_out.update({"usage_success": 0, "usage_failed": 0, "usage_this_month": 0})
        return payload_out


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


@router.get("/services/summary")
async def services_summary(_: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        return GatewayUsageService(db).list_service_summaries()


@router.get("/services/{service}/workspace")
async def service_workspace(service: str, _: Any = Depends(require_admin)) -> dict[str, Any]:
    session_factory = get_session_factory()
    with session_factory() as db:
        usage_service = GatewayUsageService(db)
        workspace = usage_service.build_workspace(service)
        workspace["keys"] = [usage_service.serialize_key(row) for row in workspace["keys"]]
        return workspace
