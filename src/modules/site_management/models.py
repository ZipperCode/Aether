"""站点管理模块 - ORM 模型定义

包含 WebDavSource、SiteAccount、SiteSourceSnapshot、SiteSyncRun、
SiteSyncItem、SiteCheckinRun、SiteCheckinItem 七个模型。
"""

from __future__ import annotations

import uuid
from datetime import datetime, timezone

from sqlalchemy import (
    JSON,
    Boolean,
    Column,
    DateTime,
    Float,
    ForeignKey,
    Index,
    Integer,
    String,
    Text,
    UniqueConstraint,
)
from sqlalchemy.orm import relationship

from src.models.database import Base

# ---------------------------------------------------------------------------
# WebDavSource - WebDAV 数据源
# ---------------------------------------------------------------------------


class WebDavSource(Base):
    """WebDAV 数据源配置"""

    __tablename__ = "webdav_sources"

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    name = Column(String(100), nullable=False)
    url = Column(String(500), nullable=False)
    username = Column(String(200), nullable=False)
    password = Column(Text, nullable=False)  # Encrypted via CryptoService
    is_active = Column(Boolean, nullable=False, default=True)
    sync_enabled = Column(Boolean, nullable=False, default=True)
    last_sync_at = Column(DateTime(timezone=True), nullable=True)
    last_sync_status = Column(String(20), nullable=True)  # success/failed
    created_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
    )
    updated_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
        onupdate=lambda: datetime.now(timezone.utc),
    )

    # Relationships
    accounts = relationship(
        "SiteAccount",
        back_populates="webdav_source",
        passive_deletes=True,
    )
    snapshots = relationship(
        "SiteSourceSnapshot",
        back_populates="webdav_source",
        passive_deletes=True,
    )


# ---------------------------------------------------------------------------
# SiteAccount - 站点账号配置
# ---------------------------------------------------------------------------


class SiteAccount(Base):
    """站点账号配置（独立于 Provider，通过 WebDavSource 关联）"""

    __tablename__ = "site_accounts"
    __table_args__ = (
        UniqueConstraint("webdav_source_id", "domain", name="uq_site_accounts_source_domain"),
        Index("idx_site_accounts_domain", "domain"),
        Index("idx_site_accounts_webdav_source_id", "webdav_source_id"),
        Index("idx_site_accounts_enabled", "is_active", "checkin_enabled", "balance_sync_enabled"),
        {"extend_existing": True},
    )

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    webdav_source_id = Column(
        String(36),
        ForeignKey("webdav_sources.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    site_url = Column(String(500), nullable=True)
    domain = Column(String(255), nullable=False)

    architecture_id = Column(String(100), nullable=True)
    base_url = Column(String(500), nullable=True)
    auth_type = Column(String(30), nullable=False, default="cookie")
    credentials = Column(JSON, nullable=True)
    config = Column(JSON, nullable=True)

    checkin_enabled = Column(Boolean, nullable=False, default=True)
    balance_sync_enabled = Column(Boolean, nullable=False, default=True)
    is_active = Column(Boolean, nullable=False, default=True)

    last_checkin_status = Column(String(20), nullable=True)
    last_checkin_message = Column(Text, nullable=True)
    last_checkin_at = Column(DateTime(timezone=True), nullable=True)
    last_balance_status = Column(String(20), nullable=True)
    last_balance_message = Column(Text, nullable=True)
    last_balance_total = Column(Float, nullable=True)
    last_balance_currency = Column(String(10), nullable=True)
    last_balance_at = Column(DateTime(timezone=True), nullable=True)

    updated_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
        onupdate=lambda: datetime.now(timezone.utc),
    )
    created_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
    )

    # Relationships
    webdav_source = relationship("WebDavSource", back_populates="accounts")


# ---------------------------------------------------------------------------
# SiteSourceSnapshot - 站点源快照
# ---------------------------------------------------------------------------


class SiteSourceSnapshot(Base):
    """站点源快照（WebDAV 拉取缓存）"""

    __tablename__ = "site_source_snapshots"
    __table_args__ = (
        Index("idx_site_source_snapshot_fetched_at", "fetched_at"),
        Index("idx_site_source_snapshot_payload_hash", "payload_hash"),
        Index("idx_site_source_snapshot_webdav_source_id", "webdav_source_id"),
        {"extend_existing": True},
    )

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    webdav_source_id = Column(
        String(36),
        ForeignKey("webdav_sources.id", ondelete="CASCADE"),
        nullable=True,
        index=True,
    )
    source_type = Column(String(30), nullable=False, default="all_api_hub_webdav")
    etag = Column(String(255), nullable=True)
    last_modified = Column(String(255), nullable=True)
    payload_hash = Column(String(64), nullable=False)
    raw_payload = Column(JSON, nullable=False)
    account_count = Column(Integer, nullable=False, default=0)
    fetched_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
        index=True,
    )
    created_at = Column(
        DateTime(timezone=True),
        nullable=False,
        default=lambda: datetime.now(timezone.utc),
    )

    # Relationships
    webdav_source = relationship("WebDavSource", back_populates="snapshots")


# ---------------------------------------------------------------------------
# SiteSyncRun - 站点同步运行记录
# ---------------------------------------------------------------------------


