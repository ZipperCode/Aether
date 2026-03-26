from __future__ import annotations

from typing import Any


def summarize_transformer_diagnostics(
    diagnostics: list[dict[str, Any]] | None,
) -> dict[str, Any] | None:
    if not diagnostics:
        return None

    by_code: dict[str, int] = {}
    by_transformer: dict[str, int] = {}
    count = 0

    for item in diagnostics:
        if not isinstance(item, dict):
            continue
        count += 1
        code = str(item.get("code") or "").strip()
        if code:
            by_code[code] = by_code.get(code, 0) + 1
        transformer = str(item.get("transformer") or "").strip()
        if transformer:
            by_transformer[transformer] = by_transformer.get(transformer, 0) + 1

    return {
        "count": count,
        "by_code": by_code,
        "by_transformer": by_transformer,
    }
