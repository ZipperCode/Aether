from __future__ import annotations

from typing import Any

from src.core.api_format.transformers.base import FormatTransformer

_GLOBAL_REGISTRY: "TransformerRegistry | None" = None


class TransformerRegistry:
    def __init__(self) -> None:
        self._registry: dict[str, type[Any]] = {}

    def register(self, transformer_cls: type[Any]) -> None:
        name = str(getattr(transformer_cls, "NAME", "") or "").strip().lower()
        if not name:
            raise ValueError("transformer NAME 不能为空")
        self._registry[name] = transformer_cls

    def is_registered(self, name: str) -> bool:
        return str(name).strip().lower() in self._registry

    def create(self, name: str) -> FormatTransformer:
        key = str(name).strip().lower()
        transformer_cls = self._registry.get(key)
        if transformer_cls is None:
            raise KeyError(f"未注册 transformer: {name}")
        return transformer_cls()


def register_builtin_transformers(registry: TransformerRegistry) -> TransformerRegistry:
    from src.core.api_format.transformers.builtin.enhancetool import EnhanceToolTransformer
    from src.core.api_format.transformers.builtin.cleancache import CleanCacheTransformer
    from src.core.api_format.transformers.builtin.maxtoken import MaxTokenTransformer
    from src.core.api_format.transformers.builtin.reasoning import ReasoningTransformer
    from src.core.api_format.transformers.builtin.sampling import SamplingTransformer
    from src.core.api_format.transformers.builtin.tooluse import ToolUseTransformer

    registry.register(ToolUseTransformer)
    registry.register(EnhanceToolTransformer)
    registry.register(ReasoningTransformer)
    registry.register(SamplingTransformer)
    registry.register(MaxTokenTransformer)
    registry.register(CleanCacheTransformer)
    return registry


def get_transformer_registry() -> TransformerRegistry:
    global _GLOBAL_REGISTRY  # noqa: PLW0603
    if _GLOBAL_REGISTRY is None:
        _GLOBAL_REGISTRY = register_builtin_transformers(TransformerRegistry())
    return _GLOBAL_REGISTRY
