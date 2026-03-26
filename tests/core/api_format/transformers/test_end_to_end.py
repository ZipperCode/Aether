from __future__ import annotations

from src.core.api_format.transformers.base import TransformContext
from src.core.api_format.transformers.runtime import apply_request_transformers


def test_end_to_end_openai_chat_to_claude_with_default_transformers() -> None:
    out = apply_request_transformers(
        request_body={
            "model": "gpt-5",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 1.8,
            "tool_choice": {"type": "function", "function": {"name": "get_weather"}},
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "description": "Get weather",
                        "parameters": {"type": "object", "properties": {"city": {"type": "string"}}},
                    },
                }
            ],
            "reasoning_effort": "high",
        },
        source_format="openai:chat",
        target_format="claude:chat",
        specs=[
            {"name": "tooluse"},
            {"name": "enhancetool"},
            {"name": "reasoning"},
            {"name": "sampling"},
            {"name": "maxtoken"},
        ],
        context=TransformContext(stage="request", client_format="openai:chat", provider_format="claude:chat"),
        output_limit=4096,
    )

    assert out["temperature"] == 1.0
    assert out["tool_choice"] == {"type": "tool", "name": "get_weather"}
    assert out["thinking"]["type"] == "enabled"
    assert out["thinking"]["budget_tokens"] >= 4096
    assert out["max_tokens"] == out["thinking"]["budget_tokens"] + 1


def test_end_to_end_openai_chat_to_gemini_with_default_transformers() -> None:
    out = apply_request_transformers(
        request_body={
            "model": "gpt-5",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 1.7,
            "reasoning_effort": "medium",
        },
        source_format="openai:chat",
        target_format="gemini:chat",
        specs=[
            {"name": "tooluse"},
            {"name": "enhancetool"},
            {"name": "reasoning"},
            {"name": "sampling"},
            {"name": "maxtoken"},
        ],
        context=TransformContext(stage="request", client_format="openai:chat", provider_format="gemini:chat"),
        output_limit=2048,
    )

    generation_config = out["generation_config"]
    assert generation_config["temperature"] == 1.0
    assert generation_config["max_output_tokens"] == 2048
    assert generation_config["thinkingConfig"]["thinkingBudget"] == 2048


def test_end_to_end_claude_same_format_clears_cache_control() -> None:
    out = apply_request_transformers(
        request_body={
            "model": "claude-3-7-sonnet",
            "system": [
                {
                    "type": "text",
                    "text": "system prompt",
                    "cache_control": {"type": "ephemeral"},
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": "hello",
                            "cache_control": {"type": "ephemeral"},
                        }
                    ],
                }
            ],
            "max_tokens": 1024,
        },
        source_format="claude:chat",
        target_format="claude:chat",
        specs=[{"name": "cleancache"}],
        context=TransformContext(stage="request", client_format="claude:chat", provider_format="claude:chat"),
    )

    assert out["system"] == "system prompt"
    assert out["messages"][0]["content"] == "hello"


def test_end_to_end_openai_cli_to_claude_with_default_transformers() -> None:
    out = apply_request_transformers(
        request_body={
            "model": "gpt-5",
            "instructions": "system prompt",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "weather?"}]},
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_weather",
                    "arguments": '{"city":"SF"}',
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": '{"temp_c":20}',
                },
            ],
            "tool_choice": {"type": "function", "name": "get_weather"},
            "reasoning": {"effort": "medium"},
        },
        source_format="openai:cli",
        target_format="claude:chat",
        specs=[
            {"name": "tooluse"},
            {"name": "enhancetool"},
            {"name": "reasoning"},
            {"name": "sampling"},
            {"name": "maxtoken"},
        ],
        context=TransformContext(stage="request", client_format="openai:cli", provider_format="claude:chat"),
        output_limit=2048,
    )

    assert out["system"] == "system prompt"
    assert out["tool_choice"] == {"type": "tool", "name": "get_weather"}
    assert out["thinking"]["type"] == "enabled"
    assert out["messages"][0]["role"] == "user"
    assert out["messages"][1]["role"] == "assistant"
    assert out["messages"][1]["content"][0]["type"] == "tool_use"
    assert out["messages"][1]["content"][0]["input"] == {"city": "SF"}
    assert out["messages"][2]["content"][0]["type"] == "tool_result"


def test_end_to_end_openai_cli_to_gemini_with_default_transformers() -> None:
    out = apply_request_transformers(
        request_body={
            "model": "gpt-5",
            "instructions": "system prompt",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "weather?"}]},
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_weather",
                    "arguments": '{"city":"SF"}',
                },
            ],
            "reasoning": {"effort": "medium"},
        },
        source_format="openai:cli",
        target_format="gemini:chat",
        specs=[
            {"name": "tooluse"},
            {"name": "enhancetool"},
            {"name": "reasoning"},
            {"name": "sampling"},
            {"name": "maxtoken"},
        ],
        context=TransformContext(stage="request", client_format="openai:cli", provider_format="gemini:chat"),
        output_limit=1024,
    )

    assert out["system_instruction"]["parts"][0]["text"] == "system prompt"
    assert out["generation_config"]["max_output_tokens"] == 1024
    assert out["generation_config"]["thinkingConfig"]["thinkingBudget"] == 2048
    assert out["contents"][0]["role"] == "user"
    assert out["contents"][1]["role"] == "model"
    assert out["contents"][1]["parts"][0]["function_call"]["name"] == "get_weather"
