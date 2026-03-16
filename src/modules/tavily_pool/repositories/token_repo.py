"""Token 仓储。"""

from __future__ import annotations

from sqlalchemy import select, update
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyToken


class TavilyTokenRepository:
    def __init__(self, db: Session) -> None:
        self.db = db

    def create(self, token: TavilyToken) -> TavilyToken:
        self.db.add(token)
        self.db.flush()
        return token

    def list_by_account(self, account_id: str) -> list[TavilyToken]:
        stmt = select(TavilyToken).where(TavilyToken.account_id == account_id)
        return list(self.db.execute(stmt).scalars().all())

    def get(self, token_id: str) -> TavilyToken | None:
        return self.db.get(TavilyToken, token_id)

    def deactivate_account_tokens(self, account_id: str) -> None:
        stmt = (
            update(TavilyToken)
            .where(TavilyToken.account_id == account_id)
            .values(is_active=False)
        )
        self.db.execute(stmt)
