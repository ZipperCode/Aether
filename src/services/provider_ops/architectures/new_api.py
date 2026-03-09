"""
New API 架构

针对 New API 风格的中转站优化的预设配置。
"""

from typing import Any

import httpx

from src.core.api_format.headers import BROWSER_FINGERPRINT_HEADERS
from src.services.provider_ops.actions import (
    NewApiBalanceAction,
    ProviderAction,
)
from src.services.provider_ops.architectures.base import (
    ProviderArchitecture,
    ProviderConnector,
)
from src.services.provider_ops.types import ConnectorAuthType, ProviderActionType

COMPAT_USER_ID_HEADER_NAMES = (
    "New-Api-User",
    "New-API-User",
    "Veloera-User",
    "voapi-user",
    "User-id",
    "Rix-Api-User",
    "neo-api-user",
)


class NewApiConnector(ProviderConnector):
    """
    New API 专用连接器

    特点：
    - 使用 Bearer Token 认证
    - 可选 New-Api-User Header（部分站点需要）
    """

    auth_type = ConnectorAuthType.API_KEY
    display_name = "New API Key"

    def __init__(self, base_url: str, config: dict[str, Any] | None = None):
        super().__init__(base_url, config)
        self._api_key: str | None = None
        self._user_id: str | None = None
        self._cookie: str | None = None

    async def connect(self, credentials: dict[str, Any]) -> bool:
        """建立连接"""
        api_key = credentials.get("api_key")
        cookie = (
            credentials.get("cookie")
            or credentials.get("session_cookie")
            or credentials.get("cookie_string")
            or credentials.get("auth_cookie")
            or credentials.get("token_cookie")
        )
        user_id = credentials.get("user_id")

        # api_key 和 cookie 至少需要一个
        if not api_key and not cookie:
            self._set_error("访问令牌和 Cookie 至少需要填写一个")
            return False

        self._api_key = api_key
        self._user_id = str(user_id) if user_id else None
        self._cookie = cookie
        self._set_connected()
        return True

    async def disconnect(self) -> None:
        """断开连接"""
        self._api_key = None
        self._user_id = None
        self._cookie = None
        self._set_disconnected()

    async def is_authenticated(self) -> bool:
        """检查是否已认证"""
        # 有 cookie 或 api_key 即可视为已认证
        if self._cookie:
            return True
        return self._api_key is not None

    def _apply_auth(self, request: httpx.Request) -> httpx.Request:
        """为请求应用认证信息"""
        # 添加浏览器指纹 Headers 以绕过 Cloudflare 等防护
        for key, value in BROWSER_FINGERPRINT_HEADERS.items():
            request.headers.setdefault(key, value)

        if self._api_key:
            request.headers["Authorization"] = f"Bearer {self._api_key}"
        if self._user_id:
            for header_name in COMPAT_USER_ID_HEADER_NAMES:
                request.headers[header_name] = self._user_id
        if self._cookie:
            request.headers["Cookie"] = self._cookie
        return request

    @classmethod
    def get_credentials_schema(cls) -> dict[str, Any]:
        """获取凭据配置 schema"""
        return {
            "type": "object",
            "properties": {
                "base_url": {
                    "type": "string",
                    "title": "站点地址",
                    "description": "API 基础地址",
                },
                "api_key": {
                    "type": "string",
                    "title": "访问令牌 (API Key)",
                    "description": "New API 的访问令牌，与 Cookie 二选一",
                    "x-sensitive": True,
                    "x-input-type": "password",
                },
                "user_id": {
                    "type": "string",
                    "title": "用户 ID",
                    "description": "可选；仅部分站点需要 New-Api-User Header",
                },
                "cookie": {
                    "type": "string",
                    "title": "Cookie",
                    "description": "用于 Cookie 认证，与访问令牌二选一",
                    "x-sensitive": True,
                    "x-input-type": "password",
                },
            },
            "required": [],
            "x-field-groups": [
                {"fields": ["base_url"]},
                {
                    "fields": ["cookie"],
                    "x-help": "从浏览器开发者工具复制完整 Cookie",
                },
                {
                    "layout": "inline",
                    "fields": ["api_key", "user_id"],
                    "x-flex": {"api_key": 3, "user_id": 1},
                },
            ],
            "x-auth-type": "api_key",
            "x-auth-method": "bearer",
            "x-validation": [
                {
                    "type": "any_required",
                    "fields": ["api_key", "cookie"],
                    "message": "访问令牌和 Cookie 至少需要填写一个",
                },
            ],
            "x-quota-divisor": 500000,
            "x-currency": "USD",
            "x-field-hooks": {
                "cookie": {
                    "action": "parse_new_api_user_id",
                    "target": "user_id",
                },
            },
        }


class NewApiArchitecture(ProviderArchitecture):
    """
    New API 架构预设

    针对 New API 风格的中转站优化的预设配置。

    特点：
    - 使用 Bearer Token 认证
    - 可选 New-Api-User Header
    - 验证端点: /api/user/self
    - quota 单位通常是 1/500000 美元
    """

    architecture_id = "new_api"
    display_name = "New API"
    description = "New API 风格中转站的预设配置"

    supported_connectors: list[type[ProviderConnector]] = [
        NewApiConnector,
    ]

    supported_actions: list[type[ProviderAction]] = [
        NewApiBalanceAction,
    ]

    default_action_configs: dict[ProviderActionType, dict[str, Any]] = {
        ProviderActionType.QUERY_BALANCE: {
            "endpoint": "/api/user/self",
            "method": "GET",
            "quota_divisor": 500000,  # New API 的 quota 单位是 1/500000 美元
            "checkin_endpoint": "/api/user/checkin",  # 签到端点
        },
    }

    def get_credentials_schema(self) -> dict[str, Any]:
        """New API 需要 api_key，user_id 可选"""
        return NewApiConnector.get_credentials_schema()

    def get_verify_endpoint(self) -> str:
        """New API 验证端点"""
        return "/api/user/self"

    def build_verify_headers(
        self,
        config: dict[str, Any],
        credentials: dict[str, Any],
    ) -> dict[str, str]:
        """
        构建 New API 的验证请求 Headers

        New API 可选 New-Api-User Header 传递用户 ID
        """
        # 以浏览器指纹 Headers 为基础，绕过 Cloudflare 等防护
        headers: dict[str, str] = {**BROWSER_FINGERPRINT_HEADERS}

        # Bearer Token 认证
        api_key = credentials.get("api_key", "")
        if api_key:
            headers["Authorization"] = f"Bearer {api_key}"

        # New API 特有的 header
        user_id = credentials.get("user_id", "")
        if user_id:
            user_id_value = str(user_id)
            for header_name in COMPAT_USER_ID_HEADER_NAMES:
                headers[header_name] = user_id_value

        # 可选的 Cookie
        cookie = credentials.get("cookie", "")
        if cookie:
            headers["Cookie"] = cookie

        return headers
