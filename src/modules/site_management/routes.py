"""API routes for the site management module."""
from __future__ import annotations

from dataclasses import asdict, is_dataclass
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy import func
from sqlalchemy.orm import Session

from src.database import get_db
from src.modules.site_management.models import (
    SiteAccount,
    SiteCheckinItem,
    SiteCheckinRun,
    SiteSyncItem,
    SiteSyncRun,
    WebDavSource,
)
from src.modules.site_management.schemas import (
    BatchAccountActionRequest,
    CreateWebDavSourceRequest,
    TriggerSyncRequest,
    UpdateWebDavSourceRequest,
)
from src.modules.site_management.services.account_ops_service import AccountOpsService
from src.modules.site_management.services.account_sync_service import AccountSyncService
from src.modules.site_management.services.log_service import SiteManagementLogService
from src.modules.site_management.services.snapshot_service import SiteSnapshotService
from src.modules.site_management.services.webdav_source_service import WebDavSourceService
from src.utils.auth_utils import require_admin

router = APIRouter()


# ---------------------------------------------------------------------------
# Serialization helpers (mirrored from legacy API)
# ---------------------------------------------------------------------------


def _serialize_data(data: Any) -> Any:
    """Convert dataclass instances to dicts for JSON serialization."""
    if data is None:
        return None
    if is_dataclass(data) and not isinstance(data, type):
        return asdict(data)
    return data


def _serialize_action_result(result: Any) -> dict[str, Any]:
    """Serialize an ``ActionResult`` to a JSON-friendly dict."""
    return {
        "status": result.status.value,
        "action_type": result.action_type.value,
        "data": _serialize_data(result.data),
        "message": result.message,
        "executed_at": result.executed_at.isoformat(),
        "response_time_ms": result.response_time_ms,
        "cache_ttl_seconds": result.cache_ttl_seconds,
    }


def _source_to_dict(source: WebDavSource, account_count: int) -> dict[str, Any]:
    """Serialize a ``WebDavSource`` ORM object to a response dict."""
    return {
        "id": str(source.id),
        "name": source.name,
        "url": source.url,
        "username": source.username,
        "is_active": bool(source.is_active),
        "sync_enabled": bool(source.sync_enabled),
        "last_sync_at": source.last_sync_at.isoformat() if source.last_sync_at else None,
        "last_sync_status": source.last_sync_status,
        "created_at": source.created_at.isoformat() if source.created_at else None,
        "updated_at": source.updated_at.isoformat() if source.updated_at else None,
        "account_count": account_count,
    }


def _account_count_for_source(db: Session, source_id: str) -> int:
    return (
        db.query(func.count(SiteAccount.id))
        .filter(SiteAccount.webdav_source_id == source_id)
        .scalar()
        or 0
    )


# ===================================================================
# Source CRUD
# ===================================================================


