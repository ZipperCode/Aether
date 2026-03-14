"""
Key 级上游模型同步服务。

强制从上游获取模型，并覆盖 Key 的 allowed_models，
不依赖 auto_fetch_models 也不应用过滤/锁定规则。
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any

from sqlalchemy.orm import Session, joinedload

from src.core.crypto import crypto_service
from src.core.exceptions import NotFoundException
from src.core.provider_types import ProviderType
from src.models.database import Provider, ProviderAPIKey
from src.services.model.fetch_scheduler import MODEL_FETCH_HTTP_TIMEOUT, set_upstream_models_to_cache
from src.services.model.global_model import on_key_allowed_models_changed
from src.services.model.upstream_fetcher import (
    UpstreamModelsFetchContext,
    UpstreamModelsFetcherRegistry,
    build_format_to_config,
    fetch_models_for_key,
    merge_upstream_metadata,
)
from src.services.provider.envelope import ensure_providers_bootstrapped
from src.services.provider.oauth_token import resolve_oauth_access_token
from src.services.proxy_node.resolver import resolve_effective_proxy


def _aggregate_models_for_cache(models: list[dict]) -> list[dict]:
    """按 model id 聚合模型，合并 api_format 到 api_formats。"""
    model_map: dict[str, dict] = {}

    for model in models:
        model_id = model.get("id")
        if not model_id:
            continue

        api_format = model.get("api_format", "")
        existing_formats = model.get("api_formats") or []

        if model_id not in model_map:
            aggregated = {"id": model_id, "api_formats": []}
            for key, value in model.items():
                if key not in ("id", "api_format", "api_formats"):
                    aggregated[key] = value
            model_map[model_id] = aggregated

        if api_format and api_format not in model_map[model_id]["api_formats"]:
            model_map[model_id]["api_formats"].append(api_format)

        for fmt in existing_formats:
            if fmt and fmt not in model_map[model_id]["api_formats"]:
                model_map[model_id]["api_formats"].append(fmt)

    result = list(model_map.values())
    for model in result:
        model["api_formats"].sort()

    result.sort(key=lambda m: m["id"])
    return result


async def sync_key_models(db: Session, key_id: str) -> dict[str, Any]:
    """强制同步指定 Key 的上游模型并覆盖 allowed_models。"""
    key = (
        db.query(ProviderAPIKey)
        .options(joinedload(ProviderAPIKey.provider).joinedload(Provider.endpoints))
        .filter(ProviderAPIKey.id == key_id)
        .first()
    )
    if not key:
        raise NotFoundException(f"Key {key_id} 不存在")

    provider = getattr(key, "provider", None)
    now = datetime.now(timezone.utc)

    if not provider:
        key.last_models_fetch_at = now
        key.last_models_fetch_error = "Provider not found"
        db.commit()
        return {"success": False, "models_count": 0, "error": "Provider not found"}

    ensure_providers_bootstrapped()
    provider_type = str(getattr(provider, "provider_type", "") or "")

    format_to_endpoint = build_format_to_config(getattr(provider, "endpoints", []) or [])
    has_custom_fetcher = UpstreamModelsFetcherRegistry.get(provider_type) is not None
    if not format_to_endpoint and not has_custom_fetcher:
        key.last_models_fetch_at = now
        key.last_models_fetch_error = "No active endpoints"
        db.commit()
        return {"success": False, "models_count": 0, "error": "No active endpoints"}

    if not getattr(key, "api_key", None):
        key.last_models_fetch_at = now
        key.last_models_fetch_error = "No API key configured"
        db.commit()
        return {"success": False, "models_count": 0, "error": "No API key configured"}

    auth_type = str(getattr(key, "auth_type", "api_key") or "api_key").lower()
    proxy_config = resolve_effective_proxy(
        getattr(provider, "proxy", None), getattr(key, "proxy", None)
    )

    api_key_value: str = ""
    auth_config: dict[str, Any] | None = None

    if auth_type == "oauth":
        endpoint_api_format = (
            "gemini:chat"
            if provider_type.lower() == ProviderType.ANTIGRAVITY.value
            else None
        )
        try:
            resolved = await resolve_oauth_access_token(
                key_id=str(key.id),
                encrypted_api_key=str(key.api_key or ""),
                encrypted_auth_config=(
                    str(key.auth_config) if getattr(key, "auth_config", None) else None
                ),
                provider_proxy_config=proxy_config,
                endpoint_api_format=endpoint_api_format,
            )
            api_key_value = resolved.access_token
            auth_config = resolved.decrypted_auth_config
        except Exception as exc:
            key.last_models_fetch_at = now
            key.last_models_fetch_error = f"OAuth token resolution failed: {exc}"
            db.commit()
            return {
                "success": False,
                "models_count": 0,
                "error": key.last_models_fetch_error,
            }
    else:
        try:
            api_key_value = crypto_service.decrypt(str(key.api_key))
        except Exception:
            key.last_models_fetch_at = now
            key.last_models_fetch_error = "Decrypt error"
            db.commit()
            return {"success": False, "models_count": 0, "error": "Decrypt error"}

        encrypted_auth_config = getattr(key, "auth_config", None)
        if isinstance(encrypted_auth_config, str) and encrypted_auth_config:
            try:
                decrypted = crypto_service.decrypt(encrypted_auth_config)
                parsed = json.loads(decrypted)
                auth_config = parsed if isinstance(parsed, dict) else None
            except Exception:
                auth_config = None

    fetch_ctx = UpstreamModelsFetchContext(
        provider_type=provider_type,
        api_key_value=str(api_key_value or ""),
        format_to_endpoint=format_to_endpoint,
        proxy_config=proxy_config,
        auth_config=auth_config,
    )

    all_models, errors, has_success, upstream_metadata = await fetch_models_for_key(
        fetch_ctx, timeout_seconds=MODEL_FETCH_HTTP_TIMEOUT
    )

    key.last_models_fetch_at = now

    if not has_success:
        error_msg = "; ".join(errors) if errors else "All endpoints failed"
        key.last_models_fetch_error = error_msg
        db.commit()
        return {"success": False, "models_count": 0, "error": error_msg}

    key.last_models_fetch_error = None
    if upstream_metadata and isinstance(upstream_metadata, dict):
        key.upstream_metadata = merge_upstream_metadata(key.upstream_metadata, upstream_metadata)

    model_ids = sorted({m.get("id") for m in all_models if m.get("id")})
    key.allowed_models = list(model_ids)

    unique_models = _aggregate_models_for_cache(all_models)
    if unique_models:
        await set_upstream_models_to_cache(provider.id, key.id, unique_models)

    db.commit()

    await on_key_allowed_models_changed(
        db=db,
        provider_id=str(provider.id),
        allowed_models=list(key.allowed_models or []),
    )

    return {"success": True, "models_count": len(model_ids)}
