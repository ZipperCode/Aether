"""
Provider 操作模块

提供对提供商的扩展操作支持：
- 多种鉴权方式（API Key、登录、Cookie）
- 可扩展的操作类型（余额查询、签到等）
"""

from __future__ import annotations

from typing import Any

__all__ = [
    # 服务
    "ProviderOpsService",
    # 注册表
    "ArchitectureRegistry",
    "get_registry",
    # 类型
    "ActionResult",
    "ActionStatus",
    "BalanceInfo",
    "CheckinInfo",
    "ConnectorAuthType",
    "ConnectorState",
    "ConnectorStatus",
    "ProviderActionType",
    "ProviderOpsConfig",
]


def __getattr__(name: str) -> Any:
    if name in {"ArchitectureRegistry", "get_registry"}:
        from src.services.provider_ops.registry import ArchitectureRegistry, get_registry

        values = {
            "ArchitectureRegistry": ArchitectureRegistry,
            "get_registry": get_registry,
        }
        return values[name]

    if name == "ProviderOpsService":
        from src.services.provider_ops.service import ProviderOpsService

        return ProviderOpsService

    if name in {
        "ActionResult",
        "ActionStatus",
        "BalanceInfo",
        "CheckinInfo",
        "ConnectorAuthType",
        "ConnectorState",
        "ConnectorStatus",
        "ProviderActionType",
        "ProviderOpsConfig",
    }:
        from src.services.provider_ops.types import (
            ActionResult,
            ActionStatus,
            BalanceInfo,
            CheckinInfo,
            ConnectorAuthType,
            ConnectorState,
            ConnectorStatus,
            ProviderActionType,
            ProviderOpsConfig,
        )

        values = {
            "ActionResult": ActionResult,
            "ActionStatus": ActionStatus,
            "BalanceInfo": BalanceInfo,
            "CheckinInfo": CheckinInfo,
            "ConnectorAuthType": ConnectorAuthType,
            "ConnectorState": ConnectorState,
            "ConnectorStatus": ConnectorStatus,
            "ProviderActionType": ProviderActionType,
            "ProviderOpsConfig": ProviderOpsConfig,
        }
        return values[name]

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
