"""账号服务。"""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyAccount
from src.modules.tavily_pool.repositories.account_repo import TavilyAccountRepository
from src.modules.tavily_pool.schemas import TavilyAccountRead
from src.modules.tavily_pool.services.crypto import TavilyCryptoService


class TavilyAccountService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.repo = TavilyAccountRepository(db)
        self.crypto = TavilyCryptoService()

    def create_account(
        self,
        *,
        email: str,
        password: str,
        source: str = "manual",
        notes: str | None = None,
    ) -> TavilyAccountRead:
        account = TavilyAccount(
            email=email.strip(),
            password_encrypted=self.crypto.encrypt(password),
            source=source,
            notes=notes,
        )
        created = self.repo.create(account)
        self.db.commit()
        self.db.refresh(created)
        return TavilyAccountRead.model_validate(created, from_attributes=True)

    def list_accounts(self) -> list[TavilyAccountRead]:
        return [
            TavilyAccountRead.model_validate(item, from_attributes=True)
            for item in self.repo.list_all()
        ]
