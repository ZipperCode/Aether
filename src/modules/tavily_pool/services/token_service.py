"""Token 服务。"""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.tavily_pool.models import TavilyToken
from src.modules.tavily_pool.repositories.token_repo import TavilyTokenRepository
from src.modules.tavily_pool.schemas import TavilyTokenRead
from src.modules.tavily_pool.services.crypto import TavilyCryptoService, mask_token


class TavilyTokenService:
    def __init__(self, db: Session) -> None:
        self.db = db
        self.repo = TavilyTokenRepository(db)
        self.crypto = TavilyCryptoService()

    def create_token(self, account_id: str, token: str) -> TavilyTokenRead:
        self.repo.deactivate_account_tokens(account_id)
        entity = TavilyToken(
            account_id=account_id,
            token_encrypted=self.crypto.encrypt(token),
            token_masked=mask_token(token),
            is_active=True,
        )
        created = self.repo.create(entity)
        self.db.commit()
        self.db.refresh(created)
        return TavilyTokenRead.model_validate(created, from_attributes=True)

    def list_tokens(self, account_id: str) -> list[TavilyTokenRead]:
        items = self.repo.list_by_account(account_id)
        return [TavilyTokenRead.model_validate(item, from_attributes=True) for item in items]

    def activate_token(self, token_id: str) -> TavilyTokenRead:
        token = self.repo.get(token_id)
        if token is None:
            raise ValueError("Token not found")
        self.repo.deactivate_account_tokens(token.account_id)
        token.is_active = True
        self.db.commit()
        self.db.refresh(token)
        return TavilyTokenRead.model_validate(token, from_attributes=True)
