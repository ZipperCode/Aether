"""Tavily 账号池模块定义。"""

from __future__ import annotations

from typing import TYPE_CHECKING

from src.core.modules.base import ModuleCategory, ModuleDefinition, ModuleHealth, ModuleMetadata

if TYPE_CHECKING:
    from sqlalchemy.orm import Session


async def _health_check() -> ModuleHealth:
    return ModuleHealth.HEALTHY


def _validate_config(db: Session) -> tuple[bool, str]:
    _ = db
    return True, ""


tavily_pool_module = ModuleDefinition(
    metadata=ModuleMetadata(
        name="tavily_pool",
        display_name="Tavily 账号池",
        description="Tavily 账号池与令牌管理模块",
        category=ModuleCategory.INTEGRATION,
        env_key="TAVILY_POOL_AVAILABLE",
        default_available=True,
        required_packages=[],
        api_prefix="/api/admin/tavily-pool",
        admin_route="/admin/tavily-pool",
        admin_menu_icon="Key",
        admin_menu_group="system",
        admin_menu_order=59,
    ),
    health_check=_health_check,
    validate_config=_validate_config,
)
