"""Search Pool Gateway schemas."""

from __future__ import annotations

from pydantic import BaseModel, Field


class CreateKeyRequest(BaseModel):
    service: str
    key: str
    email: str = ""


class ImportKeysRequest(BaseModel):
    service: str
    content: str = ""
    keys: list[str] = Field(default_factory=list)


class ToggleKeyRequest(BaseModel):
    active: bool = True


class CreateTokenRequest(BaseModel):
    service: str
    name: str = ""
    hourly_limit: int = Field(default=0, ge=0)
    daily_limit: int = Field(default=0, ge=0)
    monthly_limit: int = Field(default=0, ge=0)


class UpdateTokenRequest(BaseModel):
    name: str = ""
    hourly_limit: int = Field(default=0, ge=0)
    daily_limit: int = Field(default=0, ge=0)
    monthly_limit: int = Field(default=0, ge=0)


class UsageSyncRequest(BaseModel):
    service: str | None = None
    force: bool = False
