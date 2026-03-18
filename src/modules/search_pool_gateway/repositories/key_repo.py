"""Repository for gateway API keys."""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.models import GatewayApiKey


class GatewayKeyRepository:
    def __init__(self, db: Session) -> None:
        self.db = db

    def create(self, *, service: str, raw_key: str, key_masked: str, email: str = "") -> GatewayApiKey:
        row = GatewayApiKey(
            service=service,
            raw_key=raw_key,
            key_masked=key_masked,
            email=email,
            active=True,
        )
        self.db.add(row)
        self.db.flush()
        self.db.commit()
        self.db.refresh(row)
        return row

    def list_keys(self, service: str | None = None) -> list[GatewayApiKey]:
        query = self.db.query(GatewayApiKey).order_by(GatewayApiKey.created_at.asc())
        if service:
            query = query.filter(GatewayApiKey.service == service)
        return query.all()

    def get(self, key_id: str) -> GatewayApiKey | None:
        return self.db.get(GatewayApiKey, key_id)

    def set_active(self, key_id: str, active: bool) -> GatewayApiKey | None:
        row = self.get(key_id)
        if row is None:
            return None
        row.active = active
        row.consecutive_fails = 0
        self.db.flush()
        self.db.commit()
        self.db.refresh(row)
        return row

    def delete(self, key_id: str) -> bool:
        row = self.get(key_id)
        if row is None:
            return False
        self.db.delete(row)
        self.db.commit()
        return True
