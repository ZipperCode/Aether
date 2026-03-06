"""merge heads

Merge multiple heads into a single head.

Revision ID: c2d5e0f7a9b1
Revises: 6a9b8c7d5e4f, 7b2a91d0c3ef
Create Date: 2026-03-06 12:00:00.000000+00:00
"""

from __future__ import annotations

from alembic import op

revision = "c2d5e0f7a9b1"
down_revision = ("6a9b8c7d5e4f", "7b2a91d0c3ef")
branch_labels = None
depends_on = None


def upgrade() -> None:
    pass


def downgrade() -> None:
    pass
