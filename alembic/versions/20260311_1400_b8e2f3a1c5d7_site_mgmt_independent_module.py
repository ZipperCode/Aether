"""Site management independent module schema.

Adds webdav_sources table, refactors site_accounts to use webdav_source_id
instead of provider_id, and adds webdav_source_id to related tables.

Revision ID: b8e2f3a1c5d7
Revises: e4f8d1c2b3a4, a3f1b7c9d2e4
Create Date: 2026-03-11 14:00:00.000000+00:00
"""

from __future__ import annotations

import uuid

import sqlalchemy as sa
from alembic import op

# revision identifiers, used by Alembic.
revision = "b8e2f3a1c5d7"
down_revision = ("e4f8d1c2b3a4", "a3f1b7c9d2e4")
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 1. Create webdav_sources table
    op.create_table(
        "webdav_sources",
        sa.Column("id", sa.String(length=36), nullable=False),
        sa.Column("name", sa.String(length=100), nullable=False),
        sa.Column("url", sa.String(length=500), nullable=False),
        sa.Column("username", sa.String(length=200), nullable=False),
        sa.Column("password", sa.Text(), nullable=False),
        sa.Column("is_active", sa.Boolean(), nullable=False, server_default=sa.text("true")),
        sa.Column("sync_enabled", sa.Boolean(), nullable=False, server_default=sa.text("true")),
        sa.Column("last_sync_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("last_sync_status", sa.String(length=20), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
    )

    # 2. Add nullable webdav_source_id to site_accounts
    op.add_column("site_accounts", sa.Column("webdav_source_id", sa.String(36), nullable=True))

    # 3. Read existing WebDav config from system_configs, insert default WebDavSource
    default_source_id = str(uuid.uuid4())

    conn = op.get_bind()
    url_row = conn.execute(
        sa.text("SELECT value FROM system_configs WHERE key = 'all_api_hub_webdav_url'")
    ).fetchone()

    if url_row:
        username_row = conn.execute(
            sa.text("SELECT value FROM system_configs WHERE key = 'all_api_hub_webdav_username'")
        ).fetchone()
        password_row = conn.execute(
            sa.text("SELECT value FROM system_configs WHERE key = 'all_api_hub_webdav_password'")
        ).fetchone()

        conn.execute(
            sa.text("""
                INSERT INTO webdav_sources (id, name, url, username, password, is_active, sync_enabled, created_at, updated_at)
                VALUES (:id, :name, :url, :username, :password, true, true, NOW(), NOW())
            """),
            {
                "id": default_source_id,
                "name": "Default WebDav Source",
                "url": url_row[0],
                "username": username_row[0] if username_row else "",
                "password": password_row[0] if password_row else "",
            },
        )

        # 4. Backfill all existing site_accounts.webdav_source_id
        conn.execute(
            sa.text("""
                UPDATE site_accounts SET webdav_source_id = :source_id WHERE webdav_source_id IS NULL
            """),
            {"source_id": default_source_id},
        )

    # 5. Make webdav_source_id NOT NULL
    op.alter_column("site_accounts", "webdav_source_id", nullable=False)

    # 6. Add FK constraint for webdav_source_id
    op.create_foreign_key(
        "fk_site_accounts_webdav_source_id",
        "site_accounts",
        "webdav_sources",
        ["webdav_source_id"],
        ["id"],
        ondelete="CASCADE",
    )

    # 7. Drop provider_id FK, index, and column
    op.drop_constraint("site_accounts_provider_id_fkey", "site_accounts", type_="foreignkey")
    op.drop_index("idx_site_accounts_provider_id", table_name="site_accounts")
    op.drop_column("site_accounts", "provider_id")

    # 8. Drop source_snapshot_id FK and column
    op.drop_constraint("site_accounts_source_snapshot_id_fkey", "site_accounts", type_="foreignkey")
    op.drop_index("ix_site_accounts_source_snapshot_id", table_name="site_accounts")
    op.drop_column("site_accounts", "source_snapshot_id")

    # 9. Drop source_type column
    op.drop_column("site_accounts", "source_type")

    # 10. Add composite unique constraint
    op.create_unique_constraint(
        "uq_site_accounts_source_domain", "site_accounts", ["webdav_source_id", "domain"]
    )

    # 11. Add index for webdav_source_id
    op.create_index("idx_site_accounts_webdav_source_id", "site_accounts", ["webdav_source_id"])

    # 12. Add webdav_source_id to site_source_snapshots
    op.add_column(
        "site_source_snapshots", sa.Column("webdav_source_id", sa.String(36), nullable=True)
    )
    op.create_foreign_key(
        "fk_site_source_snapshots_webdav_source_id",
        "site_source_snapshots",
        "webdav_sources",
        ["webdav_source_id"],
        ["id"],
        ondelete="CASCADE",
    )

    # 12b. Make source_url nullable (no longer required, URL comes from WebDavSource)
    op.alter_column("site_source_snapshots", "source_url", nullable=True)

    # 13. Add webdav_source_id to site_sync_runs
    op.add_column("site_sync_runs", sa.Column("webdav_source_id", sa.String(36), nullable=True))


def downgrade() -> None:
    # 1. Drop webdav_source_id from site_sync_runs
    op.drop_column("site_sync_runs", "webdav_source_id")

    # 2. Drop FK and webdav_source_id from site_source_snapshots
    # 2a. Restore source_url NOT NULL
    op.alter_column("site_source_snapshots", "source_url", nullable=False)
    try:
        op.drop_constraint(
            "fk_site_source_snapshots_webdav_source_id",
            "site_source_snapshots",
            type_="foreignkey",
        )
    except Exception:
        pass
    op.drop_column("site_source_snapshots", "webdav_source_id")

    # 3. Drop unique constraint uq_site_accounts_source_domain
    try:
        op.drop_constraint("uq_site_accounts_source_domain", "site_accounts", type_="unique")
    except Exception:
        pass

    # 4. Drop index idx_site_accounts_webdav_source_id
    op.drop_index("idx_site_accounts_webdav_source_id", table_name="site_accounts")

    # 5. Add back source_type column to site_accounts
    op.add_column(
        "site_accounts",
        sa.Column("source_type", sa.String(length=30), nullable=True),
    )

    # 6. Add back source_snapshot_id column to site_accounts
    op.add_column(
        "site_accounts",
        sa.Column("source_snapshot_id", sa.String(length=36), nullable=True),
    )

    # 7. Add back provider_id column to site_accounts (nullable, no data to restore)
    op.add_column(
        "site_accounts",
        sa.Column("provider_id", sa.String(length=36), nullable=True),
    )

    # 8. Drop FK fk_site_accounts_webdav_source_id
    try:
        op.drop_constraint(
            "fk_site_accounts_webdav_source_id", "site_accounts", type_="foreignkey"
        )
    except Exception:
        pass

    # 9. Drop webdav_source_id column from site_accounts
    op.drop_column("site_accounts", "webdav_source_id")

    # 10. Drop webdav_sources table
    op.drop_table("webdav_sources")
