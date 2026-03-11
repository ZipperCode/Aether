"""WebDav source CRUD service."""
from __future__ import annotations

import uuid
from typing import Any

from sqlalchemy.orm import Session

from src.core.crypto import CryptoService
from src.modules.site_management.models import WebDavSource


class WebDavSourceService:
    def __init__(self, db: Session):
        self.db = db
        self.crypto = CryptoService()

    def create(self, *, name: str, url: str, username: str, password: str) -> WebDavSource:
        """Create a new WebDav source with encrypted password."""
        source = WebDavSource(
            id=str(uuid.uuid4()),
            name=name,
            url=url,
            username=username,
            password=self.crypto.encrypt(password),
        )
        self.db.add(source)
        self.db.flush()
        return source

    def update(self, source_id: str, **kwargs: Any) -> WebDavSource | None:
        """Update a WebDav source. Re-encrypts password if changed."""
        source = self.db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
        if not source:
            return None
        for key, value in kwargs.items():
            if key == "password" and value:
                value = self.crypto.encrypt(value)
            if hasattr(source, key):
                setattr(source, key, value)
        self.db.flush()
        return source

    def delete(self, source_id: str) -> bool:
        """Delete a WebDav source. CASCADE handles related records."""
        source = self.db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
        if not source:
            return False
        self.db.delete(source)
        self.db.flush()
        return True

    def get(self, source_id: str) -> WebDavSource | None:
        """Get a single source by ID."""
        return self.db.query(WebDavSource).filter(WebDavSource.id == source_id).first()

    def get_decrypted_password(self, source: WebDavSource) -> str:
        """Decrypt and return the source password."""
        return self.crypto.decrypt(source.password)

    def list_all(self, active_only: bool = False) -> list[WebDavSource]:
        """List all sources, optionally filtered to active only."""
        query = self.db.query(WebDavSource)
        if active_only:
            query = query.filter(WebDavSource.is_active == True)  # noqa: E712
        return query.order_by(WebDavSource.created_at.desc()).all()

    async def test_connection(self, source_id: str) -> tuple[bool, str]:
        """Test WebDav connection for a source. Returns (success, message)."""
        source = self.db.query(WebDavSource).filter(WebDavSource.id == source_id).first()
        if not source:
            return False, "Source not found"
        try:
            from src.modules.site_management.webdav_client import download_backup_with_meta

            password = self.crypto.decrypt(source.password)
            await download_backup_with_meta(source.url, source.username, password)
            return True, "Connection successful"
        except Exception as e:
            return False, str(e)
