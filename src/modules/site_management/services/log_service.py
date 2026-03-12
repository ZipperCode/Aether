"""Site management logging service."""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from sqlalchemy.orm import Session

from src.modules.site_management.models import (
    SiteCheckinItem,
    SiteCheckinRun,
    SiteSyncItem,
    SiteSyncRun,
)


@dataclass
class SyncItemLog:
    domain: str
    site_url: str | None
    status: str
    message: str | None
    provider_id: str | None = None
    provider_name: str | None = None
    cookie_field: str | None = None
    before_fingerprint: str | None = None
    after_fingerprint: str | None = None


@dataclass
class CheckinItemLog:
    status: str
    message: str
    provider_id: str | None = None
    provider_name: str | None = None
    provider_domain: str | None = None
    account_id: str | None = None
    account_domain: str | None = None
    account_site_url: str | None = None
    balance_total: float | None = None
    balance_currency: str | None = None


class SiteManagementLogService:
    @staticmethod
    def record_sync_run(
        db: Session,
        *,
        trigger_source: str,
        status: str,
        webdav_source_id: str | None = None,
        total_accounts: int = 0,
        created_accounts: int = 0,
        updated_accounts: int = 0,
        dry_run: bool = False,
        items: list[SyncItemLog] | None = None,
        error_message: str | None = None,
        started_at: datetime | None = None,
        finished_at: datetime | None = None,
    ) -> SiteSyncRun:
        run = SiteSyncRun(
            trigger_source=trigger_source,
            status=status,
            error_message=error_message,
            dry_run=dry_run,
            webdav_source_id=webdav_source_id,
            total_accounts=total_accounts,
            total_providers=0,  # No longer relevant, kept for schema compat
            matched_providers=0,
            updated_providers=updated_accounts,
            skipped_no_provider_ops=0,
            skipped_no_cookie=0,
            skipped_not_changed=0,
            started_at=started_at,
            finished_at=finished_at or datetime.now(timezone.utc),
        )
        db.add(run)
        db.flush()

        for item in items or []:
            db.add(
                SiteSyncItem(
                    run_id=run.id,
                    domain=item.domain,
                    site_url=item.site_url,
                    provider_id=item.provider_id,
                    provider_name=item.provider_name,
                    status=item.status,
                    message=item.message,
                    cookie_field=item.cookie_field,
                    before_fingerprint=item.before_fingerprint,
                    after_fingerprint=item.after_fingerprint,
                )
            )

        db.commit()
        return run

    @staticmethod
    def record_checkin_run(
        db: Session,
        *,
        trigger_source: str,
        status: str,
        total_providers: int,
        success_count: int,
        failed_count: int,
        skipped_count: int,
        items: list[CheckinItemLog],
        webdav_source_id: str | None = None,
        error_message: str | None = None,
        started_at: datetime | None = None,
        finished_at: datetime | None = None,
    ) -> SiteCheckinRun:
        run = SiteCheckinRun(
            webdav_source_id=webdav_source_id,
            trigger_source=trigger_source,
            status=status,
            error_message=error_message,
            total_providers=total_providers,
            success_count=success_count,
            failed_count=failed_count,
            skipped_count=skipped_count,
            started_at=started_at,
            finished_at=finished_at or datetime.now(timezone.utc),
        )
        db.add(run)
        db.flush()

        for item in items:
            db.add(
                SiteCheckinItem(
                    run_id=run.id,
                    provider_id=item.provider_id,
                    provider_name=item.provider_name,
                    provider_domain=item.provider_domain,
                    account_id=item.account_id,
                    account_domain=item.account_domain,
                    account_site_url=item.account_site_url,
                    status=item.status,
                    message=item.message,
                    balance_total=item.balance_total,
                    balance_currency=item.balance_currency,
                )
            )

        db.commit()
        return run

    @staticmethod
    def resolve_system_password(raw_value: Any) -> str:
        if not isinstance(raw_value, str) or not raw_value:
            return ""
        from src.core.crypto import crypto_service

        return crypto_service.decrypt(raw_value, silent=True)
