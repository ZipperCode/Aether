#!/usr/bin/env python
"""
生成安全密钥
"""

import secrets


def main() -> None:
    # 生成JWT密钥
    jwt_key = secrets.token_urlsafe(32)

    # 数据库密码
    db_password_key = secrets.token_urlsafe(24)

    # 生成独立的加密密钥
    encryption_key = secrets.token_urlsafe(32)

    # 生成 Redis 密码
    redis_password = secrets.token_urlsafe(32)

    print("\n将以下内容添加到 .env 文件：\n")
    print(f"JWT_SECRET_KEY={jwt_key}")
    print(f"DB_PASSWORD={db_password_key}")
    print(f"ENCRYPTION_KEY={encryption_key}")
    print(f"REDIS_PASSWORD={redis_password}")
    print()
    print("注意:")
    print("  - JWT_SECRET_KEY 用于用户登录 token 签名")
    print("  - DB_PASSWORD 用于用户登录数据库")
    print("  - ENCRYPTION_KEY 用于敏感数据加密（如 Provider API Keys）")
    print("  - REDIS_PASSWORD 用于 Redis 连接认证（并发控制）")
    print("  - 这些密钥应该独立设置，避免相互耦合")
    print()


if __name__ == "__main__":
    main()
