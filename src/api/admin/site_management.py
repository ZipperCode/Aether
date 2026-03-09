from __future__ import annotations

import json
from dataclasses import asdict, is_dataclass
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field
from sqlalchemy.orm import Session
from sqlalchemy.orm.attributes import flag_modified

from src.core.crypto import CryptoService
from src.database import get_db
from src.models.database import (
    Provider,
    SiteAccount,
    SiteCheckinItem,
    SiteCheckinRun,
    SiteSyncItem,
    SiteSyncRun,
    User,
)
from src.services.provider_sync import AllApiHubSyncService
from src.services.provider_sync.all_api_hub_backup import parse_all_api_hub_accounts
from src.services.provider_sync.webdav_client import download_backup
from src.services.site_management import (
    SiteAccountOpsService,
    SiteAccountSyncService,
    SiteManagementLogService,
    SiteSnapshotService,
)
from src.services.system.config import SystemConfigService
from src.services.system.maintenance_scheduler import get_maintenance_scheduler
from src.utils.auth_utils import require_admin

router = APIRouter(prefix="/api/admin/site-management", tags=["Site Management"])

_SENSITIVE_CREDENTIAL_FIELDS = {
    "api_key",
    "password",
    "refresh_token",
    "session_token",
    "session_cookie",
    "token_cookie",
    "auth_cookie",
    "cookie_string",
    "cookie",
}


def _checkin_message_requires_manual_verification(message: str | None) -> bool:
    text = str(message or "").strip().lower()
    if not text:
        return False
    indicators = ("turnstile", "captcha", "验证码", "需手动核验", "manual verification")
    return any(ind in text for ind in indicators)


def _serialize_data(data: Any) -> Any:
    if data is None:
        return None
    if is_dataclass(data) and not isinstance(data, type):
        return asdict(data)
    return data


def _serialize_action_result(result: Any) -> dict[str, Any]:
    return {
        "status": result.status.value,
        "action_type": result.action_type.value,
        "data": _serialize_data(result.data),
        "message": result.message,
        "executed_at": result.executed_at.isoformat(),
        "response_time_ms": result.response_time_ms,
        "cache_ttl_seconds": result.cache_ttl_seconds,
    }


def _decrypt_site_account_credentials(credentials: dict[str, Any]) -> dict[str, Any]:
    crypto = CryptoService()
    decrypted: dict[str, Any] = {}
    for key, value in credentials.items():
        if key in _SENSITIVE_CREDENTIAL_FIELDS and isinstance(value, str) and value:
            try:
                decrypted[key] = crypto.decrypt(value)
            except Exception:
                decrypted[key] = value
        else:
            decrypted[key] = value
    return decrypted


class TriggerSiteSyncRequest(BaseModel):
    dry_run: bool = Field(False, description="Preview mode, do not mutate provider cookies")
    backup: dict[str, Any] | None = Field(
        None,
        description="Optional inline backup payload (for testing/manual preview)",
    )


class SiteManagementAccount(BaseModel):
    site_url: str
    domain: str
    provider_id: str | None = None
    provider_name: str | None = None
    checkin_enabled: bool = True
    auth_type: str = "cookie"
    user_id: str | None = None
    access_token: str | None = None
    cookie: str | None = None


class ApplySiteAccountsSyncRequest(BaseModel):
    accounts: list[SiteManagementAccount]
    dry_run: bool = False


class SiteAccountsCheckinStatusRequest(BaseModel):
    provider_ids: list[str] = Field(default_factory=list)


class SyncSiteAccountsRequest(BaseModel):
    force_refresh: bool = Field(False, description="是否强制刷新 WebDAV 快照")
    cache_ttl_seconds: int | None = Field(
        None, ge=0, le=86400, description="快照缓存 TTL（秒），为空时读取系统配置"
    )
    apply_policy: str | None = Field(
        None,
        description="同步策略：matched_only 或 matched_and_unmatched",
    )


def _build_backup_from_accounts(accounts: list[SiteManagementAccount]) -> dict[str, Any]:
    payload_accounts: list[dict[str, Any]] = []
    for account in accounts:
        auth_type = (account.auth_type or "cookie").strip().lower()
        item: dict[str, Any] = {
            "site_url": (account.site_url or "").strip(),
            "authType": auth_type,
        }

        user_id = (account.user_id or "").strip()
        if user_id:
            item["user_id"] = user_id

        cookie_value = (account.cookie or "").strip()
        if cookie_value:
            item["cookieAuth"] = {"cookie": cookie_value}

        access_token = (account.access_token or "").strip()
        account_info: dict[str, Any] = {}
        if access_token:
            account_info["access_token"] = access_token
        if user_id:
            account_info["user_id"] = user_id
        if account_info:
            item["account_info"] = account_info

        payload_accounts.append(item)

    return {
        "version": "2.0",
        "accounts": {
            "accounts": payload_accounts,
        },
    }


