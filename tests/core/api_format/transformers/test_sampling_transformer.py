from __future__ import annotations

from src.core.api_format.conversion.internal import InternalRequest
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.sampling import SamplingTransformer


def test_sampling_transformer_clamps_temperature_and_top_p_for_claude() -> None:
    transformer = SamplingTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        temperature=1.8,
        top_p=1.5,
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    assert out.temperature == 1.0
    assert out.top_p == 1.0


def test_sampling_transformer_drops_unsupported_claude_sampling_fields() -> None:
    transformer = SamplingTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        n=3,
        presence_penalty=0.4,
        frequency_penalty=0.2,
        logprobs=True,
        top_logprobs=5,
        seed=7,
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    assert out.n is None
    assert out.presence_penalty is None
    assert out.frequency_penalty is None
    assert out.logprobs is None
    assert out.top_logprobs is None
    assert out.seed is None


def test_sampling_transformer_records_diagnostics_for_clamped_and_dropped_fields() -> None:
    transformer = SamplingTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[],
        temperature=1.7,
        top_p=1.2,
        n=3,
        presence_penalty=0.6,
        frequency_penalty=0.2,
    )
    ctx = TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat")

    out = transformer.transform_request(internal, ctx)

    assert out.temperature == 1.0
    assert out.top_p == 1.0
    assert out.n is None
    assert out.presence_penalty is None
    assert out.frequency_penalty is None
    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "sampling",
            "code": "sampling_clamped",
            "message": "temperature was clamped to target-supported range",
            "severity": "info",
            "details": {"field": "temperature", "value": 1.7, "clamped_to": 1.0},
        },
        {
            "stage": "request",
            "transformer": "sampling",
            "code": "sampling_clamped",
            "message": "top_p was clamped to target-supported range",
            "severity": "info",
            "details": {"field": "top_p", "value": 1.2, "clamped_to": 1.0},
        },
        {
            "stage": "request",
            "transformer": "sampling",
            "code": "sampling_dropped",
            "message": "field is not supported by target format",
            "severity": "warning",
            "details": {"field": "n", "value": 3},
        },
        {
            "stage": "request",
            "transformer": "sampling",
            "code": "sampling_dropped",
            "message": "field is not supported by target format",
            "severity": "warning",
            "details": {"field": "presence_penalty", "value": 0.6},
        },
        {
            "stage": "request",
            "transformer": "sampling",
            "code": "sampling_dropped",
            "message": "field is not supported by target format",
            "severity": "warning",
            "details": {"field": "frequency_penalty", "value": 0.2},
        },
    ]
