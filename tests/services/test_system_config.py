from src.services.system.config import SystemConfigService


def test_legacy_all_api_hub_keys_not_in_default_configs() -> None:
    legacy_keys = {
        "enable_all_api_hub_sync",
        "all_api_hub_sync_time",
        "all_api_hub_webdav_url",
        "all_api_hub_webdav_username",
        "all_api_hub_webdav_password",
        "enable_all_api_hub_auto_create_provider_ops",
    }

    assert legacy_keys.isdisjoint(SystemConfigService.DEFAULT_CONFIGS)


def test_legacy_all_api_hub_password_not_marked_sensitive() -> None:
    assert "all_api_hub_webdav_password" not in SystemConfigService.SENSITIVE_KEYS
