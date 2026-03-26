from __future__ import annotations

from src.core.api_format.conversion.internal import InternalRequest
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.maxtoken import MaxTokenTransformer


def test_maxtoken_transformer_uses_output_limit_when_max_tokens_missing() -> None:
    transformer = MaxTokenTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        output_limit=4096,
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    assert out.max_tokens == 4096


def test_maxtoken_transformer_clamps_max_tokens_to_output_limit() -> None:
    transformer = MaxTokenTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        max_tokens=8192,
        output_limit=4096,
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    assert out.max_tokens == 4096


def test_maxtoken_transformer_records_diagnostics_for_fill_and_clamp() -> None:
    transformer = MaxTokenTransformer()

    fill_ctx = TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat")
    filled = transformer.transform_request(
        InternalRequest(
            model="gpt-5",
            messages=[],
            output_limit=4096,
        ),
        fill_ctx,
    )

    clamp_ctx = TransformContext(
        stage="request",
        client_format="openai:chat",
        provider_format="claude:chat",
    )
    clamped = transformer.transform_request(
        InternalRequest(
            model="gpt-5",
            messages=[],
            max_tokens=8192,
            output_limit=4096,
        ),
        clamp_ctx,
    )

    assert filled.max_tokens == 4096
    assert fill_ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "maxtoken",
            "code": "max_tokens_filled",
            "message": "max_tokens was filled from output_limit",
            "severity": "info",
            "details": {"output_limit": 4096},
        }
    ]

    assert clamped.max_tokens == 4096
    assert clamp_ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "maxtoken",
            "code": "max_tokens_clamped",
            "message": "max_tokens was clamped to output_limit",
            "severity": "warning",
            "details": {"value": 8192, "output_limit": 4096},
        }
    ]
