from __future__ import annotations

from dataclasses import replace

from src.core.api_format.conversion.internal import (
    ErrorType,
    InternalError,
    InternalRequest,
    InternalResponse,
    InternalMessage,
    Role,
    TextBlock,
)
from src.core.api_format.conversion.stream_events import ContentDeltaEvent
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.config import merge_transformer_specs
from src.core.api_format.transformers.pipeline import TransformerPipeline
from src.core.api_format.transformers.registry import (
    TransformerRegistry,
    get_transformer_registry,
    register_builtin_transformers,
)


class TrackingTransformer:
    NAME = "tracking"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        internal.extra.setdefault("trace", []).append(f"{self.NAME}:{ctx.stage}")
        return internal

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse:
        internal.extra.setdefault("trace", []).append(f"{self.NAME}:{ctx.stage}")
        return internal

    def transform_stream_event(
        self,
        event: ContentDeltaEvent,
        ctx: TransformContext,
    ) -> list[ContentDeltaEvent]:
        return [replace(event, text_delta=f"{event.text_delta}|{self.NAME}:{ctx.stage}")]

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError:
        internal.extra.setdefault("trace", []).append(f"{self.NAME}:{ctx.stage}")
        return internal


def _make_request() -> InternalRequest:
    return InternalRequest(
        model="gpt-4o-mini",
        messages=[InternalMessage(role=Role.USER, content=[TextBlock(text="hi")])],
    )


def _make_response() -> InternalResponse:
    return InternalResponse(
        id="resp_1",
        model="gpt-4o-mini",
        content=[TextBlock(text="hello")],
    )


def test_merge_transformer_specs_overrides_and_disables() -> None:
    merged = merge_transformer_specs(
        [{"name": "tooluse"}, {"name": "reasoning", "config": {"mode": "safe"}}],
        [{"name": "tooluse", "config": {"repair_ids": True}}],
        [{"name": "reasoning", "enabled": False}],
    )

    assert merged == [
        {"name": "tooluse", "enabled": True, "config": {"repair_ids": True}},
    ]


def test_registry_register_and_create() -> None:
    registry = TransformerRegistry()
    registry.register(TrackingTransformer)

    transformer = registry.create("tracking")

    assert isinstance(transformer, TrackingTransformer)


def test_pipeline_runs_request_response_stream_and_error_stages() -> None:
    registry = TransformerRegistry()
    registry.register(TrackingTransformer)
    pipeline = TransformerPipeline(
        registry=registry,
        specs=[{"name": "tracking"}],
    )

    request_ctx = TransformContext(stage="request", client_format="openai:chat")
    request = pipeline.transform_request(_make_request(), request_ctx)
    assert request.extra["trace"] == ["tracking:request"]

    response_ctx = TransformContext(stage="response", client_format="openai:chat")
    response = pipeline.transform_response(_make_response(), response_ctx)
    assert response.extra["trace"] == ["tracking:response"]

    stream_ctx = TransformContext(stage="stream", client_format="openai:chat")
    events = pipeline.transform_stream_event(ContentDeltaEvent(text_delta="hi"), stream_ctx)
    assert len(events) == 1
    assert events[0].text_delta == "hi|tracking:stream"

    error_ctx = TransformContext(stage="error", client_format="openai:chat")
    error = pipeline.transform_error(
        InternalError(type=ErrorType.INVALID_REQUEST, message="boom"),
        error_ctx,
    )
    assert error.extra["trace"] == ["tracking:error"]


def test_pipeline_skips_disabled_transformer() -> None:
    registry = TransformerRegistry()
    registry.register(TrackingTransformer)
    pipeline = TransformerPipeline(
        registry=registry,
        specs=[{"name": "tracking", "enabled": False}],
    )

    request = pipeline.transform_request(
        _make_request(),
        TransformContext(stage="request", client_format="openai:chat"),
    )

    assert request.extra == {}


def test_builtin_transformer_registration_is_idempotent() -> None:
    registry = TransformerRegistry()

    register_builtin_transformers(registry)
    register_builtin_transformers(registry)

    assert registry.is_registered("tooluse")
    assert registry.is_registered("reasoning")


def test_get_transformer_registry_returns_shared_instance() -> None:
    registry1 = get_transformer_registry()
    registry2 = get_transformer_registry()

    assert registry1 is registry2
    assert registry1.is_registered("tooluse")
