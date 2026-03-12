"""Pydantic schemas for site management module."""
from __future__ import annotations

from datetime import datetime
from typing import Generic, TypeVar

from pydantic import BaseModel, ConfigDict, Field

T = TypeVar("T")


# ---------------------------------------------------------------------------
# Requests
# ---------------------------------------------------------------------------


class CreateWebDavSourceRequest(BaseModel):
    name: str = Field(..., max_length=100)
    url: str = Field(..., max_length=500)
    username: str = Field(..., max_length=200)
    password: str
    checkin_enabled: bool = True
    checkin_time: str = "04:00"


class UpdateWebDavSourceRequest(BaseModel):
    name: str | None = Field(None, max_length=100)
    url: str | None = Field(None, max_length=500)
    username: str | None = Field(None, max_length=200)
    password: str | None = None
    is_active: bool | None = None
    sync_enabled: bool | None = None
    checkin_enabled: bool | None = None
    checkin_time: str | None = None


class TriggerSyncRequest(BaseModel):
    dry_run: bool = False
    force_refresh: bool = False


class BatchAccountActionRequest(BaseModel):
    account_ids: list[str] | None = Field(
        None, description="None means all eligible accounts"
    )


# ---------------------------------------------------------------------------
# Responses
# ---------------------------------------------------------------------------


class WebDavSourceResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    name: str
    url: str
    username: str
    is_active: bool
    sync_enabled: bool
    checkin_enabled: bool
    checkin_time: str
    last_sync_at: datetime | None
    last_sync_status: str | None
    created_at: datetime
    updated_at: datetime
    account_count: int = 0


class SiteAccountResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    webdav_source_id: str
    domain: str
    site_url: str | None
    architecture_id: str | None
    base_url: str | None
    auth_type: str
    checkin_enabled: bool
    balance_sync_enabled: bool
    is_active: bool
    last_checkin_status: str | None
    last_checkin_message: str | None
    last_checkin_at: datetime | None
    last_balance_status: str | None
    last_balance_message: str | None
    last_balance_total: float | None
    last_balance_currency: str | None
    last_balance_at: datetime | None
    created_at: datetime
    updated_at: datetime


class SyncRunResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    webdav_source_id: str | None
    trigger_source: str
    status: str
    error_message: str | None
    dry_run: bool
    total_accounts: int
    started_at: datetime | None
    finished_at: datetime | None
    created_at: datetime


class SyncItemResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    run_id: str
    domain: str
    site_url: str | None
    status: str
    message: str | None
    created_at: datetime


class CheckinRunResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    trigger_source: str
    status: str
    error_message: str | None
    total_providers: int
    success_count: int
    failed_count: int
    skipped_count: int
    started_at: datetime | None
    finished_at: datetime | None
    created_at: datetime


class CheckinItemResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    id: str
    run_id: str
    provider_id: str | None
    provider_name: str | None
    provider_domain: str | None
    status: str
    message: str | None
    balance_total: float | None
    balance_currency: str | None
    created_at: datetime


class PaginatedResponse(BaseModel, Generic[T]):
    items: list[T]
    total: int
    page: int
    page_size: int
