"""Tavily Pool 模块 SQLite ORM 模型。"""

from __future__ import annotations

from datetime import datetime, timezone
from uuid import uuid4

from sqlalchemy import Boolean, DateTime, ForeignKey, Integer, String, Text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


class TavilyPoolBase(DeclarativeBase):
    """独立于主业务库的 Tavily Pool Base。"""


class TavilyAccount(TavilyPoolBase):
    __tablename__ = "tavily_accounts"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    email: Mapped[str] = mapped_column(String(255), unique=True, nullable=False, index=True)
    password_encrypted: Mapped[str] = mapped_column(Text, nullable=False)
    status: Mapped[str] = mapped_column(String(32), nullable=False, default="active")
    daily_limit: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    daily_used: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    health_status: Mapped[str] = mapped_column(String(32), nullable=False, default="unknown")
    fail_count: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    source: Mapped[str] = mapped_column(String(32), nullable=False, default="manual")
    notes: Mapped[str | None] = mapped_column(Text, nullable=True)
    last_used_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    health_checked_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=_utcnow, onupdate=_utcnow, nullable=False
    )


class TavilyToken(TavilyPoolBase):
    __tablename__ = "tavily_tokens"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    account_id: Mapped[str] = mapped_column(
        String(36), ForeignKey("tavily_accounts.id", ondelete="CASCADE"), nullable=False, index=True
    )
    token_encrypted: Mapped[str] = mapped_column(Text, nullable=False)
    token_masked: Mapped[str] = mapped_column(String(128), nullable=False)
    is_active: Mapped[bool] = mapped_column(Boolean, nullable=False, default=True)
    last_error: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=_utcnow, onupdate=_utcnow, nullable=False
    )


class TavilyHealthCheck(TavilyPoolBase):
    __tablename__ = "tavily_health_checks"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    account_id: Mapped[str | None] = mapped_column(
        String(36), ForeignKey("tavily_accounts.id", ondelete="SET NULL"), nullable=True, index=True
    )
    token_id: Mapped[str | None] = mapped_column(
        String(36), ForeignKey("tavily_tokens.id", ondelete="SET NULL"), nullable=True, index=True
    )
    check_type: Mapped[str] = mapped_column(String(32), nullable=False)
    status: Mapped[str] = mapped_column(String(32), nullable=False)
    response_ms: Mapped[int | None] = mapped_column(Integer, nullable=True)
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True)
    details_json: Mapped[str | None] = mapped_column(Text, nullable=True)
    checked_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)


class TavilyMaintenanceRun(TavilyPoolBase):
    __tablename__ = "tavily_maintenance_runs"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    job_name: Mapped[str] = mapped_column(String(64), nullable=False, index=True)
    total: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    success: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    failed: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    skipped: Mapped[int] = mapped_column(Integer, nullable=False, default=0)
    status: Mapped[str] = mapped_column(String(32), nullable=False, default="success")
    error_message: Mapped[str | None] = mapped_column(Text, nullable=True)
    started_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)


class TavilyMaintenanceItem(TavilyPoolBase):
    __tablename__ = "tavily_maintenance_items"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid4()))
    run_id: Mapped[str] = mapped_column(
        String(36),
        ForeignKey("tavily_maintenance_runs.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    account_id: Mapped[str | None] = mapped_column(
        String(36), ForeignKey("tavily_accounts.id", ondelete="SET NULL"), nullable=True, index=True
    )
    status: Mapped[str] = mapped_column(String(32), nullable=False)
    message: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=_utcnow, nullable=False)
