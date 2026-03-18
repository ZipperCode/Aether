"""Search Pool Gateway SQLite ORM models."""

from __future__ import annotations

from datetime import datetime, timezone
from uuid import uuid4

from sqlalchemy import Boolean, DateTime, Integer, String, Text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class SearchPoolGatewayBase(DeclarativeBase):
    """Search Pool Gateway base model class."""


class GatewayApiKey(SearchPoolGatewayBase):
    __tablename__ = "gateway_api_keys"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    service: Mapped[str] = mapped_column(String(32), nullable=False, index=True)
    raw_key: Mapped[str] = mapped_column(Text, nullable=False)
    key_masked: Mapped[str] = mapped_column(String(128), nullable=False)
    email: Mapped[str] = mapped_column(String(255), nullable=False, default="")
    active: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    total_used: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    total_failed: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    consecutive_fails: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    last_used_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    usage_key_used: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_key_limit: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_key_remaining: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_account_plan: Mapped[str] = mapped_column(String(128), nullable=False, default="")
    usage_account_used: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_account_limit: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_account_remaining: Mapped[int | None] = mapped_column(Integer, nullable=True)
    usage_synced_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    usage_sync_error: Mapped[str] = mapped_column(Text, nullable=False, default="")
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=_utcnow, onupdate=_utcnow, nullable=False
    )


class GatewayToken(SearchPoolGatewayBase):
    __tablename__ = "gateway_tokens"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    service: Mapped[str] = mapped_column(String(32), nullable=False, index=True)
    token: Mapped[str] = mapped_column(String(128), nullable=False, unique=True, index=True)
    name: Mapped[str] = mapped_column(String(128), nullable=False, default="")
    hourly_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    daily_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    monthly_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=_utcnow, onupdate=_utcnow, nullable=False
    )


class GatewayUsageLog(SearchPoolGatewayBase):
    __tablename__ = "gateway_usage_logs"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    service: Mapped[str] = mapped_column(String(32), nullable=False, index=True)
    token_id: Mapped[str | None] = mapped_column(String(36), nullable=True, index=True)
    api_key_id: Mapped[str | None] = mapped_column(String(36), nullable=True, index=True)
    endpoint: Mapped[str] = mapped_column(String(255), nullable=False, default="")
    success: Mapped[bool] = mapped_column(Boolean, nullable=False, default=False)
    latency_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
