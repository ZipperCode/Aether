"""快速验证真实 CI 入口；仅 mock Cargo 子进程，不编译网关。"""

from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import runpy
import shlex
import subprocess
from unittest.mock import patch

repo_root = Path(__file__).resolve().parent.parent
ci = runpy.run_path(str(repo_root / "tools/ci.py"))
# 独立验收目标，避免仅把入口自身的常量与自身比较。
expected_commands = {
    "fmt": "cargo fmt --all --check",
    "clippy-gateway": "cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings",
    "gateway": "cargo nextest run -p aether-gateway --lib --bins --no-fail-fast --locked",
}
build_env = {
    "CARGO_INCREMENTAL": "0",
    "CARGO_PROFILE_DEV_DEBUG": "0",
    "CARGO_PROFILE_TEST_DEBUG": "0",
}
# 故意传入不同的 profile 与栈，证明入口覆盖自有设置、保留 linker/wrapper 且不修改父环境。
inherited_env = {
    "CARGO_INCREMENTAL": "1", "CARGO_PROFILE_DEV_DEBUG": "2",
    "CARGO_PROFILE_TEST_DEBUG": "2", "RUST_MIN_STACK": "123456",
    "RUSTFLAGS": "-C link-arg=-fuse-ld=mold", "RUSTC_WRAPPER": "sccache",
    "SCCACHE_GHA_ENABLED": "true", "CI_FIXTURE_PRIVATE": "must-not-be-printed",
}


def invoke(argv, codes=(0,)):
    """执行真实参数解析和调度，只替换外部进程；返回退出码、调用记录和 JSON 计划。"""
    output = io.StringIO()
    results = [subprocess.CompletedProcess([], code) for code in codes]
    with patch.dict(os.environ, inherited_env, clear=True):
        with patch("subprocess.run", side_effect=results) as run, redirect_stdout(output):
            status = ci["main"](argv)
        assert dict(os.environ) == inherited_env, "dispatcher mutated parent environment"
    assert "must-not-be-printed" not in output.getvalue()
    plans = [json.loads(line) for line in output.getvalue().splitlines()]
    return status, run.call_args_list, plans


def check_call(stage, call, plan):
    """核对实际子进程的命令、根目录、环境及输出计划，不允许额外 shell 或重试调用。"""
    command = shlex.split(expected_commands[stage])
    overrides = dict(build_env)
    if stage == "gateway":
        overrides["RUST_MIN_STACK"] = "16777216"
    assert call.args == (command,), call
    assert call.kwargs == {"cwd": repo_root, "env": {**inherited_env, **overrides}}, call
    assert plan == {"stage": stage, "command": command, "env": overrides}, plan


for stage in expected_commands:
    status, calls, plans = invoke([stage])
    assert status == 0 and len(calls) == len(plans) == 1
    check_call(stage, calls[0], plans[0])
    status, calls, _ = invoke([stage], codes=(17,))
    assert status == 17 and len(calls) == 1, "failure was swallowed or retried"

status, calls, plans = invoke(["preflight"], codes=(0, 0, 0))
assert status == 0 and len(calls) == len(plans) == 3
for stage, call, plan in zip(expected_commands, calls, plans):
    check_call(stage, call, plan)
status, calls, _ = invoke(["preflight"], codes=(0, 19))
assert status == 19 and len(calls) == 2, "later stage ran after failure"
status, calls, dry_plans = invoke(["preflight", "--dry-run"])
assert status == 0 and calls == [] and dry_plans == plans, "dry-run executed or changed plan"

print("PASS: CI dispatcher commands/env/dry-run/failure propagation")
