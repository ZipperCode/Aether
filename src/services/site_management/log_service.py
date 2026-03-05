from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from sqlalchemy.orm import Session

from src.models.database import SiteCheckinItem, SiteCheckinRun, SiteSyncItem, SiteSyncRun
from src.services.provider_sync import ProviderSyncResult


@dataclass
class CheckinItemLog:
    provider_id: str
    provider_name: str | None
    provider_domain: str | None
    status: str
    message: str
    balance_total: float | None = None
    balance_currency: str | None = None


class SiteManagementLogService:
    @staticmethod
    def record_sync_run(
        db: Session,
        *,
        trigger_source: str,
        status: str,
        result: ProviderSyncResult | None = None,
        error_message: str | None = None,
        started_at: datetime | None = None,
        finished_at: datetime | None = None,
    ) -> SiteSyncRun:
        run = SiteSyncRun(
            trigger_source=trigger_source,
            status=status,
            error_message=error_message,
            dry_run=bool(result.dry_run) if result else False,
            total_accounts=result.total_accounts if result else 0,
            total_providers=result.total_providers if result else 0,
            matched_providers=result.matched_providers if result else 0,
            updated_providers=result.updated_providers if result else 0,
            skipped_no_provider_ops=result.skipped_no_provider_ops if result else 0,
            skipped_no_cookie=result.skipped_no_cookie if result else 0,
            skipped_not_changed=result.skipped_not_changed if result else 0,
            started_at=started_at,
            finished_at=finished_at or datetime.now(timezone.utc),
        )
        db.add(run)
        db.flush()

        for item in result.item_results if result else []:
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
        error_message: str | None = None,
        started_at: datetime | None = None,
        finished_at: datetime | None = None,
    ) -> SiteCheckinRun:
        run = SiteCheckinRun(
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
