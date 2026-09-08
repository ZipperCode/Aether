"""快速验证真实 CI 入口及工作流接线；仅 mock Cargo 子进程，不编译网关。"""

from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import re
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

workflow = (repo_root / ".github/workflows/rust-ci.yml").read_text(encoding="utf-8")
# 仅提取当前 YAML 的顶层 job 块；完整 YAML 语法由独立解析检查验证，不自建 YAML 解析器。
jobs = dict(re.findall(r"^  (\w+):\n(.*?)(?=^  \w+:\n|\Z)", workflow.split("jobs:\n", 1)[1], re.M | re.S))
for job, stage in (("fmt", "fmt"), ("clippy_gateway", "clippy-gateway"), ("test_gateway", "gateway")):
    commands = re.findall(r"^        run: (python3 tools/ci\.py .+)$", jobs[job], re.M)
    assert len(commands) == 1, (job, commands)
    argv = shlex.split(commands[0])
    assert argv == ["python3", "tools/ci.py", stage], argv
    status, calls, plans = invoke(argv[2:])
    assert status == 0 and len(calls) == 1
    check_call(stage, calls[0], plans[0])
    assert not re.search(r"^    (needs|if|continue-on-error):", jobs[job], re.M)
    assert "continue-on-error:" not in jobs[job]
    assert "run: cargo" not in jobs[job], "duplicated command owner"

# 两个已出现提前中止的任务必须收集全部失败，保持原测试范围且禁止吞掉失败状态。
for job, command in (
    ("test_data", "cargo nextest run -p aether-data --no-fail-fast"),
    ("test_rest", "cargo nextest run --workspace --exclude aether-gateway --exclude aether-data --exclude aether-integration-tests --no-fail-fast"),
):
    commands = re.findall(r"^        run: (cargo nextest run .+)$", jobs[job], re.M)
    assert commands == [command], (job, commands)
    assert "continue-on-error:" not in jobs[job]

gateway_env = jobs["test_gateway"].split("    steps:\n", 1)[0]
assert re.search(r'^    env:\n(?:      #.*\n)*      RUSTFLAGS: "-C link-arg=-fuse-ld=mold"$', gateway_env, re.M)
assert jobs["test_gateway"].count("RUSTFLAGS:") == 1
for name, value in build_env.items():
    assert f"  {name}: {value}\n" in workflow.split("jobs:\n", 1)[0]
rust_setups = re.findall(r"uses: dtolnay/rust-toolchain@.*?\n(.*?)(?=      - |\Z)", workflow, re.S)
assert len(rust_setups) == 11
assert all("          toolchain: 1.95.0\n" in setup for setup in rust_setups)

# 所有原有依赖与必须成功条件保持原样，包含真实 PostgreSQL、feature matrix 和独立适配器门禁。
gates = {
    "clippy": ["clippy_gateway", "clippy_data", "clippy_rest"],
    "test": ["test_gateway", "test_data", "check_data_features", "test_rest", "test_data_adapters", "check_integration_scenarios"],
    "data_db_smoke": ["data_db_smoke_postgres"],
    "check": ["frontend", "fmt", "clippy", "test", "data_db_smoke", "shell_security"],
}
assert set(jobs) == set(gates).union(*(set(needs) for needs in gates.values()))
for gate, dependencies in gates.items():
    body = jobs[gate]
    needs_block = body.split("    needs:\n", 1)[1].split("    if:", 1)[0]
    assert re.findall(r"^      - (\w+)$", needs_block, re.M) == dependencies
    assert "    if: ${{ always() }}\n" in body and "            exit 1\n" in body
    # 直接核对 shell 条件的比较方向，failed/skipped 均不能被汇总为成功。
    required_success = re.findall(r'\$\{\{ needs\.(\w+)\.result \}\}" != "success"', body)
    assert required_success == dependencies, gate

trigger_paths = {
    "rust-toolchain.toml", "aether-vscodex/**", "Makefile", "tools/ci.py",
    "tests/ci_contract_test.py", "tests/gateway_build_watch_test.py",
}
for event, next_block in (("push", "  pull_request:"), ("pull_request", "concurrency:")):
    body = workflow.split(f"  {event}:\n", 1)[1].split(next_block, 1)[0]
    assert trigger_paths <= set(re.findall(r'^      - "([^"]+)"$', body, re.M))
for fixture in ("ci_contract_test.py", "gateway_build_watch_test.py"):
    command = f"          python3 tests/{fixture}\n"
    assert jobs["fmt"].count(command) == 1
    assert jobs["fmt"].index("toolchain: 1.95.0") < jobs["fmt"].index(command)
makefile = (repo_root / "Makefile").read_text(encoding="utf-8")
assert "ci preflight:\n\tpython3 tools/ci.py preflight\n" in makefile
assert "ci-gateway:\n\tpython3 tools/ci.py gateway\n" in makefile

print("PASS: CI dispatcher commands/env/dry-run/failure propagation and workflow gates/triggers")
