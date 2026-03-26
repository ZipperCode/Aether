from __future__ import annotations

from src.core.api_format.transformers.registry import TransformerRegistry, register_builtin_transformers


def test_register_builtin_transformers_includes_second_phase_transformers() -> None:
    registry = register_builtin_transformers(TransformerRegistry())

    assert registry.is_registered("tooluse")
    assert registry.is_registered("enhancetool")
    assert registry.is_registered("reasoning")
    assert registry.is_registered("sampling")
    assert registry.is_registered("maxtoken")
    assert registry.is_registered("cleancache")
