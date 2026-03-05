from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field
from sqlalchemy.orm import Session

from src.database import get_db
from src.models.database import SiteCheckinItem, SiteCheckinRun, SiteSyncItem, SiteSyncRun, User
from src.services.provider_sync import AllApiHubSyncService
from src.services.site_management import SiteManagementLogService
from src.services.system.config import SystemConfigService
from src.services.system.maintenance_scheduler import get_maintenance_scheduler
from src.utils.auth_utils import require_admin

router = APIRouter(prefix="/api/admin/site-management", tags=["Site Management"])


class TriggerSiteSyncRequest(BaseModel):
    dry_run: bool = Field(False, description="Preview mode, do not mutate provider cookies")
    backup: dict[str, Any] | None = Field(
        None,
        description="Optional inline backup payload (for testing/manual preview)",
    )


@router.post("/sync/trigger")
async def trigger_site_sync(
    payload: TriggerSiteSyncRequest,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    url = SystemConfigService.get_config(db, "all_api_hub_webdav_url")
    username = SystemConfigService.get_config(db, "all_api_hub_webdav_username")
    password_raw = SystemConfigService.get_config(db, "all_api_hub_webdav_password")
    auto_create_provider_ops = SystemConfigService.get_config(
        db,
        "enable_all_api_hub_auto_create_provider_ops",
        True,
    )
    try:
        password = SiteManagementLogService.resolve_system_password(password_raw)
    except Exception as exc:
        raise HTTPException(status_code=400, detail="WebDAV 密码解密失败，请在系统设置中重新保存密码") from exc
    if payload.backup is None and (not url or not username or not password):
        raise HTTPException(status_code=400, detail="WebDAV 配置不完整，请先在系统设置中完成配置")

    service = AllApiHubSyncService()
    started_at = datetime.now(timezone.utc)
    try:
        if payload.backup is not None:
            result = service.sync_from_backup_object(
                db,
                payload.backup,
                dry_run=payload.dry_run,
                auto_create_provider_ops=bool(auto_create_provider_ops),
            )
        else:
            result = await service.sync_from_webdav(
                db=db,
                url=str(url),
                username=str(username),
                password=password,
                dry_run=payload.dry_run,
                auto_create_provider_ops=bool(auto_create_provider_ops),
            )
        run = SiteManagementLogService.record_sync_run(
            db=db,
            trigger_source="manual",
            status="success",
            result=result,
            started_at=started_at,
            finished_at=datetime.now(timezone.utc),
        )
    except ValueError as exc:
        try:
            SiteManagementLogService.record_sync_run(
                db=db,
                trigger_source="manual",
                status="failed",
                error_message=str(exc),
                started_at=started_at,
                finished_at=datetime.now(timezone.utc),
            )
        except Exception:
            pass
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    return {
        "run_id": run.id,
        "total_accounts": result.total_accounts,
        "total_providers": result.total_providers,
        "matched_providers": result.matched_providers,
        "updated_providers": result.updated_providers,
        "skipped_no_provider_ops": result.skipped_no_provider_ops,
        "skipped_no_cookie": result.skipped_no_cookie,
        "skipped_not_changed": result.skipped_not_changed,
        "dry_run": result.dry_run,
    }


@router.post("/checkin/trigger")
async def trigger_site_checkin(
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    scheduler = get_maintenance_scheduler()
    await scheduler._perform_provider_checkin(trigger_source="manual", ignore_enabled=True)

    latest_run = db.query(SiteCheckinRun.id).order_by(SiteCheckinRun.created_at.desc()).first()
    return {
        "ok": True,
        "latest_run_id": latest_run[0] if latest_run else None,
    }


@router.get("/sync-runs")
async def list_sync_runs(
    limit: int = Query(20, ge=1, le=200),
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    runs = db.query(SiteSyncRun).order_by(SiteSyncRun.created_at.desc()).limit(limit).all()
    return [
        {
            "id": run.id,
            "trigger_source": run.trigger_source,
            "status": run.status,
            "error_message": run.error_message,
            "dry_run": run.dry_run,
            "total_accounts": run.total_accounts,
            "total_providers": run.total_providers,
            "matched_providers": run.matched_providers,
            "updated_providers": run.updated_providers,
            "skipped_no_provider_ops": run.skipped_no_provider_ops,
            "skipped_no_cookie": run.skipped_no_cookie,
            "skipped_not_changed": run.skipped_not_changed,
            "started_at": run.started_at.isoformat() if run.started_at else None,
            "finished_at": run.finished_at.isoformat() if run.finished_at else None,
            "created_at": run.created_at.isoformat() if run.created_at else None,
        }
        for run in runs
    ]


@router.get("/sync-runs/{run_id}/items")
async def get_sync_run_items(
    run_id: str,
    limit: int = Query(500, ge=1, le=2000),
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    exists = db.query(SiteSyncRun.id).filter(SiteSyncRun.id == run_id).first()
    if not exists:
        raise HTTPException(status_code=404, detail="同步记录不存在")
    items = (
        db.query(SiteSyncItem)
        .filter(SiteSyncItem.run_id == run_id)
        .order_by(SiteSyncItem.created_at.asc())
        .limit(limit)
        .all()
    )
    return [
        {
            "id": item.id,
            "domain": item.domain,
            "site_url": item.site_url,
            "provider_id": item.provider_id,
            "provider_name": item.provider_name,
            "status": item.status,
            "message": item.message,
            "cookie_field": item.cookie_field,
            "before_fingerprint": item.before_fingerprint,
            "after_fingerprint": item.after_fingerprint,
            "created_at": item.created_at.isoformat() if item.created_at else None,
        }
        for item in items
    ]


@router.get("/checkin-runs")
async def list_checkin_runs(
    limit: int = Query(20, ge=1, le=200),
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    runs = db.query(SiteCheckinRun).order_by(SiteCheckinRun.created_at.desc()).limit(limit).all()
    return [
        {
            "id": run.id,
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
    ]


@router.get("/checkin-runs/{run_id}/items")
async def get_checkin_run_items(
    run_id: str,
    limit: int = Query(500, ge=1, le=2000),
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    exists = db.query(SiteCheckinRun.id).filter(SiteCheckinRun.id == run_id).first()
    if not exists:
        raise HTTPException(status_code=404, detail="签到记录不存在")
    items = (
        db.query(SiteCheckinItem)
        .filter(SiteCheckinItem.run_id == run_id)
        .order_by(SiteCheckinItem.created_at.asc())
        .limit(limit)
        .all()
    )
    return [
        {
            "id": item.id,
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
    ]
