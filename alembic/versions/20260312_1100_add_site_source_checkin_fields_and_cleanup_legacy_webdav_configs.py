"""add site source checkin fields and clean legacy all-api-hub configs

Revision ID: a9b1c2d3e4f5
Revises: c1d2e3f4a5b6
Create Date: 2026-03-12 11:00:00.000000+00:00

"""

from __future__ import annotations

import re

import sqlalchemy as sa

from alembic import op

# revision identifiers, used by Alembic.
revision = "a9b1c2d3e4f5"
down_revision = "c1d2e3f4a5b6"
branch_labels = None
depends_on = None

_system_configs = sa.table(
    "system_configs",
    sa.column("key", sa.String(255)),
    sa.column("value", sa.JSON()),
)

LEGACY_WEB_DAV_KEYS = (
    "enable_all_api_hub_sync",
    "all_api_hub_sync_time",
    "all_api_hub_webdav_url",
    "all_api_hub_webdav_username",
    "all_api_hub_webdav_password",
    "enable_all_api_hub_auto_create_provider_ops",
)


def _get_system_config(conn: sa.Connection, key: str) -> object:
    return conn.execute(
        sa.select(_system_configs.c.value).where(_system_configs.c.key == key)
    ).scalar_one_or_none()


def _parse_enabled(value: object, default: bool = True) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered in {"true", "1", "yes", "on"}:
            return True
        if lowered in {"false", "0", "no", "off"}:
            return False
    return default


def _parse_time(value: object, default: str = "04:00") -> str:
    if not isinstance(value, str):
        return default
    raw = value.strip()
    if not raw or ":" not in raw:
        return default

    parts = raw.split(":", 1)
    try:
        hour = int(parts[0])
        minute = int(parts[1])
    except ValueError:
        return default

    if not (0 <= hour <= 23 and 0 <= minute <= 59):
        return default

    normalized = f"{hour:02d}:{minute:02d}"
    return normalized if re.match(r"^\d{2}:\d{2}$", normalized) else default


def upgrade() -> None:
    conn = op.get_bind()

    op.add_column(
        "webdav_sources",
        sa.Column("checkin_enabled", sa.Boolean(), nullable=False, server_default=sa.true()),
    )
    op.add_column(
        "webdav_sources",
        sa.Column("checkin_time", sa.String(length=5), nullable=False, server_default="04:00"),
    )
    op.create_index(
        "idx_webdav_sources_checkin_enabled",
        "webdav_sources",
        ["is_active", "checkin_enabled"],
    )

    enabled = _parse_enabled(_get_system_config(conn, "enable_site_account_checkin"), default=True)
    checkin_time = _parse_time(_get_system_config(conn, "site_account_checkin_time"), default="04:00")
    conn.execute(
        sa.text(
            """
            UPDATE webdav_sources
            SET checkin_enabled = :enabled,
                checkin_time = :checkin_time
            """
        ),
        {"enabled": enabled, "checkin_time": checkin_time},
    )

    op.add_column("site_checkin_runs", sa.Column("webdav_source_id", sa.String(length=36), nullable=True))
    op.create_index(
        "idx_site_checkin_runs_source_created",
        "site_checkin_runs",
        ["webdav_source_id", "created_at"],
    )

    op.add_column("site_checkin_items", sa.Column("account_id", sa.String(length=36), nullable=True))
    op.add_column("site_checkin_items", sa.Column("account_domain", sa.String(length=255), nullable=True))
    op.add_column("site_checkin_items", sa.Column("account_site_url", sa.String(length=500), nullable=True))
    op.create_index(
        "idx_site_checkin_items_run_account",
        "site_checkin_items",
        ["run_id", "account_id"],
    )

    conn.execute(
        sa.delete(_system_configs).where(_system_configs.c.key.in_(LEGACY_WEB_DAV_KEYS))
    )


def downgrade() -> None:
    op.drop_index("idx_site_checkin_items_run_account", table_name="site_checkin_items")
    op.drop_column("site_checkin_items", "account_site_url")
    op.drop_column("site_checkin_items", "account_domain")
    op.drop_column("site_checkin_items", "account_id")

    op.drop_index("idx_site_checkin_runs_source_created", table_name="site_checkin_runs")
    op.drop_column("site_checkin_runs", "webdav_source_id")

    op.drop_index("idx_webdav_sources_checkin_enabled", table_name="webdav_sources")
    op.drop_column("webdav_sources", "checkin_time")
    op.drop_column("webdav_sources", "checkin_enabled")

    # NOTE: legacy all-api-hub keys are deleted in upgrade and are not restored in downgrade.
