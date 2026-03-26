from __future__ import annotations

from copy import deepcopy
from typing import Any


TransformerSpec = dict[str, Any]


def merge_transformer_specs(*spec_groups: list[TransformerSpec] | None) -> list[TransformerSpec]:
    merged: dict[str, TransformerSpec] = {}
    order: list[str] = []

    for group in spec_groups:
        for raw_spec in group or []:
            name = str(raw_spec.get("name") or "").strip()
            if not name:
                continue

            key = name.lower()
            previous = merged.get(key, {"name": name, "enabled": True, "config": {}})
            current = deepcopy(raw_spec)
            current_name = str(current.get("name") or name).strip() or name
            current_enabled = bool(current.get("enabled", previous.get("enabled", True)))
            current_config = deepcopy(previous.get("config") or {})
            current_config.update(deepcopy(current.get("config") or {}))

            merged[key] = {
                "name": current_name,
                "enabled": current_enabled,
                "config": current_config,
            }
            if key not in order:
                order.append(key)

    result: list[TransformerSpec] = []
    for key in order:
        spec = merged[key]
        if spec.get("enabled", True):
            result.append(spec)
    return result
