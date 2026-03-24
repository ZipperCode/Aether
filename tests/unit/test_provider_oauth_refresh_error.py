from __future__ import annotations

import httpx

from src.api.admin import provider_oauth as module


def test_extract_oauth_refresh_error_reason_for_reused_refresh_token() -> None:
    response = httpx.Response(
        400,
        json={
            "error": {
                "message": (
                    "Your refresh token has already been used to generate a new access token. "
                    "Please try signing in again."
                ),
                "type": "invalid_request_error",
                "param": None,
                "code": "refresh_token_reused",
            }
        },
        request=httpx.Request("POST", "https://example.com/oauth/token"),
    )

    assert (
        module._extract_oauth_refresh_error_reason(response)
        == "refresh_token 已被使用并轮换，请重新登录授权"
    )


def test_extract_oauth_refresh_error_reason_prefers_nested_message() -> None:
    response = httpx.Response(
        401,
        json={
            "error": {
                "message": "refresh token expired",
                "type": "invalid_request_error",
            }
        },
        request=httpx.Request("POST", "https://example.com/oauth/token"),
    )

    assert (
        module._extract_oauth_refresh_error_reason(response)
        == "refresh_token 无效、已过期或已撤销，请重新登录授权"
    )


def test_merge_refresh_failure_reason_keeps_account_block_and_appends_refresh_failure() -> None:
    current_reason = "[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)"
    refresh_reason = "[REFRESH_FAILED] Token 续期失败 (400): refresh_token_reused"

    assert module._merge_refresh_failure_reason(current_reason, refresh_reason) == (
        "[ACCOUNT_BLOCK] 工作区已停用 (deactivated_workspace)\n"
        "[REFRESH_FAILED] Token 续期失败 (400): refresh_token_reused"
    )
