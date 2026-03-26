from __future__ import annotations

from src.core.api_format.conversion.internal import InternalError, InternalRequest, InternalResponse
from src.core.api_format.conversion.stream_events import InternalStreamEvent
from src.core.api_format.transformers.base import TransformContext


class MaxTokenTransformer:
    NAME = "maxtoken"

    def transform_request(
        self,
        internal: InternalRequest,
        ctx: TransformContext,
    ) -> InternalRequest:
        output_limit = internal.output_limit
        if output_limit is None or output_limit <= 0:
            return internal

        if internal.max_tokens is None or internal.max_tokens <= 0:
            internal.max_tokens = output_limit
            ctx.add_diagnostic(
                code="max_tokens_filled",
                message="max_tokens was filled from output_limit",
                severity="info",
                details={"output_limit": output_limit},
                transformer=self.NAME,
            )
            return internal

        if internal.max_tokens > output_limit:
            original_value = internal.max_tokens
            internal.max_tokens = output_limit
            ctx.add_diagnostic(
                code="max_tokens_clamped",
                message="max_tokens was clamped to output_limit",
                severity="warning",
                details={"value": original_value, "output_limit": output_limit},
                transformer=self.NAME,
            )
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
