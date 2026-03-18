"""Business service for gateway keys."""

from __future__ import annotations

from sqlalchemy.orm import Session

from src.modules.search_pool_gateway.repositories.key_repo import GatewayKeyRepository
from src.modules.search_pool_gateway.services.crypto import mask_key

SUPPORTED_SERVICES = {"tavily", "firecrawl"}


class GatewayKeyService:
    def __init__(self, db: Session) -> None:
        self.repo = GatewayKeyRepository(db)

    def create_key(self, *, service: str, raw_key: str, email: str = ""):
        service_norm = service.strip().lower()
        if service_norm not in SUPPORTED_SERVICES:
            raise ValueError("unsupported service")
        normalized_key = raw_key.strip()
        if not normalized_key:
            raise ValueError("key is required")
        return self.repo.create(
            service=service_norm,
            raw_key=normalized_key,
            key_masked=mask_key(normalized_key),
            email=email.strip(),
        )

    def list_keys(self, service: str | None = None):
        return self.repo.list_keys(service.strip().lower() if service else None)

    def import_keys(self, *, service: str, content: str = "", keys: list[str] | None = None):
        service_norm = service.strip().lower()
        if service_norm not in SUPPORTED_SERVICES:
            raise ValueError("unsupported service")

        parsed_items: list[tuple[str, str]] = []
        raw_lines = list(keys or [])
        if content.strip():
            raw_lines.extend(content.splitlines())

        for line in raw_lines:
            normalized = line.strip()
            if not normalized:
                continue
            parts = [part.strip() for part in normalized.split(",") if part.strip()]
            if not parts:
                continue
            if len(parts) == 1:
                email = ""
                raw_key = parts[0]
            elif len(parts) == 2:
                email, raw_key = parts
            else:
                email = parts[0]
                raw_key = parts[-1]
            if raw_key:
                parsed_items.append((email, raw_key))

        created = [self.create_key(service=service_norm, raw_key=raw_key, email=email) for email, raw_key in parsed_items]
        return created

    def set_active(self, key_id: str, active: bool):
        row = self.repo.set_active(key_id, active)
        if row is None:
            raise ValueError("key not found")
        return row

    def delete(self, key_id: str) -> None:
        if not self.repo.delete(key_id):
            raise ValueError("key not found")
