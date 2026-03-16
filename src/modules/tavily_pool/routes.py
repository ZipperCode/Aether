"""Tavily Pool 管理 API。"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, Depends, HTTPException

from src.modules.tavily_pool.schemas import CreateAccountRequest, CreateTokenRequest
from src.modules.tavily_pool.services.account_service import TavilyAccountService
from src.modules.tavily_pool.services.token_service import TavilyTokenService
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
