from __future__ import annotations

from src.core.api_format.conversion.internal import (
    InternalMessage,
    InternalRequest,
    Role,
    TextBlock,
    ToolResultBlock,
    ToolUseBlock,
)
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.tooluse import ToolUseTransformer


def test_tooluse_transformer_generates_missing_tool_use_ids() -> None:
    transformer = ToolUseTransformer()
    internal = InternalRequest(
        model="gpt-4o-mini",
        messages=[
            InternalMessage(role=Role.USER, content=[TextBlock(text="weather?")]),
            InternalMessage(
                role=Role.ASSISTANT,
                content=[ToolUseBlock(tool_id="", tool_name="get_weather", tool_input={"city": "SF"})],
            ),
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat"),
    )

    tool_use = out.messages[1].content[0]
    assert isinstance(tool_use, ToolUseBlock)
    assert tool_use.tool_id.startswith("call_auto_")


def test_tooluse_transformer_links_tool_result_to_latest_tool_use_when_missing() -> None:
    transformer = ToolUseTransformer()
    internal = InternalRequest(
        model="gpt-4o-mini",
        messages=[
            InternalMessage(role=Role.USER, content=[TextBlock(text="weather?")]),
            InternalMessage(
                role=Role.ASSISTANT,
                content=[ToolUseBlock(tool_id="call_1", tool_name="get_weather", tool_input={"city": "SF"})],
            ),
            InternalMessage(
                role=Role.USER,
                content=[ToolResultBlock(tool_use_id="", output={"temp_c": 20})],
            ),
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat"),
    )

    tool_result = out.messages[2].content[0]
    assert isinstance(tool_result, ToolResultBlock)
    assert tool_result.tool_use_id == "call_1"


def test_tooluse_transformer_records_diagnostics_for_generated_and_linked_ids() -> None:
    transformer = ToolUseTransformer()
    internal = InternalRequest(
        model="gpt-4o-mini",
        messages=[
            InternalMessage(role=Role.USER, content=[TextBlock(text="weather?")]),
            InternalMessage(
                role=Role.ASSISTANT,
                content=[ToolUseBlock(tool_id="", tool_name="get_weather", tool_input={"city": "SF"})],
            ),
            InternalMessage(
                role=Role.USER,
                content=[ToolResultBlock(tool_use_id="", output={"temp_c": 20})],
            ),
        ],
    )
    ctx = TransformContext(stage="request", client_format="openai:chat")

    out = transformer.transform_request(internal, ctx)

    generated_tool_use = out.messages[1].content[0]
    tool_result = out.messages[2].content[0]
    assert isinstance(generated_tool_use, ToolUseBlock)
    assert isinstance(tool_result, ToolResultBlock)
    assert tool_result.tool_use_id == generated_tool_use.tool_id
    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "tooluse",
            "code": "tool_id_generated",
            "message": "missing tool_use id was generated",
            "severity": "info",
            "details": {"tool_name": "get_weather", "tool_id": generated_tool_use.tool_id},
        },
        {
            "stage": "request",
            "transformer": "tooluse",
            "code": "tool_result_linked",
            "message": "tool_result was linked to the latest pending tool_use",
            "severity": "warning",
            "details": {"tool_use_id": generated_tool_use.tool_id},
        },
    ]
