"""站点管理模块

提供 all-api-hub 同步与 Provider 签到的运行状态、差异明细可视化。
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from src.core.modules.base import ModuleCategory, ModuleDefinition, ModuleHealth, ModuleMetadata

if TYPE_CHECKING:
    from sqlalchemy.orm import Session


def _get_router() -> Any:
    from src.api.admin.site_management import router

    return router


async def _health_check() -> ModuleHealth:
    return ModuleHealth.HEALTHY


def _validate_config(db: Session) -> tuple[bool, str]:
    _ = db
    return True, ""


site_management_module = ModuleDefinition(
    metadata=ModuleMetadata(
        name="site_management",
        display_name="站点管理",
        description="查看 all-api-hub 同步差异、签到结果与历史记录",
        category=ModuleCategory.INTEGRATION,
        env_key="SITE_MANAGEMENT_AVAILABLE",
        default_available=True,
        required_packages=[],
        api_prefix="/api/admin/site-management",
        admin_route="/admin/site-management",
        admin_menu_icon="Server",
        admin_menu_group="system",
        admin_menu_order=58,
    ),
    router_factory=_get_router,
    health_check=_health_check,
    validate_config=_validate_config,
)
