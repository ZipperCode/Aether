from __future__ import annotations

from dataclasses import dataclass
from typing import Any
from urllib.parse import urlparse


@dataclass
class ImportedAccount:
    domain: str
    session_cookie: str
    site_url: str
    auth_type: str = "cookie"


def _normalize_domain(site_url: str) -> str:
    text = (site_url or "").strip()
    if not text:
        return ""

    if "://" not in text:
        text = f"https://{text}"

    host = (urlparse(text).hostname or "").lower()
    return host.strip(".")


def _extract_accounts_list(raw: dict[str, Any]) -> list[dict[str, Any]]:
    accounts_node = raw.get("accounts")

    # V2 full backup: {"accounts": {"accounts": [...]}}
    if isinstance(accounts_node, dict):
        nested = accounts_node.get("accounts")
        if isinstance(nested, list):
            return [item for item in nested if isinstance(item, dict)]

    # 容错：部分导出可能直接是数组
    if isinstance(accounts_node, list):
        return [item for item in accounts_node if isinstance(item, dict)]

    # 容错：旧结构可能在 data.accounts
    data_node = raw.get("data")
    if isinstance(data_node, dict):
        legacy = data_node.get("accounts")
        if isinstance(legacy, list):
            return [item for item in legacy if isinstance(item, dict)]

    return []


def parse_all_api_hub_accounts(raw: dict[str, Any]) -> list[ImportedAccount]:
    result: list[ImportedAccount] = []
    for account in _extract_accounts_list(raw):
        site_url = str(account.get("site_url") or "").strip()
        domain = _normalize_domain(site_url)
        if not domain:
            continue

        auth_type = str(account.get("authType") or "cookie").strip().lower()
        cookie_auth = account.get("cookieAuth")
        session_cookie = ""
        if isinstance(cookie_auth, dict):
            session_cookie = str(cookie_auth.get("sessionCookie") or "").strip()

        # 新版 all-api-hub 结构：access_token 保存在 account_info 下
        if not session_cookie:
            account_info = account.get("account_info")
            if isinstance(account_info, dict):
                session_cookie = str(account_info.get("access_token") or "").strip()
            if not session_cookie:
                session_cookie = str(account.get("access_token") or "").strip()

        if not session_cookie:
            continue

        result.append(
            ImportedAccount(
                domain=domain,
                session_cookie=session_cookie,
                site_url=site_url,
                auth_type=auth_type,
            )
        )

    return result
