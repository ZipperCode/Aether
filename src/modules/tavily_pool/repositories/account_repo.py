"""账号仓储。"""

from __future__ import annotations

from sqlalchemy import select
from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount


class TavilyAccountRepository:
    def __init__(self, db: Session) -> None:
        self.db = db

    def create(self, account: TavilyAccount) -> TavilyAccount:
        self.db.add(account)
        self.db.flush()
        return account

    def get(self, account_id: str) -> TavilyAccount | None:
        return self.db.get(TavilyAccount, account_id)

    def list_all(self) -> list[TavilyAccount]:
        return list(self.db.execute(select(TavilyAccount)).scalars().all())
