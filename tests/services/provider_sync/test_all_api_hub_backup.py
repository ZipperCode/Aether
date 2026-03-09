from src.services.provider_sync.all_api_hub_backup import parse_all_api_hub_accounts


def test_extract_accounts_from_backup_v2_and_match_domain() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://anyrouter.top/path",
                    "cookieAuth": {"sessionCookie": "session=abc"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)

    assert len(accounts) == 1
    assert accounts[0].domain == "anyrouter.top"
    assert accounts[0].session_cookie == "session=abc"


def test_extract_accounts_from_new_access_token_structure() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "token-xyz"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].domain == "example.com"
    assert accounts[0].session_cookie == "token-xyz"
    assert accounts[0].auth_type == "access_token"


def test_extract_user_id_from_access_token_account_info() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "account_info": {
                        "access_token": "token-xyz",
                        "user_id": "user-1001",
                    },
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].user_id == "user-1001"


def test_does_not_use_username_as_new_api_user_id() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "token-xyz", "username": "alice"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].user_id is None


def test_does_not_use_top_level_account_id_as_user_id() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "id": "rec-12345",
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "account_info": {"access_token": "token-xyz"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].user_id is None


def test_extract_cookie_value_when_access_token_account_also_has_cookie_auth() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "cookieAuth": {"sessionCookie": "session=from-cookie"},
                    "account_info": {"access_token": "token-xyz"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].cookie_value == "session=from-cookie"
    assert accounts[0].session_cookie == "session=from-cookie"


def test_extract_cookie_value_from_cookie_auth_cookie_field() -> None:
    raw = {
        "version": "2.0",
        "accounts": {
            "accounts": [
                {
                    "site_url": "https://example.com",
                    "authType": "access_token",
                    "cookieAuth": {"cookie": "session=from-cookie-key"},
                    "account_info": {"access_token": "token-xyz"},
                }
            ]
        },
    }

    accounts = parse_all_api_hub_accounts(raw)
    assert len(accounts) == 1
    assert accounts[0].cookie_value == "session=from-cookie-key"
