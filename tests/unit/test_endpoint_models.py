import pytest
from pydantic import ValidationError

from src.models.endpoint_models import ProviderEndpointCreate, ProviderEndpointUpdate


def test_provider_endpoint_models_accept_nested_conditions_and_source() -> None:
    payload = {
        "provider_id": "provider-1",
        "api_format": "openai:chat",
        "base_url": "https://api.example.com",
        "header_rules": [
            {
                "action": "set",
                "key": "X-Test",
                "value": "1",
                "condition": {
                    "all": [
                        {"path": "mode", "op": "eq", "value": "prod", "source": "original"},
                        {
                            "any": [
                                {"path": "tier", "op": "eq", "value": "gold"},
                                {"path": "tier", "op": "eq", "value": "silver"},
                            ]
                        },
                    ]
                },
            }
        ],
        "body_rules": [
            {
                "action": "set",
                "path": "metadata.enabled",
                "value": True,
                "condition": {
                    "all": [
                        {"path": "metadata.kind", "op": "eq", "value": "chat"},
                        {"path": "metadata.tags", "op": "contains", "value": "vip"},
                    ]
                },
            }
        ],
    }

    created = ProviderEndpointCreate(**payload)
    updated = ProviderEndpointUpdate(
        header_rules=payload["header_rules"],
        body_rules=payload["body_rules"],
    )

    assert created.header_rules == payload["header_rules"]
    assert created.body_rules == payload["body_rules"]
    assert updated.header_rules == payload["header_rules"]
    assert updated.body_rules == payload["body_rules"]


@pytest.mark.parametrize(
    ("field_name", "rules"),
    [
        (
            "header_rules",
            [
                {
                    "action": "set",
                    "key": "X-Test",
                    "value": "1",
                    "condition": {"path": "mode", "op": "eq", "value": "prod", "source": "bad"},
                }
            ],
        ),
        (
            "body_rules",
            [
                {
                    "action": "set",
                    "path": "metadata.enabled",
                    "value": True,
                    "condition": {"all": []},
                }
            ],
        ),
    ],
)
def test_provider_endpoint_models_reject_invalid_condition_shapes(
    field_name: str,
    rules: list[dict],
) -> None:
    with pytest.raises(ValidationError):
        ProviderEndpointCreate(
            provider_id="provider-1",
            api_format="openai:chat",
            base_url="https://api.example.com",
            **{field_name: rules},
        )
