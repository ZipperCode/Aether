#!/usr/bin/env python3
"""统一本地和 CI 的网关检查命令；不安装工具、不读取 .env 或启动服务。"""

import argparse
import json
import os
from pathlib import Path
import subprocess

# 固定从仓库根目录执行，允许本地调用方位于其他目录。
REPO_ROOT = Path(__file__).resolve().parent.parent
# 各阶段保留现有目标覆盖；网关同时收集 lib/bins 的失败并使用已锁定依赖。
COMMANDS = {
    "fmt": ["cargo", "fmt", "--all", "--check"],
    "clippy-gateway": [
        "cargo", "clippy", "-p", "aether-gateway", "--lib", "--bins",
        "--examples", "--", "-D", "warnings",
    ],
    "gateway": [
        "cargo", "nextest", "run", "-p", "aether-gateway", "--lib", "--bins",
        "--no-fail-fast", "--locked",
    ],
}
# 仅覆盖子进程的非敏感编译设置，与工作流缓存恢复时的 profile 保持一致。
BUILD_ENV = {
    "CARGO_INCREMENTAL": "0",  # 禁用增量编译，与 CI 和 sccache 合同一致。
    "CARGO_PROFILE_DEV_DEBUG": "0",  # dev 构建不输出调试信息。
    "CARGO_PROFILE_TEST_DEBUG": "0",  # test 构建不输出调试信息。
}


def main(argv=None):
    """解析阶段与 dry-run；逐阶段执行，首个失败退出码原样返回，不自动重试。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=[*COMMANDS, "preflight"], help="检查阶段")
    parser.add_argument("--dry-run", action="store_true", help="仅打印命令和覆盖环境，不执行")
    args = parser.parse_args(argv)
    stages = COMMANDS if args.stage == "preflight" else [args.stage]
    for stage in stages:
        overrides = dict(BUILD_ENV)
        if stage == "gateway":
            # 网关测试线程栈为 16 MiB；linker 和 sccache 仍继承调用环境。
            overrides["RUST_MIN_STACK"] = "16777216"
        command = COMMANDS[stage]
        print(json.dumps({"stage": stage, "command": command, "env": overrides}), flush=True)
        if not args.dry_run:
            result = subprocess.run(command, cwd=REPO_ROOT, env={**os.environ, **overrides})
            if result.returncode:
                return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
