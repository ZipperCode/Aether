"""
Handler 基类模块

提供 Adapter、Handler 的抽象基类，以及请求构建器和响应解析器。

注意：Handler 基类（ChatHandlerBase, CliMessageHandlerBase 等）不在这里导出，
因为它们依赖 services.usage.stream，而后者又需要导入 response_parser，
会形成循环导入。请直接从具体模块导入 Handler 基类。
"""

from __future__ import annotations

from typing import Any

__all__ = [
    # Chat Adapter
    "ChatAdapterBase",
    "register_adapter",
    "get_adapter_class",
    "get_adapter_instance",
    "list_registered_formats",
    # CLI Adapter
    "CliAdapterBase",
    "register_cli_adapter",
    "get_cli_adapter_class",
    "get_cli_adapter_instance",
    "list_registered_cli_formats",
    # 请求构建器
    "RequestBuilder",
    "PassthroughRequestBuilder",
    "build_passthrough_request",
    "SENSITIVE_HEADERS",
    # 响应解析器
    "ResponseParser",
    "ParsedChunk",
    "ParsedResponse",
    "StreamStats",
]

_LAZY_IMPORTS: dict[str, tuple[str, str]] = {
    # Chat Adapter
    "ChatAdapterBase": ("src.api.handlers.base.chat_adapter_base", "ChatAdapterBase"),
    "register_adapter": ("src.api.handlers.base.chat_adapter_base", "register_adapter"),
    "get_adapter_class": ("src.api.handlers.base.chat_adapter_base", "get_adapter_class"),
    "get_adapter_instance": ("src.api.handlers.base.chat_adapter_base", "get_adapter_instance"),
    "list_registered_formats": (
        "src.api.handlers.base.chat_adapter_base",
        "list_registered_formats",
    ),
    # CLI Adapter
    "CliAdapterBase": ("src.api.handlers.base.cli_adapter_base", "CliAdapterBase"),
    "register_cli_adapter": (
        "src.api.handlers.base.cli_adapter_base",
        "register_cli_adapter",
    ),
    "get_cli_adapter_class": (
        "src.api.handlers.base.cli_adapter_base",
        "get_cli_adapter_class",
    ),
    "get_cli_adapter_instance": (
        "src.api.handlers.base.cli_adapter_base",
        "get_cli_adapter_instance",
    ),
    "list_registered_cli_formats": (
        "src.api.handlers.base.cli_adapter_base",
        "list_registered_cli_formats",
    ),
    # 请求构建器
    "RequestBuilder": ("src.api.handlers.base.request_builder", "RequestBuilder"),
    "PassthroughRequestBuilder": (
        "src.api.handlers.base.request_builder",
        "PassthroughRequestBuilder",
    ),
    "build_passthrough_request": (
        "src.api.handlers.base.request_builder",
        "build_passthrough_request",
    ),
    "SENSITIVE_HEADERS": ("src.api.handlers.base.request_builder", "SENSITIVE_HEADERS"),
    # 响应解析器
    "ResponseParser": ("src.api.handlers.base.response_parser", "ResponseParser"),
    "ParsedChunk": ("src.api.handlers.base.response_parser", "ParsedChunk"),
    "ParsedResponse": ("src.api.handlers.base.response_parser", "ParsedResponse"),
    "StreamStats": ("src.api.handlers.base.response_parser", "StreamStats"),
}


def __getattr__(name: str) -> Any:
    """延迟导入以避免不必要的依赖加载。"""
    if name in _LAZY_IMPORTS:
        module_path, attr_name = _LAZY_IMPORTS[name]
        import importlib

        module = importlib.import_module(module_path)
        return getattr(module, attr_name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
