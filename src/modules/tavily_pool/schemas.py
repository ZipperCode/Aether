"""Tavily Pool 数据传输模型。"""

from __future__ import annotations

from datetime import datetime

from pydantic import BaseModel


class CreateAccountRequest(BaseModel):
    email: str
    password: str
    source: str = "manual"
    notes: str | None = None


class CreateTokenRequest(BaseModel):
    token: str


class TavilyAccountRead(BaseModel):
    id: str
    email: str
    status: str
    source: str
    notes: str | None
    created_at: datetime
    updated_at: datetime


class TavilyTokenRead(BaseModel):
    id: str
    account_id: str
    token_masked: str
    is_active: bool
    created_at: datetime
    updated_at: datetime
