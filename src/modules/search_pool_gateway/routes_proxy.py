"""Search Pool Gateway compatible proxy routes."""

from __future__ import annotations

from fastapi import APIRouter, Request

from src.modules.search_pool_gateway.services.proxy_service import (
    proxy_firecrawl as proxy_firecrawl_service,
)
from src.modules.search_pool_gateway.services.proxy_service import (
    proxy_tavily as proxy_tavily_service,
)

router = APIRouter(tags=["Search Pool Gateway Proxy"])


@router.post("/api/search")
async def proxy_tavily_search(request: Request):
    return await proxy_tavily_service(request, "search")


@router.post("/api/extract")
async def proxy_tavily_extract(request: Request):
    return await proxy_tavily_service(request, "extract")


@router.api_route("/firecrawl/{path:path}", methods=["GET", "POST", "PUT", "PATCH", "DELETE"])
async def proxy_firecrawl(path: str, request: Request):
    return await proxy_firecrawl_service(path, request)
