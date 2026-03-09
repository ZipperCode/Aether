"""add_site_account_tables

Add site source snapshots and site accounts tables.

Revision ID: e4f8d1c2b3a4
Revises: c2d5e0f7a9b1
Create Date: 2026-03-07 12:00:00.000000+00:00
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "e4f8d1c2b3a4"
down_revision = "c2d5e0f7a9b1"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "site_source_snapshots",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("source_type", sa.String(length=30), nullable=False),
        sa.Column("source_url", sa.String(length=500), nullable=False),
        sa.Column("etag", sa.String(length=255), nullable=True),
        sa.Column("last_modified", sa.String(length=255), nullable=True),
        sa.Column("payload_hash", sa.String(length=64), nullable=False),
        sa.Column("raw_payload", sa.JSON(), nullable=False),
        sa.Column("account_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("fetched_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_source_snapshots_id", "site_source_snapshots", ["id"])
    op.create_index(
        "idx_site_source_snapshot_fetched_at",
        "site_source_snapshots",
        ["fetched_at"],
    )
    op.create_index(
        "idx_site_source_snapshot_payload_hash",
        "site_source_snapshots",
        ["payload_hash"],
    )

    op.create_table(
        "site_accounts",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("source_type", sa.String(length=30), nullable=False),
        sa.Column("source_snapshot_id", sa.String(length=36), nullable=True),
        sa.Column("site_url", sa.String(length=500), nullable=True),
        sa.Column("domain", sa.String(length=255), nullable=False),
        sa.Column("provider_id", sa.String(length=36), nullable=True),
        sa.Column("architecture_id", sa.String(length=100), nullable=True),
        sa.Column("base_url", sa.String(length=500), nullable=True),
        sa.Column("auth_type", sa.String(length=30), nullable=False, server_default="cookie"),
        sa.Column("credentials", sa.JSON(), nullable=True),
        sa.Column("config", sa.JSON(), nullable=True),
        sa.Column("checkin_enabled", sa.Boolean(), nullable=False, server_default=sa.text("true")),
        sa.Column(
            "balance_sync_enabled", sa.Boolean(), nullable=False, server_default=sa.text("true")
        ),
        sa.Column("is_active", sa.Boolean(), nullable=False, server_default=sa.text("true")),
        sa.Column("last_checkin_status", sa.String(length=20), nullable=True),
        sa.Column("last_checkin_message", sa.Text(), nullable=True),
        sa.Column("last_checkin_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("last_balance_status", sa.String(length=20), nullable=True),
        sa.Column("last_balance_message", sa.Text(), nullable=True),
        sa.Column("last_balance_total", sa.Float(), nullable=True),
        sa.Column("last_balance_currency", sa.String(length=10), nullable=True),
        sa.Column("last_balance_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["provider_id"], ["providers.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(
            ["source_snapshot_id"],
            ["site_source_snapshots.id"],
            ondelete="SET NULL",
        ),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_accounts_id", "site_accounts", ["id"])
    op.create_index("idx_site_accounts_domain", "site_accounts", ["domain"])
    op.create_index("idx_site_accounts_provider_id", "site_accounts", ["provider_id"])
    op.create_index("ix_site_accounts_source_snapshot_id", "site_accounts", ["source_snapshot_id"])
    op.create_index(
        "idx_site_accounts_enabled",
        "site_accounts",
        ["is_active", "checkin_enabled", "balance_sync_enabled"],
    )


def downgrade() -> None:
    op.drop_index("idx_site_accounts_enabled", table_name="site_accounts")
    op.drop_index("ix_site_accounts_source_snapshot_id", table_name="site_accounts")
    op.drop_index("idx_site_accounts_provider_id", table_name="site_accounts")
    op.drop_index("idx_site_accounts_domain", table_name="site_accounts")
    op.drop_index("ix_site_accounts_id", table_name="site_accounts")
    op.drop_table("site_accounts")

    op.drop_index("idx_site_source_snapshot_payload_hash", table_name="site_source_snapshots")
    op.drop_index("idx_site_source_snapshot_fetched_at", table_name="site_source_snapshots")
    op.drop_index("ix_site_source_snapshots_id", table_name="site_source_snapshots")
    op.drop_table("site_source_snapshots")
