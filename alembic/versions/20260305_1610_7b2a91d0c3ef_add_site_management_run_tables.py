"""add_site_management_run_tables

Add site sync/checkin run and item tables for site management observability.

Revision ID: 7b2a91d0c3ef
Revises: 5f1d2e3c4b5a
Create Date: 2026-03-05 16:10:00.000000+00:00
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "7b2a91d0c3ef"
down_revision = "5f1d2e3c4b5a"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "site_sync_runs",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("trigger_source", sa.String(length=20), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("error_message", sa.Text(), nullable=True),
        sa.Column("dry_run", sa.Boolean(), nullable=False, server_default=sa.text("false")),
        sa.Column("total_accounts", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("total_providers", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("matched_providers", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("updated_providers", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("skipped_no_provider_ops", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("skipped_no_cookie", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("skipped_not_changed", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_sync_runs_id", "site_sync_runs", ["id"])
    op.create_index("ix_site_sync_runs_created_at", "site_sync_runs", ["created_at"])
    op.create_index("ix_site_sync_runs_started_at", "site_sync_runs", ["started_at"])

    op.create_table(
        "site_sync_items",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("run_id", sa.String(length=36), nullable=False),
        sa.Column("domain", sa.String(length=255), nullable=False),
        sa.Column("site_url", sa.String(length=500), nullable=True),
        sa.Column("provider_id", sa.String(length=36), nullable=True),
        sa.Column("provider_name", sa.String(length=100), nullable=True),
        sa.Column("status", sa.String(length=30), nullable=False),
        sa.Column("message", sa.Text(), nullable=True),
        sa.Column("cookie_field", sa.String(length=50), nullable=True),
        sa.Column("before_fingerprint", sa.String(length=20), nullable=True),
        sa.Column("after_fingerprint", sa.String(length=20), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["run_id"], ["site_sync_runs.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_sync_items_id", "site_sync_items", ["id"])
    op.create_index("ix_site_sync_items_run_id", "site_sync_items", ["run_id"])
    op.create_index("idx_site_sync_items_run_status", "site_sync_items", ["run_id", "status"])
    op.create_index("idx_site_sync_items_domain", "site_sync_items", ["domain"])

    op.create_table(
        "site_checkin_runs",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("trigger_source", sa.String(length=20), nullable=False),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("error_message", sa.Text(), nullable=True),
        sa.Column("total_providers", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("success_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("failed_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("skipped_count", sa.Integer(), nullable=False, server_default="0"),
        sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_checkin_runs_id", "site_checkin_runs", ["id"])
    op.create_index("ix_site_checkin_runs_created_at", "site_checkin_runs", ["created_at"])
    op.create_index("ix_site_checkin_runs_started_at", "site_checkin_runs", ["started_at"])

    op.create_table(
        "site_checkin_items",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("run_id", sa.String(length=36), nullable=False),
        sa.Column("provider_id", sa.String(length=36), nullable=True),
        sa.Column("provider_name", sa.String(length=100), nullable=True),
        sa.Column("provider_domain", sa.String(length=255), nullable=True),
        sa.Column("status", sa.String(length=20), nullable=False),
        sa.Column("message", sa.Text(), nullable=True),
        sa.Column("balance_total", sa.Float(), nullable=True),
        sa.Column("balance_currency", sa.String(length=10), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.ForeignKeyConstraint(["run_id"], ["site_checkin_runs.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index("ix_site_checkin_items_id", "site_checkin_items", ["id"])
    op.create_index("ix_site_checkin_items_run_id", "site_checkin_items", ["run_id"])
    op.create_index(
        "idx_site_checkin_items_run_status", "site_checkin_items", ["run_id", "status"]
    )
    op.create_index("idx_site_checkin_items_provider", "site_checkin_items", ["provider_id"])


def downgrade() -> None:
    op.drop_index("idx_site_checkin_items_provider", table_name="site_checkin_items")
    op.drop_index("idx_site_checkin_items_run_status", table_name="site_checkin_items")
    op.drop_index("ix_site_checkin_items_run_id", table_name="site_checkin_items")
    op.drop_index("ix_site_checkin_items_id", table_name="site_checkin_items")
    op.drop_table("site_checkin_items")

    op.drop_index("ix_site_checkin_runs_started_at", table_name="site_checkin_runs")
    op.drop_index("ix_site_checkin_runs_created_at", table_name="site_checkin_runs")
    op.drop_index("ix_site_checkin_runs_id", table_name="site_checkin_runs")
    op.drop_table("site_checkin_runs")

    op.drop_index("idx_site_sync_items_domain", table_name="site_sync_items")
    op.drop_index("idx_site_sync_items_run_status", table_name="site_sync_items")
    op.drop_index("ix_site_sync_items_run_id", table_name="site_sync_items")
    op.drop_index("ix_site_sync_items_id", table_name="site_sync_items")
    op.drop_table("site_sync_items")

    op.drop_index("ix_site_sync_runs_started_at", table_name="site_sync_runs")
    op.drop_index("ix_site_sync_runs_created_at", table_name="site_sync_runs")
    op.drop_index("ix_site_sync_runs_id", table_name="site_sync_runs")
    op.drop_table("site_sync_runs")
