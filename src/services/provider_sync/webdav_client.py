from __future__ import annotations

import base64

import httpx


def _build_basic_auth(username: str, password: str) -> str:
    token = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode("utf-8")
    return f"Basic {token}"


async def download_backup(url: str, username: str, password: str) -> str:
    headers = {
        "Authorization": _build_basic_auth(username, password),
        "Accept": "application/json",
    }

    async with httpx.AsyncClient(timeout=20.0) as client:
        response = await client.get(url, headers=headers)

    if response.status_code == 200:
        return response.text
    if response.status_code in (401, 403):
        raise ValueError("webdav auth failed")
    if response.status_code == 404:
        raise ValueError("webdav backup not found")
    raise ValueError(f"webdav request failed: status={response.status_code}")
