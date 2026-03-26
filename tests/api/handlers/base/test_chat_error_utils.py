from __future__ import annotations

from src.api.handlers.base.chat_error_utils import _build_error_json_payload
from src.core.api_format.transformers.registry import get_transformer_registry
from src.core.api_format.transformers.base import TransformContext
from src.core.exceptions import UpstreamClientException


class _AppendErrorMessageTransformer:
    NAME = "test_append_error_message"

    def transform_request(self, internal, ctx: TransformContext):
        return internal

    def transform_response(self, internal, ctx: TransformContext):
        return internal

    def transform_stream_event(self, event, ctx: TransformContext):
        return [event]

    def transform_error(self, internal, ctx: TransformContext):
        internal.message += "|error"
        return internal


class _DiagnosticErrorTransformer:
    NAME = "test_error_diagnostic"

    def transform_request(self, internal, ctx: TransformContext):
        return internal

    def transform_response(self, internal, ctx: TransformContext):
        return internal

    def transform_stream_event(self, event, ctx: TransformContext):
        return [event]

    def transform_error(self, internal, ctx: TransformContext):
        ctx.add_diagnostic(
            code="error_adjusted",
            message="error adjusted",
            severity="warning",
            transformer=self.NAME,
        )
        return internal


def test_build_error_json_payload_applies_transformers_when_provided() -> None:
    get_transformer_registry().register(_AppendErrorMessageTransformer)

    exc = UpstreamClientException(
        message="upstream failed",
        provider_name="provider-x",
        status_code=400,
        upstream_error='{"error":{"message":"bad request","type":"invalid_request_error"}}',
    )

    payload = _build_error_json_payload(
        exc,
        client_format="claude:chat",
        provider_format="openai:chat",
        needs_conversion=True,
        transformer_specs=[{"name": "test_append_error_message"}],
        transform_context=TransformContext(stage="error", client_format="claude:chat"),
    )

    assert payload["error"]["message"] == "bad request|error"


def test_build_error_json_payload_collects_transformer_diagnostics_on_context() -> None:
    get_transformer_registry().register(_DiagnosticErrorTransformer)

    exc = UpstreamClientException(
        message="upstream failed",
        provider_name="provider-x",
        status_code=400,
        upstream_error='{"error":{"message":"bad request","type":"invalid_request_error"}}',
    )
    ctx = TransformContext(stage="error", client_format="claude:chat")

    _build_error_json_payload(
        exc,
        client_format="claude:chat",
        provider_format="openai:chat",
        needs_conversion=True,
        transformer_specs=[{"name": "test_error_diagnostic"}],
        transform_context=ctx,
    )

    assert ctx.diagnostics == [
        {
            "stage": "error",
            "transformer": "test_error_diagnostic",
            "code": "error_adjusted",
            "message": "error adjusted",
            "severity": "warning",
        }
    ]
