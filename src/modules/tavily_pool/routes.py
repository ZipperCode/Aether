"""Tavily Pool 管理 API。"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from sqlalchemy import desc

from src.modules.tavily_pool.schemas import CreateAccountRequest, CreateTokenRequest
from src.modules.tavily_pool.services.health_service import TavilyHealthService
from src.modules.tavily_pool.services.maintenance_service import TavilyMaintenanceService
from src.modules.tavily_pool.services.account_service import TavilyAccountService
from src.modules.tavily_pool.services.token_service import TavilyTokenService
from src.modules.tavily_pool.models import TavilyHealthCheck, TavilyMaintenanceRun
from src.modules.tavily_pool.sqlite import get_session_factory
from src.utils.auth_utils import require_admin

router = APIRouter(prefix="/api/admin/tavily-pool", tags=["Admin - Tavily Pool"])


@router.get("/accounts")
async def list_accounts(_: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        return TavilyAccountService(db).list_accounts()


@router.post("/accounts")
async def create_account(payload: CreateAccountRequest, _: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        return TavilyAccountService(db).create_account(
            email=payload.email,
            password=payload.password,
            source=payload.source,
            notes=payload.notes,
        )


@router.post("/accounts/{account_id}/tokens")
async def create_account_token(
    account_id: str,
    payload: CreateTokenRequest,
    _: Any = Depends(require_admin),
) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        return TavilyTokenService(db).create_token(account_id=account_id, token=payload.token)


@router.get("/accounts/{account_id}/tokens")
async def list_account_tokens(account_id: str, _: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        return TavilyTokenService(db).list_tokens(account_id)


@router.post("/tokens/{token_id}/activate")
async def activate_token(token_id: str, _: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        try:
            return TavilyTokenService(db).activate_token(token_id)
        except ValueError as exc:
            raise HTTPException(status_code=404, detail=str(exc)) from exc


@router.post("/health-check/run")
async def run_health_check(_: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        return TavilyHealthService(db).run_health_check()


@router.get("/health-check/runs")
async def list_health_checks(limit: int = 50, _: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        items = (
            db.query(TavilyHealthCheck)
            .order_by(desc(TavilyHealthCheck.checked_at))
            .limit(max(1, min(limit, 200)))
            .all()
        )
        return [
            {
                "id": item.id,
                "account_id": item.account_id,
                "token_id": item.token_id,
                "check_type": item.check_type,
                "status": item.status,
                "error_message": item.error_message,
                "checked_at": item.checked_at.isoformat() if item.checked_at else None,
            }
            for item in items
        ]


@router.post("/maintenance/run")
async def run_maintenance(_: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        run = TavilyMaintenanceService(db).run_maintenance(job_name="manual")
        return {
            "run_id": run.id,
            "status": run.status,
            "total": run.total,
            "success": run.success,
            "failed": run.failed,
            "skipped": run.skipped,
        }


@router.get("/maintenance/runs")
async def list_maintenance_runs(limit: int = 50, _: Any = Depends(require_admin)) -> Any:
    session_factory = get_session_factory()
    with session_factory() as db:
        runs = (
            db.query(TavilyMaintenanceRun)
            .order_by(desc(TavilyMaintenanceRun.started_at))
            .limit(max(1, min(limit, 200)))
            .all()
        )
        return [
            {
                "id": run.id,
                "job_name": run.job_name,
                "status": run.status,
                "total": run.total,
                "success": run.success,
                "failed": run.failed,
                "skipped": run.skipped,
                "started_at": run.started_at.isoformat() if run.started_at else None,
                "finished_at": run.finished_at.isoformat() if run.finished_at else None,
            }
            for run in runs
        ]
