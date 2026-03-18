"""Search Pool Gateway module definition."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from fastapi import APIRouter

from src.core.modules.base import ModuleCategory, ModuleDefinition, ModuleHealth, ModuleMetadata

if TYPE_CHECKING:
    from sqlalchemy.orm import Session


def _get_router() -> Any:
    from src.modules.search_pool_gateway.routes_admin import router as admin_router
    from src.modules.search_pool_gateway.routes_proxy import router as proxy_router

    combined = APIRouter()
    combined.include_router(admin_router)
    combined.include_router(proxy_router)
    return combined


async def _on_startup() -> None:
    from src.modules.search_pool_gateway.sqlite import get_engine, init_schema

    init_schema(get_engine())


async def _on_shutdown() -> None:
    return None


async def _health_check() -> ModuleHealth:
    return ModuleHealth.HEALTHY


def _validate_config(db: Session) -> tuple[bool, str]:
    _ = db
    return True, ""


search_pool_gateway_module = ModuleDefinition(
    metadata=ModuleMetadata(
        name="search_pool_gateway",
        display_name="搜索池网关",
        description="Tavily / Firecrawl 搜索池与兼容网关",
        category=ModuleCategory.INTEGRATION,
        env_key="SEARCH_POOL_GATEWAY_AVAILABLE",
        default_available=True,
        required_packages=[],
        api_prefix="/api/admin/search-pool",
        admin_route="/admin/search-pool",
        admin_menu_icon="Key",
        admin_menu_group="system",
        admin_menu_order=59,
    ),
    router_factory=_get_router,
    on_startup=_on_startup,
    on_shutdown=_on_shutdown,
    health_check=_health_check,
    validate_config=_validate_config,
)
