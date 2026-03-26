from __future__ import annotations

from copy import deepcopy
from typing import Any

from src.core.api_format.conversion import format_conversion_registry, register_default_normalizers
from src.core.api_format.conversion.stream_state import StreamState
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.config import TransformerSpec, merge_transformer_specs
from src.core.api_format.transformers.pipeline import TransformerPipeline
from src.core.api_format.transformers.registry import get_transformer_registry


def resolve_transformer_specs(
    *,
    provider_defaults: list[TransformerSpec] | None,
    endpoint_specs: list[TransformerSpec] | None,
) -> list[TransformerSpec]:
    return merge_transformer_specs(provider_defaults, endpoint_specs)


def apply_request_transformers(
    *,
    request_body: dict[str, Any],
    source_format: str,
    target_format: str,
    specs: list[TransformerSpec] | None,
    context: TransformContext,
    transformer_registry=None,
    target_variant: str | None = None,
    output_limit: int | None = None,
) -> dict[str, Any]:
    if not specs:
        return deepcopy(request_body)

    register_default_normalizers()
    source_normalizer = format_conversion_registry.get_normalizer(source_format)
    target_normalizer = format_conversion_registry.get_normalizer(target_format)
    if source_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {source_format}")
    if target_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {target_format}")

    internal = source_normalizer.request_to_internal(deepcopy(request_body))
    if output_limit is not None:
        internal.output_limit = output_limit
    pipeline = TransformerPipeline(
        registry=transformer_registry or get_transformer_registry(),
        specs=specs,
    )
    transformed = pipeline.transform_request(internal, context)
    return target_normalizer.request_from_internal(transformed, target_variant=target_variant)


def apply_response_transformers(
    *,
    response_body: dict[str, Any],
    source_format: str,
    target_format: str,
    specs: list[TransformerSpec] | None,
    context: TransformContext,
    transformer_registry=None,
) -> dict[str, Any]:
    if not specs:
        return deepcopy(response_body)

    register_default_normalizers()
    source_normalizer = format_conversion_registry.get_normalizer(source_format)
    target_normalizer = format_conversion_registry.get_normalizer(target_format)
    if source_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {source_format}")
    if target_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {target_format}")

    internal = source_normalizer.response_to_internal(deepcopy(response_body))
    pipeline = TransformerPipeline(
        registry=transformer_registry or get_transformer_registry(),
        specs=specs,
    )
    transformed = pipeline.transform_response(internal, context)
    return target_normalizer.response_from_internal(transformed)


def apply_error_transformers(
    *,
    error_body: dict[str, Any],
    source_format: str,
    target_format: str,
    specs: list[TransformerSpec] | None,
    context: TransformContext,
    transformer_registry=None,
) -> dict[str, Any]:
    if not specs:
        return deepcopy(error_body)

    register_default_normalizers()
    source_normalizer = format_conversion_registry.get_normalizer(source_format)
    target_normalizer = format_conversion_registry.get_normalizer(target_format)
    if source_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {source_format}")
    if target_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {target_format}")

    internal = source_normalizer.error_to_internal(deepcopy(error_body))
    pipeline = TransformerPipeline(
        registry=transformer_registry or get_transformer_registry(),
        specs=specs,
    )
    transformed = pipeline.transform_error(internal, context)
    return target_normalizer.error_from_internal(transformed)


def apply_stream_transformers(
    *,
    chunk: dict[str, Any],
    source_format: str,
    target_format: str,
    specs: list[TransformerSpec] | None,
    context: TransformContext,
    source_state: StreamState,
    target_state: StreamState,
    transformer_registry=None,
) -> list[dict[str, Any]]:
    if not specs:
        return deepcopy([chunk])

    register_default_normalizers()
    source_normalizer = format_conversion_registry.get_normalizer(source_format)
    target_normalizer = format_conversion_registry.get_normalizer(target_format)
    if source_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {source_format}")
    if target_normalizer is None:
        raise RuntimeError(f"未注册 Normalizer: {target_format}")

    internal_events = source_normalizer.stream_chunk_to_internal(deepcopy(chunk), source_state)
    pipeline = TransformerPipeline(
        registry=transformer_registry or get_transformer_registry(),
        specs=specs,
    )
    out: list[dict[str, Any]] = []
    for event in internal_events:
        transformed_events = pipeline.transform_stream_event(event, context)
        for transformed_event in transformed_events:
            out.extend(target_normalizer.stream_event_from_internal(transformed_event, target_state))
    return out
