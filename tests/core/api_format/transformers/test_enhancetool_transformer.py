from __future__ import annotations

from src.core.api_format.conversion.internal import (
    InternalMessage,
    InternalRequest,
    Role,
    ToolResultBlock,
    ToolUseBlock,
)
from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.builtin.enhancetool import EnhanceToolTransformer


def test_enhancetool_transformer_parses_raw_json_arguments() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.ASSISTANT,
                content=[
                    ToolUseBlock(
                        tool_id="call_1",
                        tool_name="get_weather",
                        tool_input={},
                        extra={"raw": {"arguments": '{"city":"SF"}'}},
                    )
                ],
            )
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    tool_use = out.messages[0].content[0]
    assert isinstance(tool_use, ToolUseBlock)
    assert tool_use.tool_input == {"city": "SF"}


def test_enhancetool_transformer_falls_back_to_empty_dict_for_invalid_json_arguments() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.ASSISTANT,
                content=[
                    ToolUseBlock(
                        tool_id="call_1",
                        tool_name="get_weather",
                        tool_input={},
                        extra={"raw": {"arguments": '{"city": }'}},
                    )
                ],
            )
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    tool_use = out.messages[0].content[0]
    assert isinstance(tool_use, ToolUseBlock)
    assert tool_use.tool_input == {}


def test_enhancetool_transformer_generates_missing_tool_name() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.ASSISTANT,
                content=[ToolUseBlock(tool_id="call_1", tool_name="", tool_input={})],
            )
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
    )

    tool_use = out.messages[0].content[0]
    assert isinstance(tool_use, ToolUseBlock)
    assert tool_use.tool_name == "tool_1"


def test_enhancetool_transformer_backfills_tool_result_name_from_tool_use() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.ASSISTANT,
                content=[ToolUseBlock(tool_id="call_1", tool_name="get_weather", tool_input={})],
            ),
            InternalMessage(
                role=Role.USER,
                content=[ToolResultBlock(tool_use_id="call_1", tool_name=None, output={"temp_c": 20})],
            ),
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="gemini:generate"),
    )

    tool_result = out.messages[1].content[0]
    assert isinstance(tool_result, ToolResultBlock)
    assert tool_result.tool_name == "get_weather"


def test_enhancetool_transformer_normalizes_empty_tool_result_to_empty_text() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.USER,
                content=[ToolResultBlock(tool_use_id="call_1", tool_name=None, output=None, content_text=None)],
            ),
        ],
    )

    out = transformer.transform_request(
        internal,
        TransformContext(stage="request", client_format="openai:chat", provider_format="openai:chat"),
    )

    tool_result = out.messages[0].content[0]
    assert isinstance(tool_result, ToolResultBlock)
    assert tool_result.content_text == ""


def test_enhancetool_transformer_records_diagnostics_for_repairs() -> None:
    transformer = EnhanceToolTransformer()
    internal = InternalRequest(
        model="gpt-5",
        messages=[
            InternalMessage(
                role=Role.ASSISTANT,
                content=[
                    ToolUseBlock(
                        tool_id="call_1",
                        tool_name="",
                        tool_input={},
                        extra={"raw": {"arguments": '{"city": }'}},
                    )
                ],
            ),
            InternalMessage(
                role=Role.USER,
                content=[ToolResultBlock(tool_use_id="call_1", tool_name=None, output=None, content_text=None)],
            ),
        ],
    )
    ctx = TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat")

    out = transformer.transform_request(internal, ctx)

    tool_use = out.messages[0].content[0]
    tool_result = out.messages[1].content[0]
    assert isinstance(tool_use, ToolUseBlock)
    assert isinstance(tool_result, ToolResultBlock)
    assert tool_use.tool_name == "tool_1"
    assert tool_use.tool_input == {}
    assert tool_result.tool_name == "tool_1"
    assert tool_result.content_text == ""
    assert ctx.diagnostics == [
        {
            "stage": "request",
            "transformer": "enhancetool",
            "code": "tool_name_generated",
            "message": "missing tool name was generated",
            "severity": "warning",
            "details": {"tool_id": "call_1", "tool_name": "tool_1"},
        },
        {
            "stage": "request",
            "transformer": "enhancetool",
            "code": "tool_arguments_repaired",
            "message": "invalid tool arguments JSON was replaced with an empty object",
            "severity": "warning",
            "details": {"tool_id": "call_1"},
        },
        {
            "stage": "request",
            "transformer": "enhancetool",
            "code": "tool_result_name_filled",
            "message": "tool_result name was backfilled from tool_use",
            "severity": "info",
            "details": {"tool_use_id": "call_1", "tool_name": "tool_1"},
        },
        {
            "stage": "request",
            "transformer": "enhancetool",
            "code": "tool_result_normalized",
            "message": "empty tool_result was normalized to empty text",
            "severity": "info",
            "details": {"tool_use_id": "call_1"},
        },
    ]
