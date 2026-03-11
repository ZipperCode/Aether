"""站点管理模块

独立的站点管理模块，支持多 WebDav 源同步、签到、余额查询。
与 Provider 模块完全解耦。
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from src.core.modules.base import ModuleCategory, ModuleDefinition, ModuleHealth, ModuleMetadata

if TYPE_CHECKING:
    from sqlalchemy.orm import Session


def _get_router() -> Any:
    from src.modules.site_management.routes import router

    return router


async def _on_startup() -> None:
    from src.modules.site_management.services.scheduler import SiteManagementScheduler

    scheduler = SiteManagementScheduler()
    scheduler.start()


async def _on_shutdown() -> None:
    from src.modules.site_management.services.scheduler import SiteManagementScheduler

    scheduler = SiteManagementScheduler()
    scheduler.stop()


async def _health_check() -> ModuleHealth:
    return ModuleHealth.HEALTHY


def _validate_config(db: Session) -> tuple[bool, str]:
    _ = db
    return True, ""


site_management_module = ModuleDefinition(
    metadata=ModuleMetadata(
        name="site_management",
        display_name="站点管理",
        description="多 WebDav 源站点管理，支持同步、签到、余额查询",
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
    on_startup=_on_startup,
    on_shutdown=_on_shutdown,
    health_check=_health_check,
    validate_config=_validate_config,
)
