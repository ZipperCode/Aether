"""Repository for gateway tokens."""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.models import GatewayToken


class GatewayTokenRepository:
    def __init__(self, db: Session) -> None:
        self.db = db

    def create(
        self,
        *,
        service: str,
        token: str,
        name: str = "",
        hourly_limit: int = 0,
        daily_limit: int = 0,
        monthly_limit: int = 0,
    ) -> GatewayToken:
        row = GatewayToken(
            service=service,
            token=token,
            name=name,
            hourly_limit=hourly_limit,
            daily_limit=daily_limit,
            monthly_limit=monthly_limit,
        )
        self.db.add(row)
        self.db.flush()
        self.db.commit()
        self.db.refresh(row)
        return row

    def list_tokens(self, service: str | None = None) -> list[GatewayToken]:
        query = self.db.query(GatewayToken).order_by(GatewayToken.created_at.asc())
        if service:
            query = query.filter(GatewayToken.service == service)
        return query.all()

    def get(self, token_id: str) -> GatewayToken | None:
        return self.db.get(GatewayToken, token_id)

    def get_by_token(self, raw_token: str) -> GatewayToken | None:
        return self.db.query(GatewayToken).filter(GatewayToken.token == raw_token).first()

    def update_limits(
        self,
        token_id: str,
        *,
        name: str,
        hourly_limit: int,
        daily_limit: int,
        monthly_limit: int,
    ) -> GatewayToken | None:
        row = self.get(token_id)
        if row is None:
            return None
        row.name = name
        row.hourly_limit = hourly_limit
        row.daily_limit = daily_limit
        row.monthly_limit = monthly_limit
        self.db.flush()
        self.db.commit()
        self.db.refresh(row)
        return row

    def delete(self, token_id: str) -> bool:
        row = self.get(token_id)
        if row is None:
            return False
        self.db.delete(row)
        self.db.commit()
        return True
