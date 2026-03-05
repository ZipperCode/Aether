import base64
from unittest.mock import AsyncMock, patch

import pytest

from src.services.provider_sync.webdav_client import download_backup


class _Resp:
    def __init__(self, status_code: int, text: str = "") -> None:
        self.status_code = status_code
        self.text = text


@pytest.mark.asyncio
async def test_download_backup_returns_json_text() -> None:
    response = _Resp(200, '{"version":"2.0"}')

    with patch("httpx.AsyncClient.get", new=AsyncMock(return_value=response)) as mocked_get:
        body = await download_backup(
            url="https://dav.example.com/backup.json",
            username="user",
            password="pass",
        )

    assert body == '{"version":"2.0"}'
    headers = mocked_get.call_args.kwargs["headers"]
    expected = base64.b64encode(b"user:pass").decode()
    assert headers["Authorization"] == f"Basic {expected}"


@pytest.mark.asyncio
async def test_download_backup_raises_on_auth_error() -> None:
    with patch("httpx.AsyncClient.get", new=AsyncMock(return_value=_Resp(401, ""))):
        with pytest.raises(ValueError, match="auth"):
            await download_backup(
                url="https://dav.example.com/backup.json",
                username="user",
                password="pass",
            )


@pytest.mark.asyncio
async def test_download_backup_raises_on_not_found() -> None:
    with patch("httpx.AsyncClient.get", new=AsyncMock(return_value=_Resp(404, ""))):
        with pytest.raises(ValueError, match="not found"):
            await download_backup(
                url="https://dav.example.com/backup.json",
                username="user",
                password="pass",
            )
