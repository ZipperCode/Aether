from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable

from src.services.provider_ops.types import (
    ActionResult,
    ActionStatus,
    ConnectorAuthType,
    ProviderActionType,
    ProviderOpsConfig,
)


@dataclass
class OpsExecutionTarget:
    """统一执行目标（Provider / SiteAccount）。"""

    target_id: str
    architecture_id: str
    base_url: str
    auth_type: ConnectorAuthType
    connector_config: dict[str, Any] = field(default_factory=dict)
    credentials: dict[str, Any] = field(default_factory=dict)
    actions: dict[str, dict[str, Any]] = field(default_factory=dict)

    @classmethod
    def from_provider_config(
        cls,
        *,
        target_id: str,
        base_url: str,
        config: ProviderOpsConfig,
        credentials: dict[str, Any],
    ) -> OpsExecutionTarget:
        return cls(
            target_id=target_id,
            architecture_id=config.architecture_id,
            base_url=base_url,
            auth_type=config.connector_auth_type,
            connector_config=dict(config.connector_config or {}),
            credentials=dict(credentials or {}),
            actions=dict(config.actions or {}),
        )


class OpsExecutionEngine:
    """Provider/SiteAccount 共用执行引擎。"""

    def __init__(
        self,
        registry_getter: Callable[[], Any] | None = None,
    ) -> None:
        self._registry_getter = registry_getter or self._default_registry_getter

    @staticmethod
    def _default_registry_getter() -> Any:
        from src.services.provider_ops.registry import get_registry

        return get_registry()

    def create_connector(self, target: OpsExecutionTarget) -> Any:
        registry = self._registry_getter()
        architecture = registry.get_or_default(target.architecture_id)
        return architecture.get_connector(
            base_url=target.base_url,
            auth_type=target.auth_type,
            config=target.connector_config,
        )

    async def execute(
        self,
        *,
        connector: Any,
        target: OpsExecutionTarget,
        action_type: ProviderActionType,
        action_config: dict[str, Any] | None = None,
    ) -> ActionResult:
        if not await connector.is_authenticated():
            return ActionResult(
                status=ActionStatus.AUTH_EXPIRED,
                action_type=action_type,
                message="认证已过期，请重新连接",
            )

        registry = self._registry_getter()
        architecture = registry.get_or_default(target.architecture_id)
        if not architecture.supports_action(action_type):
            return ActionResult(
                status=ActionStatus.NOT_SUPPORTED,
                action_type=action_type,
                message=f"架构 {architecture.architecture_id} 不支持 {action_type.value} 操作",
            )

        merged_config = self.build_action_config(
            target=target,
            action_type=action_type,
            action_config=action_config,
        )
        action = architecture.get_action(action_type, merged_config)
        async with connector.get_client() as client:
            return await action.execute(client)

    def build_action_config(
        self,
        *,
        target: OpsExecutionTarget,
        action_type: ProviderActionType,
        action_config: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        saved_action_config = target.actions.get(action_type.value, {}).get("config", {})
        merged_config = {**saved_action_config, **(action_config or {})}
        merged_config["_provider_id"] = target.target_id

        cookie_candidates = ("cookie", "session_cookie", "cookie_string", "auth_cookie", "token_cookie")
        has_cookie = any(str(target.credentials.get(key) or "").strip() for key in cookie_candidates)
        if has_cookie:
            merged_config["_has_cookie"] = True
        merged_config["_has_user_id"] = bool(str(target.credentials.get("user_id") or "").strip())
        return merged_config
