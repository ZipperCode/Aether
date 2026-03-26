from __future__ import annotations

from src.core.api_format.conversion.internal import InternalError, InternalRequest, InternalResponse
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext


def _clamp(value: float | None, *, min_value: float, max_value: float) -> float | None:
    if value is None:
        return None
    if value < min_value:
        return min_value
    if value > max_value:
        return max_value
    return value


class SamplingTransformer:
    NAME = "sampling"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        provider_format = str(ctx.provider_format or "").strip().lower()

        if provider_format.startswith(("claude:", "gemini:")):
            original_temperature = internal.temperature
            internal.temperature = _clamp(internal.temperature, min_value=0.0, max_value=1.0)
            if internal.temperature != original_temperature and original_temperature is not None:
                ctx.add_diagnostic(
                    code="sampling_clamped",
                    message="temperature was clamped to target-supported range",
                    severity="info",
                    details={
                        "field": "temperature",
                        "value": original_temperature,
                        "clamped_to": internal.temperature,
                    },
                    transformer=self.NAME,
                )

            original_top_p = internal.top_p
            internal.top_p = _clamp(internal.top_p, min_value=0.0, max_value=1.0)
            if internal.top_p != original_top_p and original_top_p is not None:
                ctx.add_diagnostic(
                    code="sampling_clamped",
                    message="top_p was clamped to target-supported range",
                    severity="info",
                    details={
                        "field": "top_p",
                        "value": original_top_p,
                        "clamped_to": internal.top_p,
                    },
                    transformer=self.NAME,
                )

        if provider_format.startswith("claude:"):
            for field_name in (
                "n",
                "presence_penalty",
                "frequency_penalty",
                "logprobs",
                "top_logprobs",
                "seed",
            ):
                value = getattr(internal, field_name)
                if value is not None:
                    ctx.add_diagnostic(
                        code="sampling_dropped",
                        message="field is not supported by target format",
                        severity="warning",
                        details={"field": field_name, "value": value},
                        transformer=self.NAME,
                    )
                    setattr(internal, field_name, None)

        return internal

    def transform_response(
        self,
        internal: InternalResponse,
        ctx: TransformContext,
    ) -> InternalResponse:
        return internal

    def transform_stream_event(
        self,
        event: InternalStreamEvent,
        ctx: TransformContext,
    ) -> list[InternalStreamEvent]:
        return [event]

    def transform_error(
        self,
        internal: InternalError,
        ctx: TransformContext,
    ) -> InternalError:
        return internal
