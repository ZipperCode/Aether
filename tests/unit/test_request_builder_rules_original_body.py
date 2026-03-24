from types import SimpleNamespace

from src.api.handlers.base.request_builder import PassthroughRequestBuilder


def test_passthrough_request_builder_rules_original_body_drives_conditions() -> None:
    """
    When model mapping mutates the outbound body, body/header rules should still be
    able to evaluate conditions against the pre-mapping payload via `source=original`.
    """
    builder = PassthroughRequestBuilder()

    endpoint = SimpleNamespace(
        api_family="openai",
        endpoint_kind="chat",
        body_rules=[
            {
                "action": "set",
                "path": "route",
                "value": "by-original-model",
                "condition": {
                    "source": "original",
                    "path": "model",
                    "op": "eq",
                    "value": "client-model",
                },
            }
        ],
        header_rules=[
            {
                "action": "set",
                "key": "X-Model-Source",
                "value": "client",
                "condition": {
                    "source": "original",
                    "path": "model",
                    "op": "eq",
                    "value": "client-model",
                },
            }
        ],
    )
    key = SimpleNamespace(api_key="unused")

    mapped_body = {"model": "provider-model", "messages": []}
    original_body = {"model": "client-model", "messages": []}

    payload, headers = builder.build(
        mapped_body,
        {},
        endpoint,
        key,
        rules_original_body=original_body,
        pre_computed_auth=("Authorization", "Bearer upstream-token"),
    )

    assert payload["model"] == "provider-model"
    assert payload["route"] == "by-original-model"
    assert headers["X-Model-Source"] == "client"
    assert headers["Authorization"] == "Bearer upstream-token"
