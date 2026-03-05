from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from typing import Any, Awaitable, Callable
from urllib.parse import urlparse

from sqlalchemy.orm.attributes import flag_modified

from src.core.crypto import crypto_service
from src.models.database import Provider
from src.services.provider_sync.all_api_hub_backup import parse_all_api_hub_accounts
from src.services.provider_sync.webdav_client import download_backup


DownloadFn = Callable[[str, str, str], Awaitable[str]]


@dataclass
class ProviderSyncResult:
    total_accounts: int = 0
    total_providers: int = 0
    matched_providers: int = 0
    updated_providers: int = 0
    skipped_no_provider_ops: int = 0
    skipped_no_cookie: int = 0
    skipped_not_changed: int = 0
    dry_run: bool = False


class AllApiHubSyncService:
    COOKIE_CREDENTIAL_KEYS = (
        "session_cookie",
        "cookie",
        "cookie_string",
        "auth_cookie",
        "token_cookie",
    )

    def __init__(self, downloader: DownloadFn | None = None) -> None:
        self._downloader = downloader or download_backup

    async def sync_from_webdav(
        self,
        db: Any,
        *,
        url: str,
        username: str,
        password: str,
        dry_run: bool = False,
    ) -> ProviderSyncResult:
        raw_text = await self._downloader(url, username, password)
        raw_data = json.loads(raw_text)
        if not isinstance(raw_data, dict):
            raise ValueError("invalid all-api-hub backup format")
        return self.sync_from_backup_object(db, raw_data, dry_run=dry_run)

    def sync_from_backup_object(
        self,
        db: Any,
        backup: dict[str, Any],
        dry_run: bool = False,
    ) -> ProviderSyncResult:
        accounts = parse_all_api_hub_accounts(backup)
        account_by_domain = {a.domain: a for a in accounts}

        providers = db.query(Provider).all()
        result = ProviderSyncResult(
            total_accounts=len(accounts),
            total_providers=len(providers),
            dry_run=dry_run,
        )

        changed = 0
        for provider in providers:
            provider_domain = self._normalize_domain(getattr(provider, "website", None))
            if not provider_domain:
                continue

            account = account_by_domain.get(provider_domain)
            if not account:
                continue
            result.matched_providers += 1

            provider_config = provider.config if isinstance(provider.config, dict) else {}
            provider_ops = provider_config.get("provider_ops")
            if not isinstance(provider_ops, dict):
                result.skipped_no_provider_ops += 1
                continue

            connector = provider_ops.get("connector")
            if not isinstance(connector, dict):
                result.skipped_no_provider_ops += 1
                continue

            credentials = connector.get("credentials")
            if not isinstance(credentials, dict):
                result.skipped_no_provider_ops += 1
                continue

            cookie_key = self._choose_cookie_field(
                credentials=credentials,
                architecture_id=str(provider_ops.get("architecture_id") or ""),
            )
            if not cookie_key:
                result.skipped_no_cookie += 1
                continue

            new_cookie = account.session_cookie.strip()
            encrypted_new = crypto_service.encrypt(new_cookie)
            if credentials.get(cookie_key) == encrypted_new:
                result.skipped_not_changed += 1
                continue

            result.updated_providers += 1
            if dry_run:
                continue

            # Copy-on-write to avoid accidental shared references in JSON column objects.
            next_config = copy.deepcopy(provider_config)
            next_config["provider_ops"]["connector"]["credentials"][cookie_key] = encrypted_new
            provider.config = next_config
            try:
                flag_modified(provider, "config")
            except Exception:
                pass
            changed += 1

        if changed > 0 and not dry_run:
            db.commit()

        return result

    @classmethod
    def _choose_cookie_field(cls, *, credentials: dict[str, Any], architecture_id: str) -> str | None:
        for key in cls.COOKIE_CREDENTIAL_KEYS:
            if key in credentials:
                return key

        normalized_arch = architecture_id.strip().lower()
        if normalized_arch == "anyrouter":
            return "session_cookie"
        return "cookie"

    @staticmethod
    def _normalize_domain(url: str | None) -> str:
        value = (url or "").strip()
        if not value:
            return ""
        if "://" not in value:
            value = f"https://{value}"
        host = (urlparse(value).hostname or "").lower()
        return host.strip(".")
