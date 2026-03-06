from __future__ import annotations

import copy
import hashlib
import json
from dataclasses import dataclass, field
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
    item_results: list["ProviderSyncItemResult"] = field(default_factory=list)


@dataclass
class ProviderSyncItemResult:
    domain: str
    status: str
    site_url: str | None = None
    provider_id: str | None = None
    provider_name: str | None = None
    message: str | None = None
    cookie_field: str | None = None
    before_fingerprint: str | None = None
    after_fingerprint: str | None = None


class AllApiHubSyncService:
    COOKIE_CREDENTIAL_KEYS = (
        "session_cookie",
        "cookie",
        "cookie_string",
        "auth_cookie",
        "token_cookie",
    )
    TOKEN_CREDENTIAL_KEYS = (
        "api_key",
        "access_token",
        "token",
        "session_token",
        "auth_token",
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
        auto_create_provider_ops: bool = True,
    ) -> ProviderSyncResult:
        raw_text = await self._downloader(url, username, password)
        raw_data = json.loads(raw_text)
        if not isinstance(raw_data, dict):
            raise ValueError("invalid all-api-hub backup format")
        return self.sync_from_backup_object(
            db,
            raw_data,
            dry_run=dry_run,
            auto_create_provider_ops=auto_create_provider_ops,
        )

    def sync_from_backup_object(
        self,
        db: Any,
        backup: dict[str, Any],
        dry_run: bool = False,
        auto_create_provider_ops: bool = True,
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
        matched_account_domains: set[str] = set()
        for provider in providers:
            provider_domain = self._normalize_domain(getattr(provider, "website", None))
            if not provider_domain:
                continue

            account = account_by_domain.get(provider_domain)
            if not account:
                continue
            result.matched_providers += 1
            matched_account_domains.add(provider_domain)

            base_item = ProviderSyncItemResult(
                domain=provider_domain,
                site_url=account.site_url,
                provider_id=str(getattr(provider, "id", "") or ""),
                provider_name=str(getattr(provider, "name", "") or ""),
                status="",
            )

            provider_config = provider.config if isinstance(provider.config, dict) else {}
            provider_ops = provider_config.get("provider_ops")
            if not isinstance(provider_ops, dict):
                if not auto_create_provider_ops:
                    result.skipped_no_provider_ops += 1
                    base_item.status = "no_provider_ops"
                    base_item.message = "provider_ops 未配置"
                    result.item_results.append(base_item)
                    continue
                auto_provider_ops = self._build_auto_provider_ops(
                    account_auth_type=account.auth_type,
                    account_user_id=account.user_id,
                    provider_site_url=getattr(provider, "website", None),
                    account_site_url=account.site_url,
                )
                if auto_provider_ops is None:
                    result.skipped_no_provider_ops += 1
                    base_item.status = "no_provider_ops"
                    base_item.message = "provider_ops 未配置"
                    result.item_results.append(base_item)
                    continue
                provider_config = copy.deepcopy(provider_config)
                provider_config["provider_ops"] = auto_provider_ops
                provider_ops = auto_provider_ops

            connector = provider_ops.get("connector")
            if not isinstance(connector, dict):
                result.skipped_no_provider_ops += 1
                base_item.status = "no_provider_ops"
                base_item.message = "provider_ops.connector 未配置"
                result.item_results.append(base_item)
                continue

            credentials = connector.get("credentials")
            if not isinstance(credentials, dict):
                result.skipped_no_provider_ops += 1
                base_item.status = "no_provider_ops"
                base_item.message = "provider_ops.connector.credentials 未配置"
                result.item_results.append(base_item)
                continue

            cookie_key = self._choose_cookie_field(
                credentials=credentials,
                architecture_id=str(provider_ops.get("architecture_id") or ""),
                account_auth_type=account.auth_type,
            )
            if not cookie_key:
                result.skipped_no_cookie += 1
                base_item.status = "no_cookie_field"
                base_item.message = "无法确定访问凭据字段"
                result.item_results.append(base_item)
                continue

            architecture_id = str(provider_ops.get("architecture_id") or "").strip().lower()
            if cookie_key in self.TOKEN_CREDENTIAL_KEYS and account.access_token:
                new_secret = account.access_token.strip()
            elif cookie_key in self.COOKIE_CREDENTIAL_KEYS and account.cookie_value:
                new_secret = account.cookie_value.strip()
            else:
                new_secret = account.session_cookie.strip()

            encrypted_new = crypto_service.encrypt(new_secret)
            old_cookie_raw = self._decrypt_if_possible(credentials.get(cookie_key))
            before_fp = self._cookie_fingerprint(old_cookie_raw)
            after_fp = self._cookie_fingerprint(new_secret)
            should_sync_user_id = (
                architecture_id == "new_api"
                and (account.auth_type or "").strip().lower() == "access_token"
            )
            should_sync_new_api_cookie = architecture_id == "new_api" and bool(account.cookie_value)
            old_user_id = str(credentials.get("user_id") or "").strip()
            new_user_id = str(account.user_id or "").strip()
            token_not_changed = credentials.get(cookie_key) == encrypted_new
            user_id_not_changed = (not should_sync_user_id) or old_user_id == new_user_id
            new_cookie_encrypted = (
                crypto_service.encrypt(str(account.cookie_value or "").strip())
                if should_sync_new_api_cookie
                else None
            )
            cookie_not_changed = (not should_sync_new_api_cookie) or (
                credentials.get("cookie") == new_cookie_encrypted
            )

            if token_not_changed and user_id_not_changed and cookie_not_changed:
                result.skipped_not_changed += 1
                base_item.status = "not_changed"
                base_item.cookie_field = cookie_key
                base_item.before_fingerprint = before_fp
                base_item.after_fingerprint = after_fp
                base_item.message = "Cookie 未变化"
                result.item_results.append(base_item)
                continue

            result.updated_providers += 1
            base_item.status = "updated"
            base_item.cookie_field = cookie_key
            base_item.before_fingerprint = before_fp
            base_item.after_fingerprint = after_fp
            base_item.message = "Cookie 已更新" if not dry_run else "Dry-run: Cookie 将更新"
            result.item_results.append(base_item)
            if dry_run:
                continue

            # Copy-on-write to avoid accidental shared references in JSON column objects.
            next_config = copy.deepcopy(provider_config)
            next_config["provider_ops"]["connector"]["credentials"][cookie_key] = encrypted_new
            if should_sync_user_id:
                next_config["provider_ops"]["connector"]["credentials"]["user_id"] = new_user_id
            if should_sync_new_api_cookie and new_cookie_encrypted:
                next_config["provider_ops"]["connector"]["credentials"]["cookie"] = (
                    new_cookie_encrypted
                )
            provider.config = next_config
            try:
                flag_modified(provider, "config")
            except Exception:
                pass
            changed += 1

        if changed > 0 and not dry_run:
            db.commit()

        for account in accounts:
            if account.domain in matched_account_domains:
                continue
            result.item_results.append(
                ProviderSyncItemResult(
                    domain=account.domain,
                    site_url=account.site_url,
                    status="unmatched_provider",
                    message="未找到域名匹配的 Provider",
                )
            )

        return result

    @classmethod
    def _choose_cookie_field(
        cls,
        *,
        credentials: dict[str, Any],
        architecture_id: str,
        account_auth_type: str = "cookie",
    ) -> str | None:
        normalized_auth_type = (account_auth_type or "").strip().lower()
        if normalized_auth_type == "access_token":
            for key in cls.TOKEN_CREDENTIAL_KEYS:
                if key in credentials:
                    return key

        for key in cls.COOKIE_CREDENTIAL_KEYS:
            if key in credentials:
                return key

        normalized_arch = architecture_id.strip().lower()
        if normalized_arch == "anyrouter":
            return "session_cookie"
        return "cookie"

    @staticmethod
    def _build_auto_provider_ops(
        *,
        account_auth_type: str,
        account_user_id: str | None,
        provider_site_url: str | None,
        account_site_url: str | None,
    ) -> dict[str, Any] | None:
        normalized_auth_type = (account_auth_type or "").strip().lower()
        if normalized_auth_type != "access_token":
            return None

        base_url = (provider_site_url or account_site_url or "").strip()
        return {
            "architecture_id": "new_api",
            "base_url": base_url or None,
            "connector": {
                "auth_type": "api_key",
                "config": {},
                "credentials": {
                    "api_key": "",
                    "user_id": (account_user_id or "").strip(),
                },
            },
            "actions": {},
            "schedule": {},
        }

    @staticmethod
    def _normalize_domain(url: str | None) -> str:
        value = (url or "").strip()
        if not value:
            return ""
        if "://" not in value:
            value = f"https://{value}"
        host = (urlparse(value).hostname or "").lower()
        return host.strip(".")

    @staticmethod
    def _decrypt_if_possible(value: Any) -> str:
        if not isinstance(value, str) or not value:
            return ""
        try:
            return crypto_service.decrypt(value, silent=True)
        except Exception:
            return value

    @staticmethod
    def _cookie_fingerprint(value: str | None) -> str | None:
        cookie = (value or "").strip()
        if not cookie:
            return None
        digest = hashlib.sha256(cookie.encode("utf-8")).hexdigest()
        return digest[:12]
