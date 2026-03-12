"""Tests for site management log service."""
from __future__ import annotations

from unittest.mock import MagicMock

from src.modules.site_management.services.log_service import (
    CheckinItemLog,
    SiteManagementLogService,
)


def test_record_checkin_run_persists_source_and_account_fields() -> None:
    db = MagicMock()

    SiteManagementLogService.record_checkin_run(
        db,
        trigger_source="scheduled",
        status="success",
        webdav_source_id="source-1",
        total_providers=1,
        success_count=1,
        failed_count=0,
        skipped_count=0,
        items=[
            CheckinItemLog(
                provider_id="provider-1",
                provider_name=None,
                provider_domain="demo.example",
                account_id="account-1",
                account_domain="demo.example",
                account_site_url="https://demo.example",
                status="success",
                message="checked in",
            )
        ],
    )

    added_run = db.add.call_args_list[0].args[0]
    added_item = db.add.call_args_list[1].args[0]
    assert added_run.webdav_source_id == "source-1"
    assert added_item.account_id == "account-1"
    assert added_item.account_domain == "demo.example"
    assert added_item.account_site_url == "https://demo.example"
