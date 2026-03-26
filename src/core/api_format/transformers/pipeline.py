from __future__ import annotations

from copy import deepcopy
from typing import Any

from src.core.api_format.conversion.internal import InternalError, InternalRequest, InternalResponse
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.config import TransformerSpec, merge_transformer_specs
from src.core.api_format.transformers.registry import TransformerRegistry


class TransformerPipeline:
    def __init__(
        self,
        *,
        registry: TransformerRegistry,
        specs: list[TransformerSpec] | None = None,
    ) -> None:
        self._registry = registry
        self._specs = merge_transformer_specs(specs)

    def _iter_transformers(self) -> list[tuple[Any, dict[str, Any]]]:
        transformers: list[tuple[Any, dict[str, Any]]] = []
        for spec in self._specs:
            if not spec.get("enabled", True):
                continue
            transformer = self._registry.create(str(spec["name"]))
            config = deepcopy(spec.get("config") or {})
            transformers.append((transformer, config))
        return transformers

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        result = internal
        for transformer, config in self._iter_transformers():
            local_ctx = self._with_config(ctx, config, transformer_name=str(transformer.NAME))
            result = transformer.transform_request(result, local_ctx)
        return result

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse:
        result = internal
        for transformer, config in self._iter_transformers():
            local_ctx = self._with_config(ctx, config, transformer_name=str(transformer.NAME))
            result = transformer.transform_response(result, local_ctx)
        return result

    def transform_stream_event(
        self,
        event: InternalStreamEvent,
        ctx: TransformContext,
    ) -> list[InternalStreamEvent]:
        events: list[InternalStreamEvent] = [event]
        for transformer, config in self._iter_transformers():
            local_ctx = self._with_config(ctx, config, transformer_name=str(transformer.NAME))
            next_events: list[InternalStreamEvent] = []
            for current_event in events:
                next_events.extend(transformer.transform_stream_event(current_event, local_ctx))
            events = next_events
        return events

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError:
        result = internal
        for transformer, config in self._iter_transformers():
            local_ctx = self._with_config(ctx, config, transformer_name=str(transformer.NAME))
            result = transformer.transform_error(result, local_ctx)
        return result

    @staticmethod
    def _with_config(
        ctx: TransformContext,
        config: dict[str, Any],
        *,
        transformer_name: str | None,
    ) -> TransformContext:
        return TransformContext(
            stage=ctx.stage,
            client_format=ctx.client_format,
            provider_format=ctx.provider_format,
            provider_type=ctx.provider_type,
            target_variant=ctx.target_variant,
            model=ctx.model,
            is_stream=ctx.is_stream,
            endpoint_id=ctx.endpoint_id,
            request_id=ctx.request_id,
            transformer_name=transformer_name,
            transformer_config=config,
            diagnostics=ctx.diagnostics,
        )
