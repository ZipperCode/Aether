from __future__ import annotations

from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.registry import TransformerRegistry
from src.core.api_format.transformers.runtime import (
    apply_error_transformers,
    apply_request_transformers,
    apply_response_transformers,
    apply_stream_transformers,
    resolve_transformer_specs,
)
from src.core.api_format.conversion.stream_state import StreamState


class AppendTextTransformer:
    NAME = "append_text"

    def transform_request(self, internal, ctx: TransformContext):
        internal.messages[0].content[0].text += f"|{ctx.stage}"
        return internal

    def transform_response(self, internal, ctx: TransformContext):
        internal.content[0].text += f"|{ctx.stage}"
        return internal

    def transform_stream_event(self, event, ctx: TransformContext):
        if hasattr(event, "text_delta"):
            event.text_delta += f"|{ctx.stage}"
        return [event]

    def transform_error(self, internal, ctx: TransformContext):
        internal.message += f"|{ctx.stage}"
        return internal


class DiagnosticTransformer:
    NAME = "diagnostic"

    def transform_request(self, internal, ctx: TransformContext):
        ctx.add_diagnostic(
            code="request_adjusted",
            message="request adjusted",
            severity="warning",
        )
        return internal

    def transform_response(self, internal, ctx: TransformContext):
        ctx.add_diagnostic(
            code="response_adjusted",
            message="response adjusted",
            severity="info",
        )
        return internal

    def transform_stream_event(self, event, ctx: TransformContext):
        ctx.add_diagnostic(
            code="stream_adjusted",
            message="stream adjusted",
            severity="info",
        )
        return [event]

    def transform_error(self, internal, ctx: TransformContext):
        ctx.add_diagnostic(
            code="error_adjusted",
            message="error adjusted",
            severity="warning",
        )
        return internal


def test_resolve_transformer_specs_prefers_endpoint_over_provider() -> None:
    specs = resolve_transformer_specs(
        provider_defaults=[
            {"name": "tooluse"},
            {"name": "reasoning", "config": {"mode": "safe"}},
        ],
        endpoint_specs=[
            {"name": "reasoning", "config": {"mode": "aggressive"}},
        ],
    )

    assert specs == [
        {"name": "tooluse", "enabled": True, "config": {}},
        {"name": "reasoning", "enabled": True, "config": {"mode": "aggressive"}},
    ]


def test_apply_request_transformers_roundtrips_via_source_and_target_normalizers() -> None:
    registry = TransformerRegistry()
    registry.register(AppendTextTransformer)

    out = apply_request_transformers(
        request_body={
            "model": "gpt-4o-mini",
            "messages": [{"role": "user", "content": "hello"}],
        },
        source_format="openai:chat",
        target_format="claude:chat",
        specs=[{"name": "append_text"}],
        transformer_registry=registry,
        context=TransformContext(stage="request", client_format="openai:chat"),
    )

    assert out["messages"][0]["role"] == "user"
    assert out["messages"][0]["content"] == "hello|request"


def test_apply_response_transformers_roundtrips_via_source_and_target_normalizers() -> None:
    registry = TransformerRegistry()
    registry.register(AppendTextTransformer)

    out = apply_response_transformers(
        response_body={
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1},
        },
        source_format="claude:chat",
        target_format="openai:chat",
        specs=[{"name": "append_text"}],
        transformer_registry=registry,
        context=TransformContext(stage="response", client_format="openai:chat"),
    )

    assert out["choices"][0]["message"]["content"] == "hello|response"


def test_apply_error_transformers_roundtrips_via_source_and_target_normalizers() -> None:
    registry = TransformerRegistry()
    registry.register(AppendTextTransformer)

    out = apply_error_transformers(
        error_body={"error": {"message": "bad request", "type": "invalid_request_error"}},
        source_format="openai:chat",
        target_format="claude:chat",
        specs=[{"name": "append_text"}],
        transformer_registry=registry,
        context=TransformContext(stage="error", client_format="openai:chat"),
    )

    assert out["error"]["message"] == "bad request|error"


def test_apply_stream_transformers_roundtrips_via_internal_events() -> None:
    registry = TransformerRegistry()
    registry.register(AppendTextTransformer)

    out = apply_stream_transformers(
        chunk={
            "id": "chatcmpl_1",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "hi"}}],
        },
        source_format="openai:chat",
        target_format="claude:chat",
        specs=[{"name": "append_text"}],
        transformer_registry=registry,
        context=TransformContext(stage="stream", client_format="openai:chat"),
        source_state=StreamState(),
        target_state=StreamState(model="gpt-4o-mini"),
    )

    assert any(
        isinstance(evt, dict)
        and evt.get("type") == "content_block_delta"
        and evt.get("delta", {}).get("text") == "hi|stream"
        for evt in out
    )


def test_apply_request_transformers_exposes_diagnostics_on_context() -> None:
    registry = TransformerRegistry()
    registry.register(DiagnosticTransformer)
    ctx = TransformContext(stage="request", client_format="openai:chat")

    apply_request_transformers(
        request_body={
            "model": "gpt-4o-mini",
            "messages": [{"role": "user", "content": "hello"}],
        },
        source_format="openai:chat",
        target_format="claude:chat",
        specs=[{"name": "diagnostic"}],
        transformer_registry=registry,
        context=ctx,
    )

    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "diagnostic",
            "code": "request_adjusted",
            "message": "request adjusted",
            "severity": "warning",
        }
    ]