class SiteSyncRun(Base):
    """站点同步运行记录（all-api-hub WebDAV Cookie 同步）"""

    __tablename__ = "site_sync_runs"
    __table_args__ = {"extend_existing": True}

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    webdav_source_id = Column(String(36), nullable=True)  # Loose coupling, not FK
    trigger_source = Column(String(20), nullable=False, default="scheduled")  # scheduled/manual
    status = Column(String(20), nullable=False, default="success")  # success/failed/partial
    error_message = Column(Text, nullable=True)
    dry_run = Column(Boolean, default=False, nullable=False)

    total_accounts = Column(Integer, default=0, nullable=False)
    total_providers = Column(Integer, default=0, nullable=False)
    matched_providers = Column(Integer, default=0, nullable=False)
    updated_providers = Column(Integer, default=0, nullable=False)
    skipped_no_provider_ops = Column(Integer, default=0, nullable=False)
    skipped_no_cookie = Column(Integer, default=0, nullable=False)
    skipped_not_changed = Column(Integer, default=0, nullable=False)

    started_at = Column(DateTime(timezone=True), nullable=True, index=True)
    finished_at = Column(DateTime(timezone=True), nullable=True)
    created_at = Column(
        DateTime(timezone=True),
        default=lambda: datetime.now(timezone.utc),
        nullable=False,
        index=True,
    )

    items = relationship(
        "SiteSyncItem",
        back_populates="run",
        cascade="all, delete-orphan",
        passive_deletes=True,
    )


# ---------------------------------------------------------------------------
# SiteSyncItem - 站点同步明细记录
# ---------------------------------------------------------------------------


class SiteSyncItem(Base):
    """站点同步明细记录（按域名）"""

    __tablename__ = "site_sync_items"
    __table_args__ = (
        Index("idx_site_sync_items_run_status", "run_id", "status"),
        Index("idx_site_sync_items_domain", "domain"),
        {"extend_existing": True},
    )

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    run_id = Column(
        String(36),
        ForeignKey("site_sync_runs.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    domain = Column(String(255), nullable=False)
    site_url = Column(String(500), nullable=True)

    provider_id = Column(String(36), nullable=True)
    provider_name = Column(String(100), nullable=True)

    status = Column(String(30), nullable=False)  # updated/not_changed/no_provider_ops/no_cookie/unmatched
    message = Column(Text, nullable=True)
    cookie_field = Column(String(50), nullable=True)
    before_fingerprint = Column(String(20), nullable=True)
    after_fingerprint = Column(String(20), nullable=True)

    created_at = Column(
        DateTime(timezone=True),
        default=lambda: datetime.now(timezone.utc),
        nullable=False,
    )

    run = relationship("SiteSyncRun", back_populates="items")


# ---------------------------------------------------------------------------
# SiteCheckinRun - 站点签到运行记录
# ---------------------------------------------------------------------------


class SiteCheckinRun(Base):
    """站点签到运行记录（provider_ops balance/checkin）"""

    __tablename__ = "site_checkin_runs"
    __table_args__ = {"extend_existing": True}

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    trigger_source = Column(String(20), nullable=False, default="scheduled")  # scheduled/manual
    status = Column(String(20), nullable=False, default="success")  # success/failed/partial
    error_message = Column(Text, nullable=True)

    total_providers = Column(Integer, default=0, nullable=False)
    success_count = Column(Integer, default=0, nullable=False)
    failed_count = Column(Integer, default=0, nullable=False)
    skipped_count = Column(Integer, default=0, nullable=False)

    started_at = Column(DateTime(timezone=True), nullable=True, index=True)
    finished_at = Column(DateTime(timezone=True), nullable=True)
    created_at = Column(
        DateTime(timezone=True),
        default=lambda: datetime.now(timezone.utc),
        nullable=False,
        index=True,
    )

    items = relationship(
        "SiteCheckinItem",
        back_populates="run",
        cascade="all, delete-orphan",
        passive_deletes=True,
    )


# ---------------------------------------------------------------------------
# SiteCheckinItem - 站点签到明细记录
# ---------------------------------------------------------------------------


class SiteCheckinItem(Base):
    """站点签到明细记录（按 Provider）"""

    __tablename__ = "site_checkin_items"
    __table_args__ = (
        Index("idx_site_checkin_items_run_status", "run_id", "status"),
        Index("idx_site_checkin_items_provider", "provider_id"),
        {"extend_existing": True},
    )

    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()), index=True)
    run_id = Column(
        String(36),
        ForeignKey("site_checkin_runs.id", ondelete="CASCADE"),
        nullable=False,
        index=True,
    )
    provider_id = Column(String(36), nullable=True)
    provider_name = Column(String(100), nullable=True)
    provider_domain = Column(String(255), nullable=True)

    status = Column(String(20), nullable=False)  # success/failed/skipped
    message = Column(Text, nullable=True)
    balance_total = Column(Float, nullable=True)
    balance_currency = Column(String(10), nullable=True)

    created_at = Column(
        DateTime(timezone=True),
        default=lambda: datetime.now(timezone.utc),
        nullable=False,
    )

    run = relationship("SiteCheckinRun", back_populates="items")