async def _load_webdav_backup_payload(db: Session) -> dict[str, Any]:
    url = SystemConfigService.get_config(db, "all_api_hub_webdav_url")
    username = SystemConfigService.get_config(db, "all_api_hub_webdav_username")
    password_raw = SystemConfigService.get_config(db, "all_api_hub_webdav_password")
    if not url or not username or not password_raw:
        raise HTTPException(status_code=400, detail="WebDAV 配置不完整，请先在系统设置中完成配置")
    try:
        password = SiteManagementLogService.resolve_system_password(password_raw)
    except Exception as exc:
        raise HTTPException(status_code=400, detail="WebDAV 密码解密失败，请在系统设置中重新保存密码") from exc

    try:
        raw_text = await download_backup(str(url), str(username), password)
        payload = json.loads(raw_text)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except Exception as exc:
        raise HTTPException(status_code=400, detail=f"读取 WebDAV 备份失败: {exc}") from exc

    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="WebDAV 备份格式错误")
    return payload


def _extract_provider_checkin_enabled(provider_config: dict[str, Any] | None) -> bool:
    if not isinstance(provider_config, dict):
        return True
    provider_ops = provider_config.get("provider_ops")
    if not isinstance(provider_ops, dict):
        return True
    schedule = provider_ops.get("schedule")
    if not isinstance(schedule, dict):
        return True
    value = schedule.get("checkin_enabled")
    if isinstance(value, bool):
        return value
    return True


def _apply_account_checkin_preference(db: Session, accounts: list[SiteManagementAccount]) -> int:
    provider_by_id: dict[str, Any] = {}
    provider_by_domain: dict[str, Any] = {}
    providers = db.query(Provider).all()
    for provider in providers:
        provider_by_id[str(provider.id)] = provider
        domain = AllApiHubSyncService._normalize_domain(getattr(provider, "website", None))
        if domain and domain not in provider_by_domain:
            provider_by_domain[domain] = provider

    changed = 0
    for account in accounts:
        provider_id = str(account.provider_id or "").strip()
        provider = provider_by_id.get(provider_id) if provider_id else None
        if provider is None:
            account_domain = AllApiHubSyncService._normalize_domain(
                str(account.site_url or account.domain or "").strip()
            )
            provider = provider_by_domain.get(account_domain)
        if not provider:
            continue
        config = dict(provider.config or {})
        provider_ops = config.get("provider_ops")
        if not isinstance(provider_ops, dict):
            continue
        schedule = provider_ops.get("schedule")
        if not isinstance(schedule, dict):
            schedule = {}
        expected_enabled = bool(account.checkin_enabled)
        if schedule.get("checkin_enabled") == expected_enabled:
            continue
        schedule["checkin_enabled"] = expected_enabled
        provider_ops["schedule"] = schedule
        config["provider_ops"] = provider_ops
        provider.config = config
        try:
            flag_modified(provider, "config")
        except Exception:
            pass
        changed += 1
    return changed


def _serialize_site_accounts(db: Session, accounts: list[SiteAccount]) -> list[dict[str, Any]]:
    provider_name_by_id = {
        str(pid): str(name or "")
        for pid, name in db.query(Provider.id, Provider.name).all()
    }

    rows: list[dict[str, Any]] = []
    for account in accounts:
        raw_credentials = account.credentials if isinstance(account.credentials, dict) else {}
        credentials = _decrypt_site_account_credentials(raw_credentials)
        rows.append(
            {
                "id": str(account.id),
                "site_url": account.site_url,
                "domain": account.domain,
                "provider_id": str(account.provider_id) if account.provider_id else None,
                "provider_name": provider_name_by_id.get(str(account.provider_id or "")) or None,
                "checkin_enabled": bool(account.checkin_enabled),
                "balance_sync_enabled": bool(account.balance_sync_enabled),
                "is_active": bool(account.is_active),
                "auth_type": account.auth_type or "cookie",
                "architecture_id": account.architecture_id,
                "user_id": credentials.get("user_id"),
                "access_token": credentials.get("api_key"),
                "cookie": credentials.get("cookie") or credentials.get("session_cookie"),
                "last_checkin_status": account.last_checkin_status,
                "last_checkin_message": account.last_checkin_message,
                "last_checkin_at": account.last_checkin_at.isoformat()
                if account.last_checkin_at
                else None,
                "last_balance_status": account.last_balance_status,
                "last_balance_message": account.last_balance_message,
                "last_balance_total": account.last_balance_total,
                "last_balance_currency": account.last_balance_currency,
                "last_balance_at": account.last_balance_at.isoformat()
                if account.last_balance_at
                else None,
                "updated_at": account.updated_at.isoformat() if account.updated_at else None,
            }
        )
    rows.sort(key=lambda x: ((x.get("domain") or ""), (x.get("site_url") or "")))
    return rows


