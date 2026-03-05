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
