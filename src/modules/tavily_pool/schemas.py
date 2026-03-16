"""Tavily Pool 数据传输模型。"""

from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel


class CreateAccountRequest(BaseModel):
    email: str
    password: str
    api_key: str | None = None
    source: str = "manual"
    notes: str | None = None


class CreateTokenRequest(BaseModel):
    token: str


class UpdateAccountStatusRequest(BaseModel):
    status: str


class PoolReportRequest(BaseModel):
    token_id: str
    success: bool
    endpoint: str = ""
    latency_ms: int | None = None
    error_message: str | None = None


class ImportAccountsRequest(BaseModel):
    file_type: Literal["json", "csv"]
    merge_mode: Literal["skip", "overwrite", "error"] = "skip"
    content: str


class TavilyImportError(BaseModel):
    row: int
    email: str | None = None
    reason: str


class TavilyImportStats(BaseModel):
    total: int
    created: int
    updated: int
    skipped: int
    failed: int
    api_keys_created: int


class TavilyImportResponse(BaseModel):
    stats: TavilyImportStats
    errors: list[TavilyImportError]


class TavilyAccountRead(BaseModel):
    id: str
    email: str
    status: str
    health_status: str
    fail_count: int
    usage_plan: str | None = None
    usage_account_used: int | None = None
    usage_account_limit: int | None = None
    usage_account_remaining: int | None = None
    usage_synced_at: datetime | None = None
    usage_sync_error: str | None = None
    source: str
    notes: str | None
    created_at: datetime
    updated_at: datetime


class TavilyTokenRead(BaseModel):
    id: str
    account_id: str
    token_masked: str
    is_active: bool
    consecutive_fail_count: int
    last_checked_at: datetime | None = None
    last_success_at: datetime | None = None
    last_response_ms: int | None = None
    last_error: str | None = None
    created_at: datetime
    updated_at: datetime


class TavilyPoolLeaseRead(BaseModel):
    account_id: str
    token_id: str
    token: str
    token_masked: str


class TavilyPoolStatsRead(BaseModel):
    total_requests: int
    success_requests: int
    failed_requests: int
    success_rate: float
    avg_latency_ms: int
