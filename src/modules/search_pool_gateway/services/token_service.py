"""Business service for gateway tokens."""

from __future__ import annotations

import secrets

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.repositories.token_repo import GatewayTokenRepository
from src.modules.search_pool_gateway.services.key_service import SUPPORTED_SERVICES

TOKEN_PREFIX = {"tavily": "tvly-gw-", "firecrawl": "fctk-gw-"}


class GatewayTokenService:
    def __init__(self, db: Session) -> None:
        self.repo = GatewayTokenRepository(db)

    def create_token(
        self,
        *,
        service: str,
        name: str = "",
        hourly_limit: int = 0,
        daily_limit: int = 0,
        monthly_limit: int = 0,
    ):
        service_norm = service.strip().lower()
        if service_norm not in SUPPORTED_SERVICES:
            raise ValueError("unsupported service")
        token = TOKEN_PREFIX[service_norm] + secrets.token_urlsafe(24)
        return self.repo.create(
            service=service_norm,
            token=token,
            name=name.strip(),
            hourly_limit=max(0, int(hourly_limit)),
            daily_limit=max(0, int(daily_limit)),
            monthly_limit=max(0, int(monthly_limit)),
        )

    def list_tokens(self, service: str | None = None):
        return self.repo.list_tokens(service.strip().lower() if service else None)

    def update_token(
        self,
        token_id: str,
        *,
        name: str,
        hourly_limit: int,
        daily_limit: int,
        monthly_limit: int,
    ):
        row = self.repo.update_limits(
            token_id,
            name=name.strip(),
            hourly_limit=max(0, int(hourly_limit)),
            daily_limit=max(0, int(daily_limit)),
            monthly_limit=max(0, int(monthly_limit)),
        )
        if row is None:
            raise ValueError("token not found")
        return row

    def delete(self, token_id: str) -> None:
        if not self.repo.delete(token_id):
            raise ValueError("token not found")