@router.get("/sources")
async def list_sources(
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List all WebDav sources."""
    service = WebDavSourceService(db)
    sources = service.list_all()
    return [
        _source_to_dict(s, _account_count_for_source(db, str(s.id)))
        for s in sources
    ]


@router.post("/sources")
async def create_source(
    payload: CreateWebDavSourceRequest,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Create a new WebDav source."""
    service = WebDavSourceService(db)
    source = service.create(
        name=payload.name,
        url=payload.url,
        username=payload.username,
        password=payload.password,
    )
    db.commit()
    return _source_to_dict(source, 0)


@router.put("/sources/{source_id}")
async def update_source(
    source_id: str,
    payload: UpdateWebDavSourceRequest,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Update an existing WebDav source."""
    service = WebDavSourceService(db)
    update_data = payload.model_dump(exclude_unset=True)
    source = service.update(source_id, **update_data)
    if not source:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")
    db.commit()
    return _source_to_dict(source, _account_count_for_source(db, source_id))


@router.delete("/sources/{source_id}")
async def delete_source(
    source_id: str,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Delete a WebDav source (CASCADE deletes related accounts)."""
    service = WebDavSourceService(db)
    deleted = service.delete(source_id)
    if not deleted:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")
    db.commit()
    return {"ok": True}


@router.post("/sources/{source_id}/test")
async def test_source_connection(
    source_id: str,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Test WebDav connection for a source."""
    service = WebDavSourceService(db)
    success, message = await service.test_connection(source_id)
    if not success and message == "Source not found":
        raise HTTPException(status_code=404, detail="WebDav 源不存在")
    return {"success": success, "message": message}


@router.post("/sources/{source_id}/sync")
async def trigger_source_sync(
    source_id: str,
    payload: TriggerSyncRequest,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Manually trigger sync for a specific WebDav source."""
    source_service = WebDavSourceService(db)
    source = source_service.get(source_id)
    if not source:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")

    started_at = datetime.now(timezone.utc)
    try:
        snapshot = await SiteSnapshotService().get_webdav_snapshot(
            db,
            webdav_source_id=source_id,
            force_refresh=payload.force_refresh,
        )
        result = AccountSyncService().apply_snapshot(
            db,
            snapshot=snapshot.payload,
            webdav_source_id=source_id,
        )

        source.last_sync_at = datetime.now(timezone.utc)
        source.last_sync_status = "success"
        db.commit()

        run = SiteManagementLogService.record_sync_run(
            db,
            trigger_source="manual",
            status="success",
            webdav_source_id=source_id,
            total_accounts=result.total_accounts,
            created_accounts=result.created_accounts,
            updated_accounts=result.updated_accounts,
            dry_run=payload.dry_run,
            started_at=started_at,
        )

        return {
            "run_id": str(run.id),
            "total_accounts": result.total_accounts,
            "created_accounts": result.created_accounts,
            "updated_accounts": result.updated_accounts,
            "dry_run": payload.dry_run,
        }
    except ValueError as exc:
        try:
            source.last_sync_status = "failed"
            db.commit()
            SiteManagementLogService.record_sync_run(
                db,
                trigger_source="manual",
                status="failed",
                webdav_source_id=source_id,
                error_message=str(exc),
                started_at=started_at,
            )
        except Exception:
            db.rollback()
        raise HTTPException(status_code=400, detail=str(exc)) from exc


# ===================================================================
# Account operations
# ===================================================================


@router.get("/sources/{source_id}/accounts")
async def list_source_accounts(
    source_id: str,
    page: int = Query(1, ge=1),
    page_size: int = Query(20, ge=1, le=200),
    search: str | None = Query(None),
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List accounts for a WebDav source (paginated, searchable)."""
    source = db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
    if not source:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")

    query = db.query(SiteAccount).filter(SiteAccount.webdav_source_id == source_id)
    if search:
        search_filter = f"%{search}%"
        query = query.filter(
            SiteAccount.domain.ilike(search_filter)
            | SiteAccount.site_url.ilike(search_filter)
        )

    total = query.count()
    accounts = (
        query.order_by(SiteAccount.domain.asc(), SiteAccount.site_url.asc())
        .offset((page - 1) * page_size)
        .limit(page_size)
        .all()
    )

    return {
        "items": [
            {
                "id": str(a.id),
                "webdav_source_id": str(a.webdav_source_id),
                "domain": a.domain,
                "site_url": a.site_url,
                "architecture_id": a.architecture_id,
                "base_url": a.base_url,
                "auth_type": a.auth_type or "cookie",
                "checkin_enabled": bool(a.checkin_enabled),
                "balance_sync_enabled": bool(a.balance_sync_enabled),
                "is_active": bool(a.is_active),
                "last_checkin_status": a.last_checkin_status,
                "last_checkin_message": a.last_checkin_message,
                "last_checkin_at": a.last_checkin_at.isoformat() if a.last_checkin_at else None,
                "last_balance_status": a.last_balance_status,
                "last_balance_message": a.last_balance_message,
                "last_balance_total": a.last_balance_total,
                "last_balance_currency": a.last_balance_currency,
                "last_balance_at": a.last_balance_at.isoformat() if a.last_balance_at else None,
                "created_at": a.created_at.isoformat() if a.created_at else None,
                "updated_at": a.updated_at.isoformat() if a.updated_at else None,
            }
            for a in accounts
        ],
        "total": total,
        "page": page,
        "page_size": page_size,
    }


@router.post("/sources/{source_id}/accounts/{account_id}/checkin")
async def checkin_account(
    source_id: str,
    account_id: str,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Manually trigger checkin for a single account."""
    account = (
        db.query(SiteAccount)
        .filter(SiteAccount.id == account_id, SiteAccount.webdav_source_id == source_id)
        .first()
    )
    if not account:
        raise HTTPException(status_code=404, detail="站点账号不存在")

    result = await AccountOpsService(db).checkin(account_id)
    return _serialize_action_result(result)


@router.post("/sources/{source_id}/accounts/{account_id}/balance")
async def query_account_balance(
    source_id: str,
    account_id: str,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Manually query balance for a single account."""
    account = (
        db.query(SiteAccount)
        .filter(SiteAccount.id == account_id, SiteAccount.webdav_source_id == source_id)
        .first()
    )
    if not account:
        raise HTTPException(status_code=404, detail="站点账号不存在")

    result = await AccountOpsService(db).query_balance(account_id)
    return _serialize_action_result(result)


@router.post("/sources/{source_id}/accounts/checkin")
async def batch_checkin_accounts(
    source_id: str,
    payload: BatchAccountActionRequest,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Batch checkin for accounts under a source."""
    source = db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
    if not source:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")

    query = db.query(SiteAccount.id).filter(
        SiteAccount.webdav_source_id == source_id,
        SiteAccount.is_active.is_(True),
        SiteAccount.checkin_enabled.is_(True),
    )
    if payload.account_ids is not None:
        query = query.filter(SiteAccount.id.in_(payload.account_ids))
    account_ids = [str(aid) for (aid,) in query.all()]

    results: list[dict[str, Any]] = []
    ops_service = AccountOpsService(db)
    for aid in account_ids:
        result = await ops_service.checkin(aid)
        results.append({
            "account_id": aid,
            "status": result.status.value,
            "message": result.message,
        })

    return {
        "total": len(account_ids),
        "results": results,
    }


@router.post("/sources/{source_id}/accounts/balance")
async def batch_balance_accounts(
    source_id: str,
    payload: BatchAccountActionRequest,
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """Batch balance query for accounts under a source."""
    source = db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
    if not source:
        raise HTTPException(status_code=404, detail="WebDav 源不存在")

    query = db.query(SiteAccount.id).filter(
        SiteAccount.webdav_source_id == source_id,
        SiteAccount.is_active.is_(True),
        SiteAccount.balance_sync_enabled.is_(True),
    )
    if payload.account_ids is not None:
        query = query.filter(SiteAccount.id.in_(payload.account_ids))
    account_ids = [str(aid) for (aid,) in query.all()]

    results: list[dict[str, Any]] = []
    ops_service = AccountOpsService(db)
    for aid in account_ids:
        result = await ops_service.query_balance(aid)
        results.append({
            "account_id": aid,
            "status": result.status.value,
            "message": result.message,
        })

    return {
        "total": len(account_ids),
        "results": results,
    }


# ===================================================================
# History -- Sync Runs
# ===================================================================


@router.get("/sync-runs")
async def list_sync_runs(
    page: int = Query(1, ge=1),
    page_size: int = Query(20, ge=1, le=200),
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List sync runs (paginated)."""
    query = db.query(SiteSyncRun).order_by(SiteSyncRun.created_at.desc())
    total = query.count()
    runs = query.offset((page - 1) * page_size).limit(page_size).all()

    return {
        "items": [
            {
                "id": str(run.id),
                "webdav_source_id": run.webdav_source_id,
                "trigger_source": run.trigger_source,
                "status": run.status,
                "error_message": run.error_message,
                "dry_run": bool(run.dry_run),
                "total_accounts": run.total_accounts,
                "started_at": run.started_at.isoformat() if run.started_at else None,
                "finished_at": run.finished_at.isoformat() if run.finished_at else None,
                "created_at": run.created_at.isoformat() if run.created_at else None,
            }
            for run in runs
        ],
        "total": total,
        "page": page,
        "page_size": page_size,
    }


@router.get("/sync-runs/{run_id}/items")
async def get_sync_run_items(
    run_id: str,
    page: int = Query(1, ge=1),
    page_size: int = Query(50, ge=1, le=500),
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List items for a specific sync run."""
    exists = db.query(SiteSyncRun.id).filter(SiteSyncRun.id == run_id).first()
    if not exists:
        raise HTTPException(status_code=404, detail="同步记录不存在")

    query = (
        db.query(SiteSyncItem)
        .filter(SiteSyncItem.run_id == run_id)
        .order_by(SiteSyncItem.created_at.asc())
    )
    total = query.count()
    items = query.offset((page - 1) * page_size).limit(page_size).all()

    return {
        "items": [
            {
                "id": str(item.id),
                "run_id": str(item.run_id),
                "domain": item.domain,
                "site_url": item.site_url,
                "status": item.status,
                "message": item.message,
                "created_at": item.created_at.isoformat() if item.created_at else None,
            }
            for item in items
        ],
        "total": total,
        "page": page,
        "page_size": page_size,
    }


# ===================================================================
# History -- Checkin Runs
# ===================================================================


@router.get("/checkin-runs")
async def list_checkin_runs(
    page: int = Query(1, ge=1),
    page_size: int = Query(20, ge=1, le=200),
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List checkin runs (paginated)."""
    query = db.query(SiteCheckinRun).order_by(SiteCheckinRun.created_at.desc())
    total = query.count()
    runs = query.offset((page - 1) * page_size).limit(page_size).all()

    return {
        "items": [
            {
                "id": str(run.id),
                "trigger_source": run.trigger_source,
                "status": run.status,
                "error_message": run.error_message,
                "total_providers": run.total_providers,
                "success_count": run.success_count,
                "failed_count": run.failed_count,
                "skipped_count": run.skipped_count,
                "started_at": run.started_at.isoformat() if run.started_at else None,
                "finished_at": run.finished_at.isoformat() if run.finished_at else None,
                "created_at": run.created_at.isoformat() if run.created_at else None,
            }
            for run in runs
        ],
        "total": total,
        "page": page,
        "page_size": page_size,
    }


@router.get("/checkin-runs/{run_id}/items")
async def get_checkin_run_items(
    run_id: str,
    page: int = Query(1, ge=1),
    page_size: int = Query(50, ge=1, le=500),
    db: Session = Depends(get_db),
    _: Any = Depends(require_admin),
) -> Any:
    """List items for a specific checkin run."""
    exists = db.query(SiteCheckinRun.id).filter(SiteCheckinRun.id == run_id).first()
    if not exists:
        raise HTTPException(status_code=404, detail="签到记录不存在")

    query = (
        db.query(SiteCheckinItem)
        .filter(SiteCheckinItem.run_id == run_id)
        .order_by(SiteCheckinItem.created_at.asc())
    )
    total = query.count()
    items = query.offset((page - 1) * page_size).limit(page_size).all()

    return {
        "items": [
            {
                "id": str(item.id),
                "run_id": str(item.run_id),
                "provider_id": item.provider_id,
                "provider_name": item.provider_name,
                "provider_domain": item.provider_domain,
                "status": item.status,
                "message": item.message,
                "balance_total": item.balance_total,
                "balance_currency": item.balance_currency,
                "created_at": item.created_at.isoformat() if item.created_at else None,
            }
            for item in items
        ],
        "total": total,
        "page": page,
        "page_size": page_size,
    }
