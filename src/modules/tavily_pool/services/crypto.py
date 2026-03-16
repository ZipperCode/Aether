"""Tavily Pool 独立加密工具。"""

from __future__ import annotations

import base64
import hashlib
import os

from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

DEFAULT_CRYPTO_KEY = "tavily-pool-dev-key"
APP_SALT = hashlib.sha256(b"aether-tavily-pool-v1").digest()[:16]


def _build_fernet_key(raw_key: str) -> bytes:
    try:
        key_bytes = raw_key.encode()
        Fernet(key_bytes)
        return key_bytes
    except Exception:
        pass

    kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=APP_SALT, iterations=100000)
    return base64.urlsafe_b64encode(kdf.derive(raw_key.encode()))


class TavilyCryptoService:
    def __init__(self) -> None:
        key = os.getenv("TAVILY_POOL_CRYPTO_KEY", DEFAULT_CRYPTO_KEY)
        self._cipher = Fernet(_build_fernet_key(key))

    def encrypt(self, plaintext: str) -> str:
        encrypted = self._cipher.encrypt(plaintext.encode())
        return base64.urlsafe_b64encode(encrypted).decode()

    def decrypt(self, ciphertext: str) -> str:
        encrypted = base64.urlsafe_b64decode(ciphertext.encode())
        return self._cipher.decrypt(encrypted).decode()


def mask_token(raw_token: str) -> str:
    token = raw_token.strip()
    if len(token) <= 8:
        return f"{token[:2]}***{token[-2:]}" if len(token) >= 4 else "***"
    return f"{token[:4]}***{token[-4:]}"
