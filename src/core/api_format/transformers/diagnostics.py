from __future__ import annotations

from copy import deepcopy
from typing import Any


def copy_transformer_diagnostics(
    diagnostics: list[dict[str, Any]] | None,
) -> list[dict[str, Any]]:
    if not diagnostics:
        return []
    return deepcopy(list(diagnostics))


def merge_transformer_diagnostics(
    metadata: dict[str, Any] | None,
    diagnostics: list[dict[str, Any]] | None,
) -> dict[str, Any]:
    result = dict(metadata or {})
    items = copy_transformer_diagnostics(diagnostics)
    if not items:
        return result
    existing = result.get("transformer_diagnostics")
    if isinstance(existing, list):
        result["transformer_diagnostics"] = list(existing) + items
    else:
        result["transformer_diagnostics"] = items
    return result


def log_transformer_diagnostics(
    logger: Any,
    *,
    request_id: str | None,
    diagnostics: list[dict[str, Any]] | None,
    phase: str,
) -> None:
    if not diagnostics:
        return

    request_id_text = str(request_id or "").strip() or "-"
    for item in diagnostics:
        severity = str(item.get("severity") or "info").strip().lower()
        log_fn = logger.warning if severity == "warning" else logger.info
        log_fn(
            "[{}] transformer diagnostic | phase={} | transformer={} | code={} | message={}",
            request_id_text,
            phase,
            item.get("transformer") or "-",
            item.get("code") or "-",
            item.get("message") or "-",
        )
