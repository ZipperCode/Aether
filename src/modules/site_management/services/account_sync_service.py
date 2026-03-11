"""Account sync service -- apply a parsed snapshot to SiteAccount records.

Scoped to a single ``webdav_source_id``; no legacy coupling.
"""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from src.core.crypto import CryptoService
from src.modules.site_management.models import SiteAccount
from src.modules.site_management.services.parsers import ImportedAccount, parse_all_api_hub_accounts


@dataclass
class AccountSyncResult:
    total_accounts: int = 0
    created_accounts: int = 0
    updated_accounts: int = 0


class AccountSyncService:
    """Synchronise parsed backup accounts into ``SiteAccount`` rows.

    All accounts are scoped to a single ``webdav_source_id`` so that multiple
    WebDAV sources can coexist without collision.
    """

    SENSITIVE_FIELDS = {
        "api_key",
        "password",
        "refresh_token",
        "session_token",
        "session_cookie",
        "token_cookie",
        "auth_cookie",
        "cookie_string",
        "cookie",
    }

    def __init__(self, crypto: CryptoService | None = None) -> None:
        self.crypto = crypto or CryptoService()

    # ------------------------------------------------------------------
    # public
    # ------------------------------------------------------------------

    def apply_snapshot(
        self,
        db: Any,
        *,
        snapshot: dict[str, Any],
        webdav_source_id: str,
    ) -> AccountSyncResult:
        """Parse *snapshot* and upsert ``SiteAccount`` rows for *webdav_source_id*."""

        accounts = parse_all_api_hub_accounts(snapshot)
        existing_accounts = (
            db.query(SiteAccount)
            .filter(SiteAccount.webdav_source_id == webdav_source_id)
            .all()
        )
        now = datetime.now(timezone.utc)

        existing_by_identity: dict[str, SiteAccount] = {}
        existing_by_domain_auth: dict[tuple[str, str], list[SiteAccount]] = {}
        for account in existing_accounts:
            domain = str(getattr(account, "domain", "") or "").strip().lower()
            auth_type = str(getattr(account, "auth_type", "") or "cookie").strip().lower()
            if domain:
                existing_by_domain_auth.setdefault((domain, auth_type), []).append(account)
            identity = self._build_existing_identity(account)
            if identity and identity not in existing_by_identity:
                existing_by_identity[identity] = account

        result = AccountSyncResult(total_accounts=len(accounts))
        for imported in accounts:
            imported_identity = self._build_imported_identity(imported)
            site_account = existing_by_identity.get(imported_identity)
            imported_user_id = str(imported.user_id or "").strip()
            imported_auth_type = str(imported.auth_type or "cookie").strip().lower()

            if site_account is None and not imported_user_id:
                candidates = existing_by_domain_auth.get((imported.domain, imported_auth_type), [])
                if len(candidates) == 1:
                    site_account = candidates[0]

            created = False
            if site_account is None:
                site_account = SiteAccount(
                    domain=imported.domain,
                    webdav_source_id=webdav_source_id,
                    auth_type=(imported.auth_type or "cookie"),
                    is_active=True,
                    checkin_enabled=True,
                    balance_sync_enabled=True,
                )
                db.add(site_account)
                existing_by_identity[imported_identity] = site_account
                existing_by_domain_auth.setdefault(
                    (imported.domain, imported_auth_type), []
                ).append(site_account)
                created = True
                result.created_accounts += 1

            changed = self._apply_account_fields(
                site_account=site_account,
                imported=imported,
                now=now,
            )
            updated_identity = self._build_existing_identity(site_account)
            if updated_identity and updated_identity not in existing_by_identity:
                existing_by_identity[updated_identity] = site_account
            if changed and not created:
                result.updated_accounts += 1

        db.commit()
        return result

    # ------------------------------------------------------------------
    # field application
    # ------------------------------------------------------------------

    def _apply_account_fields(
        self,
        *,
        site_account: SiteAccount,
        imported: ImportedAccount,
        now: datetime,
    ) -> bool:
        changed = False

        def _set(name: str, value: Any) -> None:
            nonlocal changed
            if getattr(site_account, name, None) != value:
                setattr(site_account, name, value)
                changed = True

        architecture_id: str | None = None
        if (imported.auth_type or "").strip().lower() == "access_token":
            architecture_id = "new_api"

        _set("site_url", imported.site_url or None)
        _set("base_url", imported.site_url or None)
        _set("architecture_id", architecture_id)
        _set("auth_type", (imported.auth_type or "cookie").strip().lower())

        next_credentials = self._build_credentials(imported)
        current_credentials = self._decrypt_credentials(
            site_account.credentials if isinstance(site_account.credentials, dict) else {}
        )
        if not self._credentials_semantically_equal(current_credentials, next_credentials):
            _set("credentials", self._encrypt_credentials(next_credentials))

        if changed:
            _set("updated_at", now)
        return changed

    # ------------------------------------------------------------------
    # credential helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _build_credentials(imported: ImportedAccount) -> dict[str, Any]:
        auth_type = (imported.auth_type or "cookie").strip().lower()
        if auth_type == "access_token":
            credentials: dict[str, Any] = {
                "api_key": str(imported.access_token or imported.session_cookie or "").strip()
            }
            user_id = str(imported.user_id or "").strip()
            if user_id:
                credentials["user_id"] = user_id
            cookie_value = str(imported.cookie_value or "").strip()
            if cookie_value:
                credentials["cookie"] = cookie_value
            return credentials

        cookie = str(imported.cookie_value or imported.session_cookie or "").strip()
        return {
            "cookie": cookie,
            "session_cookie": cookie,
        }

    # ------------------------------------------------------------------
    # identity helpers
    # ------------------------------------------------------------------

    def _build_imported_identity(self, imported: ImportedAccount) -> str:
        domain = str(imported.domain or "").strip().lower()
        auth_type = str(imported.auth_type or "cookie").strip().lower()
        user_id = str(imported.user_id or "").strip().lower()
        if user_id:
            return f"{domain}|{auth_type}|user:{user_id}"
        site = self._normalize_site_url(imported.site_url)
        return f"{domain}|{auth_type}|url:{site}"

    def _build_existing_identity(self, account: SiteAccount) -> str | None:
        domain = str(getattr(account, "domain", "") or "").strip().lower()
        if not domain:
            return None
        auth_type = str(getattr(account, "auth_type", "") or "cookie").strip().lower()
        credentials = self._decrypt_credentials(
            account.credentials if isinstance(account.credentials, dict) else {}
        )
        user_id = str(credentials.get("user_id") or "").strip().lower()
        if user_id:
            return f"{domain}|{auth_type}|user:{user_id}"
        site = self._normalize_site_url(getattr(account, "site_url", None))
        return f"{domain}|{auth_type}|url:{site}"

    @staticmethod
    def _normalize_site_url(site_url: Any) -> str:
        text = str(site_url or "").strip().lower()
        if not text:
            return ""
        return text.rstrip("/")

    # ------------------------------------------------------------------
    # crypto helpers
    # ------------------------------------------------------------------

    def _encrypt_credentials(self, credentials: dict[str, Any]) -> dict[str, Any]:
        encrypted: dict[str, Any] = {}
        for key, value in credentials.items():
            if key in self.SENSITIVE_FIELDS and isinstance(value, str) and value:
                encrypted[key] = self.crypto.encrypt(value)
            else:
                encrypted[key] = value
        return encrypted

    def _decrypt_credentials(self, credentials: dict[str, Any]) -> dict[str, Any]:
        decrypted: dict[str, Any] = {}
        for key, value in credentials.items():
            if key in self.SENSITIVE_FIELDS and isinstance(value, str):
                try:
                    decrypted[key] = self.crypto.decrypt(value)
                except Exception:
                    decrypted[key] = value
            else:
                decrypted[key] = value
        return decrypted

    @staticmethod
    def _credentials_semantically_equal(
        current: dict[str, Any],
        incoming: dict[str, Any],
    ) -> bool:
        def _normalize(payload: dict[str, Any]) -> dict[str, Any]:
            normalized: dict[str, Any] = {}
            for key, value in payload.items():
                if value is None:
                    continue
                if isinstance(value, str):
                    text = value.strip()
                    if not text:
                        continue
                    normalized[key] = text
                else:
                    normalized[key] = value
            return normalized

        return _normalize(current) == _normalize(incoming)
