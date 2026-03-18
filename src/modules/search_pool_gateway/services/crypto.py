"""Crypto helpers for search pool gateway."""

from __future__ import annotations

import base64
import hashlib
import os


def _derive_mask(raw: str) -> str:
    if len(raw) <= 12:
        return raw
    return f"{raw[:8]}***{raw[-4:]}"


class GatewayCryptoService:
    def __init__(self) -> None:
        key = os.getenv("SEARCH_POOL_GATEWAY_CRYPTO_KEY", "search-pool-gateway-dev-key")
        self._salt = hashlib.sha256(key.encode("utf-8")).digest()

    def encrypt(self, plaintext: str) -> str:
        data = plaintext.encode("utf-8")
        out = bytes(b ^ self._salt[i % len(self._salt)] for i, b in enumerate(data))
        return base64.urlsafe_b64encode(out).decode("ascii")

    def decrypt(self, ciphertext: str) -> str:
        data = base64.urlsafe_b64decode(ciphertext.encode("ascii"))
        out = bytes(b ^ self._salt[i % len(self._salt)] for i, b in enumerate(data))
        return out.decode("utf-8")


def mask_key(raw: str) -> str:
    return _derive_mask(raw)
