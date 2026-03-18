"""Mask helpers for search pool gateway."""

from __future__ import annotations


def _derive_mask(raw: str) -> str:
    if len(raw) <= 12:
        return raw
    return f"{raw[:8]}***{raw[-4:]}"


def mask_key(raw: str) -> str:
    return _derive_mask(raw)
