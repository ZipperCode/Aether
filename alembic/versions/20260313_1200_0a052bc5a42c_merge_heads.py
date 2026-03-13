"""merge heads

Revision ID: 0a052bc5a42c
Revises: f0c3a7b9d1e2, a9b1c2d3e4f5, 364680d1bc99
Create Date: 2026-03-13 12:00:00.000000+00:00
"""
from __future__ import annotations

from alembic import op

# revision identifiers, used by Alembic.
revision = "0a052bc5a42c"
down_revision = ("f0c3a7b9d1e2", "a9b1c2d3e4f5", "364680d1bc99")
branch_labels = None
depends_on = None


def upgrade() -> None:
    pass


def downgrade() -> None:
    pass
