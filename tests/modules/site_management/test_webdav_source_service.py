"""Tests for WebDavSourceService."""
from __future__ import annotations

from unittest.mock import MagicMock, patch, call

import pytest

from src.modules.site_management.services.webdav_source_service import WebDavSourceService


@pytest.fixture
def mock_db():
    db = MagicMock()
    return db


@pytest.fixture
def service(mock_db):
    with patch(
        "src.modules.site_management.services.webdav_source_service.CryptoService"
    ) as MockCrypto:
        instance = MockCrypto.return_value
        instance.encrypt.side_effect = lambda x: f"enc_{x}"
        instance.decrypt.side_effect = lambda x: x.replace("enc_", "")
        svc = WebDavSourceService(mock_db)
        yield svc


def test_create_source_encrypts_password(service, mock_db):
    with patch(
        "src.modules.site_management.services.webdav_source_service.WebDavSource"
    ) as MockModel:
        mock_instance = MagicMock()
        MockModel.return_value = mock_instance

        result = service.create(
            name="Test", url="https://dav.example.com", username="user", password="secret"
        )

        # Verify model was constructed with encrypted password
        MockModel.assert_called_once()
        kwargs = MockModel.call_args[1]
        assert kwargs["password"] == "enc_secret"
        assert kwargs["name"] == "Test"
        assert kwargs["url"] == "https://dav.example.com"
        assert kwargs["username"] == "user"
        assert kwargs["id"] is not None

        mock_db.add.assert_called_once_with(mock_instance)
        mock_db.flush.assert_called_once()
        assert result is mock_instance


def test_create_source_with_checkin_config(service, mock_db):
    with patch(
        "src.modules.site_management.services.webdav_source_service.WebDavSource"
    ) as MockModel:
        MockModel.return_value = MagicMock()

        service.create(
            name="Test",
            url="https://dav.example.com",
            username="user",
            password="secret",
            checkin_enabled=False,
            checkin_time="08:30",
        )

        kwargs = MockModel.call_args[1]
        assert kwargs["checkin_enabled"] is False
        assert kwargs["checkin_time"] == "08:30"


def test_create_source_generates_uuid(service, mock_db):
    with patch(
        "src.modules.site_management.services.webdav_source_service.WebDavSource"
    ) as MockModel:
        MockModel.return_value = MagicMock()
        service.create(
            name="Test", url="https://dav.example.com", username="user", password="secret"
        )
        kwargs = MockModel.call_args[1]
        assert kwargs["id"] is not None
        assert len(kwargs["id"]) == 36  # UUID format


def test_create_source_invalid_checkin_time_raises(service):
    with pytest.raises(ValueError, match="checkin_time"):
        service.create(
            name="Test",
            url="https://dav.example.com",
            username="user",
            password="secret",
            checkin_enabled=True,
            checkin_time="25:99",
        )


def test_get_source(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source
    result = service.get("source-1")
    assert result is fake_source


def test_get_source_not_found(service, mock_db):
    mock_db.query.return_value.filter.return_value.first.return_value = None
    result = service.get("nonexistent")
    assert result is None


def test_list_sources_default(service, mock_db):
    mock_db.query.return_value.order_by.return_value.all.return_value = []
    result = service.list_all()
    assert result == []


def test_list_sources_active_only(service, mock_db):
    mock_db.query.return_value.filter.return_value.order_by.return_value.all.return_value = []
    result = service.list_all(active_only=True)
    mock_db.query.return_value.filter.assert_called_once()


def test_update_source_reencrypts_password(service, mock_db):
    fake_source = MagicMock()
    fake_source.name = "Old"
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source
    service.update("source-1", name="New", password="newsecret")
    # Check password was encrypted
    service.crypto.encrypt.assert_called_with("newsecret")
    mock_db.flush.assert_called()


def test_update_source_supports_checkin_config(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source

    service.update("source-1", checkin_enabled=False, checkin_time="07:45")

    assert fake_source.checkin_enabled is False
    assert fake_source.checkin_time == "07:45"
    mock_db.flush.assert_called()


def test_update_source_invalid_checkin_time_raises(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source

    with pytest.raises(ValueError, match="checkin_time"):
        service.update("source-1", checkin_time="99:99")


def test_update_source_without_password(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source
    service.update("source-1", name="NewName")
    # encrypt should not be called when password is not in kwargs
    service.crypto.encrypt.assert_not_called()
    mock_db.flush.assert_called()


def test_update_nonexistent_source(service, mock_db):
    mock_db.query.return_value.filter.return_value.first.return_value = None
    result = service.update("nonexistent", name="New")
    assert result is None


def test_delete_source(service, mock_db):
    fake_source = MagicMock()
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source
    result = service.delete("source-1")
    assert result is True
    mock_db.delete.assert_called_once_with(fake_source)
    mock_db.flush.assert_called()


def test_delete_nonexistent(service, mock_db):
    mock_db.query.return_value.filter.return_value.first.return_value = None
    result = service.delete("nonexistent")
    assert result is False
    mock_db.delete.assert_not_called()


def test_get_decrypted_password(service):
    source = MagicMock()
    source.password = "enc_mypass"
    result = service.get_decrypted_password(source)
    assert result == "mypass"


@pytest.mark.asyncio
async def test_test_connection_source_not_found(service, mock_db):
    mock_db.query.return_value.filter.return_value.first.return_value = None
    success, msg = await service.test_connection("nonexistent")
    assert success is False
    assert msg == "Source not found"


@pytest.mark.asyncio
async def test_test_connection_success(service, mock_db):
    fake_source = MagicMock()
    fake_source.url = "https://dav.example.com"
    fake_source.username = "user"
    fake_source.password = "enc_secret"
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source

    with patch(
        "src.modules.site_management.webdav_client.download_backup_with_meta"
    ) as mock_download:
        mock_download.return_value = MagicMock()
        success, msg = await service.test_connection("source-1")
        assert success is True
        assert msg == "Connection successful"
        mock_download.assert_called_once_with("https://dav.example.com", "user", "secret")


@pytest.mark.asyncio
async def test_test_connection_failure(service, mock_db):
    fake_source = MagicMock()
    fake_source.url = "https://dav.example.com"
    fake_source.username = "user"
    fake_source.password = "enc_secret"
    mock_db.query.return_value.filter.return_value.first.return_value = fake_source

    with patch(
        "src.modules.site_management.webdav_client.download_backup_with_meta"
    ) as mock_download:
        mock_download.side_effect = ValueError("webdav auth failed")
        success, msg = await service.test_connection("source-1")
        assert success is False
        assert msg == "webdav auth failed"
