from __future__ import annotations

import base64
from urllib.parse import urlparse

import httpx


def _build_basic_auth(username: str, password: str) -> str:
    token = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode("utf-8")
    return f"Basic {token}"


def _build_candidate_urls(url: str) -> list[str]:
    raw = (url or "").strip()
    if not raw:
        return [raw]

    candidates: list[str] = [raw]
    parsed = urlparse(raw)
    path = parsed.path or ""
    is_file_path = "." in path.split("/")[-1]
    if not is_file_path:
        base = raw.rstrip("/")
        candidates.append(f"{base}/all-api-hub-backup/all-api-hub-1-0.json")

    deduped: list[str] = []
    for item in candidates:
        if item and item not in deduped:
            deduped.append(item)
    return deduped


async def download_backup(url: str, username: str, password: str) -> str:
    headers = {
        "Authorization": _build_basic_auth(username, password),
        "Accept": "application/json",
    }

    candidate_urls = _build_candidate_urls(url)

    async with httpx.AsyncClient(timeout=20.0) as client:
        for idx, candidate_url in enumerate(candidate_urls):
            response = await client.get(candidate_url, headers=headers)
            is_last = idx == len(candidate_urls) - 1

            if response.status_code == 200:
                return response.text
            if response.status_code == 401:
                raise ValueError("webdav auth failed")
            if response.status_code == 404:
                if is_last:
                    raise ValueError("webdav backup not found")
                continue
            if response.status_code == 403:
                text = (response.text or "").lower()
                is_not_allowed = "operationnotallowed" in text or "not allowed on this location" in text
                if is_not_allowed and not is_last:
                    continue
                if is_not_allowed:
                    raise ValueError("webdav path not allowed, please use the exact backup file URL")
                raise ValueError("webdav forbidden (403), check credentials and account permission")
            if is_last:
                raise ValueError(f"webdav request failed: status={response.status_code}")

    raise ValueError("webdav request failed: no candidate URL succeeded")
