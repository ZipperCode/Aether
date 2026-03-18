"""Business service for gateway keys."""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.repositories.key_repo import GatewayKeyRepository
from src.modules.search_pool_gateway.services.crypto import GatewayCryptoService, mask_key

SUPPORTED_SERVICES = {"tavily", "firecrawl"}


class GatewayKeyService:
    def __init__(self, db: Session) -> None:
        self.repo = GatewayKeyRepository(db)
        self.crypto = GatewayCryptoService()

    def create_key(self, *, service: str, raw_key: str, email: str = ""):
        service_norm = service.strip().lower()
        if service_norm not in SUPPORTED_SERVICES:
            raise ValueError("unsupported service")
        encrypted = self.crypto.encrypt(raw_key.strip())
        return self.repo.create(
            service=service_norm,
            key_encrypted=encrypted,
            key_masked=mask_key(raw_key.strip()),
            email=email.strip(),
        )

    def list_keys(self, service: str | None = None):
        return self.repo.list_keys(service.strip().lower() if service else None)

    def set_active(self, key_id: str, active: bool):
        row = self.repo.set_active(key_id, active)
        if row is None:
            raise ValueError("key not found")
        return row

    def delete(self, key_id: str) -> None:
        if not self.repo.delete(key_id):
            raise ValueError("key not found")