async def _sync_site_accounts_from_webdav(
    db: Session,
    *,
    force_refresh: bool = False,
    cache_ttl_seconds: int | None = None,
    apply_policy: str | None = None,
) -> dict[str, Any]:
    url = SystemConfigService.get_config(db, "all_api_hub_webdav_url")
    username = SystemConfigService.get_config(db, "all_api_hub_webdav_username")
    password_raw = SystemConfigService.get_config(db, "all_api_hub_webdav_password")
    if not url or not username or not password_raw:
        raise HTTPException(status_code=400, detail="WebDAV 配置不完整，请先在系统设置中完成配置")

    try:
        password = SiteManagementLogService.resolve_system_password(password_raw)
    except Exception as exc:
        raise HTTPException(status_code=400, detail="WebDAV 密码解密失败，请在系统设置中重新保存密码") from exc

    ttl = cache_ttl_seconds
    if ttl is None:
        ttl = int(SystemConfigService.get_config(db, "site_account_snapshot_cache_ttl_seconds", 300))
    ttl = max(0, int(ttl))

    policy = (apply_policy or "").strip() or str(
        SystemConfigService.get_config(db, "site_account_sync_apply_policy", "matched_and_unmatched")
    )
    if policy not in {"matched_only", "matched_and_unmatched"}:
        raise HTTPException(status_code=400, detail="无效的同步策略，仅支持 matched_only/matched_and_unmatched")

    snapshot_service = SiteSnapshotService()
    snapshot = await snapshot_service.get_webdav_snapshot(
        db,
        url=str(url),
        username=str(username),
        password=password,
        cache_ttl_seconds=ttl,
        force_refresh=force_refresh,
    )

    sync_result = SiteAccountSyncService().apply_snapshot(
        db,
        snapshot=snapshot.payload,
        apply_policy=policy,
        source_snapshot_id=snapshot.snapshot_id,
    )
    sync_to_provider = bool(
        SystemConfigService.get_config(db, "enable_site_account_sync_to_provider", True)
    )
    provider_sync = None
    if sync_to_provider:
        auto_create_provider_ops = bool(
            SystemConfigService.get_config(
                db,
                "enable_all_api_hub_auto_create_provider_ops",
                True,
            )
        )
        provider_result = AllApiHubSyncService().sync_from_backup_object(
            db,
            snapshot.payload,
            dry_run=False,
            auto_create_provider_ops=auto_create_provider_ops,
        )
        provider_sync = {
            "total_accounts": provider_result.total_accounts,
            "total_providers": provider_result.total_providers,
            "matched_providers": provider_result.matched_providers,
            "updated_providers": provider_result.updated_providers,
            "skipped_no_provider_ops": provider_result.skipped_no_provider_ops,
            "skipped_no_cookie": provider_result.skipped_no_cookie,
            "skipped_not_changed": provider_result.skipped_not_changed,
        }

    return {
        "snapshot_id": snapshot.snapshot_id,
        "source_url": snapshot.source_url,
        "payload_hash": snapshot.payload_hash,
        "from_cache": snapshot.from_cache,
        "fetched_at": snapshot.fetched_at.isoformat(),
        "apply_policy": policy,
        "total_accounts": sync_result.total_accounts,
        "matched_accounts": sync_result.matched_accounts,
        "unmatched_accounts": sync_result.unmatched_accounts,
        "created_accounts": sync_result.created_accounts,
        "updated_accounts": sync_result.updated_accounts,
        "skipped_by_policy": sync_result.skipped_by_policy,
        "sync_to_provider": sync_to_provider,
        "provider_sync": provider_sync,
    }


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


