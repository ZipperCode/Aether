from __future__ import annotations

from src.core.api_format.conversion.internal import InternalRequest, ThinkingConfig
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.reasoning import ReasoningTransformer


def test_reasoning_transformer_maps_effort_to_budget_tokens() -> None:
    transformer = ReasoningTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        extra={"reasoning_effort": "high"},
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    assert out.thinking is not None
    assert out.thinking.enabled is True
    assert out.thinking.budget_tokens is not None


def test_reasoning_transformer_maps_budget_tokens_back_to_effort() -> None:
    transformer = ReasoningTransformer()
    internal = InternalRequest(
        model="claude-3-7-sonnet",
        messages=[],
        thinking=ThinkingConfig(enabled=True, budget_tokens=2048),
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="claude:chat", provider_format="openai:chat"),
    )

    assert out.extra["reasoning_effort"] in {"medium", "high"}


def test_reasoning_transformer_records_degradation_when_target_cannot_express_reasoning() -> None:
    transformer = ReasoningTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        extra={"reasoning_effort": "high"},
    )
    ctx = TransformContext(
        stage="request",
        client_format="openai:chat",
        provider_format="custom:chat",
    )

    out = transformer.transform_request(internal, ctx)

    assert out.extra["reasoning_effort"] == "high"
    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "reasoning",
            "code": "reasoning_not_supported",
            "message": "target format does not support reasoning controls",
            "severity": "warning",
            "details": {"provider_format": "custom:chat"},
        }
    ]
