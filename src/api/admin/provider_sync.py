from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field
from sqlalchemy.orm import Session

from src.database import get_db
from src.models.database import User
from src.services.provider_sync import AllApiHubSyncService
from src.services.site_management import SiteManagementLogService
from src.services.system.config import SystemConfigService
from src.utils.auth_utils import require_admin

router = APIRouter(prefix="/api/admin/provider-sync", tags=["Provider Sync"])


class SyncTriggerRequest(BaseModel):
    url: str = Field(..., description="WebDAV backup JSON URL")
    username: str = Field(..., description="WebDAV username")
    password: str = Field(..., description="WebDAV password")
    dry_run: bool = Field(False, description="Preview mode, do not persist")
    backup: dict[str, Any] | None = Field(
        None, description="Optional inline backup payload (for testing/manual import)"
    )


@router.post("/trigger")
async def trigger_sync(
    payload: SyncTriggerRequest,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    service = AllApiHubSyncService()
    auto_create_provider_ops = SystemConfigService.get_config(
        db,
        "enable_all_api_hub_auto_create_provider_ops",
        True,
    )
    started_at = datetime.now(timezone.utc)
    run_id: str | None = None
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
                db,
                url=payload.url,
                username=payload.username,
                password=payload.password,
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
        run_id = run.id
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
        "run_id": run_id,
        "total_accounts": result.total_accounts,
        "total_providers": result.total_providers,
        "matched_providers": result.matched_providers,
        "updated_providers": result.updated_providers,
        "skipped_no_provider_ops": result.skipped_no_provider_ops,
        "skipped_no_cookie": result.skipped_no_cookie,
        "skipped_not_changed": result.skipped_not_changed,
        "dry_run": result.dry_run,
    }