@router.get("/accounts")
async def list_site_accounts(
    refresh: bool = Query(False, description="是否强制刷新并同步 WebDAV 快照"),
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    if refresh:
        await _sync_site_accounts_from_webdav(db, force_refresh=True)

    site_accounts = db.query(SiteAccount).order_by(SiteAccount.updated_at.desc()).all()
    if site_accounts:
        return _serialize_site_accounts(db, site_accounts)

    # 兼容历史行为：当站点账号缓存为空时回退读取 WebDAV 原始列表
    try:
        backup = await _load_webdav_backup_payload(db)
    except HTTPException:
        return []

    accounts = parse_all_api_hub_accounts(backup)
    providers = db.query(Provider.id, Provider.name, Provider.website, Provider.config).all()
    provider_by_domain: dict[str, dict[str, Any]] = {}
    for provider in providers:
        domain = AllApiHubSyncService._normalize_domain(getattr(provider, "website", None))
        if not domain:
            continue
        if domain not in provider_by_domain:
            provider_by_domain[domain] = {
                "provider_id": str(provider.id),
                "provider_name": str(provider.name or ""),
                "checkin_enabled": _extract_provider_checkin_enabled(provider.config),
            }

    normalized = [
        {
            "site_url": account.site_url,
            "domain": account.domain,
            "provider_id": (provider_by_domain.get(account.domain) or {}).get("provider_id"),
            "provider_name": (provider_by_domain.get(account.domain) or {}).get("provider_name"),
            "checkin_enabled": (provider_by_domain.get(account.domain) or {}).get(
                "checkin_enabled", True
            ),
            "auth_type": account.auth_type or "cookie",
            "user_id": account.user_id,
            "access_token": account.access_token,
            "cookie": account.cookie_value,
        }
        for account in accounts
    ]
    normalized.sort(key=lambda x: ((x.get("domain") or ""), (x.get("site_url") or "")))
    return normalized


@router.post("/accounts/sync")
async def sync_site_accounts(
    payload: SyncSiteAccountsRequest,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    return await _sync_site_accounts_from_webdav(
        db,
        force_refresh=payload.force_refresh,
        cache_ttl_seconds=payload.cache_ttl_seconds,
        apply_policy=payload.apply_policy,
    )


@router.post("/accounts/apply-sync")
async def apply_site_accounts_sync(
    payload: ApplySiteAccountsSyncRequest,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    auto_create_provider_ops = SystemConfigService.get_config(
        db,
        "enable_all_api_hub_auto_create_provider_ops",
        True,
    )
    backup = _build_backup_from_accounts(payload.accounts)
    service = AllApiHubSyncService()
    started_at = datetime.now(timezone.utc)
    try:
        result = service.sync_from_backup_object(
            db,
            backup,
            dry_run=payload.dry_run,
            auto_create_provider_ops=bool(auto_create_provider_ops),
        )
        checkin_pref_updated = 0
        if not payload.dry_run:
            checkin_pref_updated = _apply_account_checkin_preference(db, payload.accounts)
            if checkin_pref_updated > 0:
                db.commit()
        run = SiteManagementLogService.record_sync_run(
            db=db,
            trigger_source="manual_edit",
            status="success",
            result=result,
            started_at=started_at,
            finished_at=datetime.now(timezone.utc),
        )
    except ValueError as exc:
        try:
            SiteManagementLogService.record_sync_run(
                db=db,
                trigger_source="manual_edit",
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
        "checkin_pref_updated": checkin_pref_updated if not payload.dry_run else 0,
        "dry_run": result.dry_run,
    }


@router.post("/accounts/checkin-statuses")
async def get_accounts_checkin_statuses(
    payload: SiteAccountsCheckinStatusRequest,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    provider_ids = [str(pid).strip() for pid in payload.provider_ids if str(pid).strip()]
    if not provider_ids:
        return {}

    items = (
        db.query(SiteCheckinItem)
        .filter(SiteCheckinItem.provider_id.in_(provider_ids))
        .order_by(SiteCheckinItem.created_at.desc())
        .limit(5000)
        .all()
    )

    latest_by_provider: dict[str, Any] = {}
    for item in items:
        provider_id = str(item.provider_id or "").strip()
        if not provider_id or provider_id in latest_by_provider:
            continue
        latest_by_provider[provider_id] = {
            "status": item.status,
            "message": item.message,
            "manual_verification_required": _checkin_message_requires_manual_verification(
                item.message
            ),
            "created_at": item.created_at.isoformat() if item.created_at else None,
        }

    return latest_by_provider


@router.post("/accounts/{account_id}/checkin")
async def checkin_site_account(
    account_id: str,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    result = await SiteAccountOpsService(db).checkin(account_id)
    return _serialize_action_result(result)


@router.post("/accounts/{account_id}/balance")
async def balance_site_account(
    account_id: str,
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    result = await SiteAccountOpsService(db).query_balance(account_id)
    return _serialize_action_result(result)


@router.post("/checkin/trigger")
async def trigger_site_checkin(
    db: Session = Depends(get_db),
    _: User = Depends(require_admin),
) -> Any:
    scheduler = get_maintenance_scheduler()
    await scheduler._perform_provider_checkin(trigger_source="manual", ignore_enabled=True)
    await scheduler._perform_site_account_checkin(ignore_enabled=True)

    latest_run = db.query(SiteCheckinRun.id).order_by(SiteCheckinRun.created_at.desc()).first()
    return {
        "ok": True,
        "latest_run_id": latest_run[0] if latest_run else None,
        "site_account_triggered": True,
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
            "manual_verification_required": _checkin_message_requires_manual_verification(
                item.message
            ),
            "balance_total": item.balance_total,
            "balance_currency": item.balance_currency,
            "created_at": item.created_at.isoformat() if item.created_at else None,
        }
        for item in items
    ]
