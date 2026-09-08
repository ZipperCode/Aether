"""用无依赖临时 Cargo 包验证网关构建脚本的 Git 监听与版本输入。"""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def run(command, cwd, environment):
    """在临时夹具中执行有限命令，失败时保留输出并让测试失败。"""
    result = subprocess.run(
        command, cwd=cwd, env=environment, capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=60,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    return result.stdout + result.stderr


def make_package(root, build_script, toolchain):
    """复制真实 build.rs 到同深度的极小包，所有构建产物留在临时目录。"""
    package = root / "apps" / "gateway"
    (package / "src").mkdir(parents=True)
    (package / "Cargo.toml").write_text(
        '[package]\nname = "aether-build-watch-fixture"\nversion = "0.0.0"\n'
        'edition = "2021"\n', encoding="utf-8",
    )
    (package / "src" / "lib.rs").write_text(
        '// 构建期版本及构建类型必须能被下游代码消费。\n'
        'pub const VERSION: &str = env!("AETHER_BUILD_VERSION");\n'
        'pub const BUILD_TYPE: &str = env!("AETHER_BUILD_TYPE");\n', encoding="utf-8",
    )
    shutil.copyfile(build_script, package / "build.rs")
    shutil.copyfile(toolchain, root / "rust-toolchain.toml")
    (root / ".gitignore").write_text("target/\nCargo.lock\n", encoding="utf-8")
    return package


def check(package, environment, label, version, *, rerun, build_type="source"):
    """断言真实执行次数、Fresh 与版本，并返回 Cargo 输出供 packed 路径断言。"""
    target = package / "target"
    output = run(
        ["cargo", "check", "--offline", "-vv"], package,
        {**environment, "CARGO_TARGET_DIR": str(target)},
    )
    executions = output.count("] cargo:rustc-env=AETHER_BUILD_VERSION=")
    print(f"{label}: build-script executions={executions}, expected={int(rerun)}", flush=True)
    assert executions == int(rerun), output
    if not rerun:
        assert "Fresh aether-build-watch-fixture" in output, output
    saved_outputs = list((target / "debug" / "build").glob("*/output"))
    assert len(saved_outputs) == 1, saved_outputs
    saved = saved_outputs[0].read_text(encoding="utf-8").splitlines()
    assert f"cargo:rustc-env=AETHER_BUILD_VERSION={version}" in saved, saved
    assert f"cargo:rustc-env=AETHER_BUILD_TYPE={build_type}" in saved, saved
    return saved


def main():
    """依次验证普通仓库、linked worktree、detached HEAD、源码包和显式版本优先级。"""
    started = time.monotonic()
    repository = Path(__file__).resolve().parent.parent
    environment = os.environ.copy()
    for name in ("AETHER_BUILD_VERSION", "AETHER_VERSION", "GITHUB_REF_NAME", "AETHER_BUILD_TYPE"):
        environment.pop(name, None)
    # 仅隔离夹具子进程的 Git 身份、签名和配置，避免触碰开发者仓库与全局配置。
    environment.update({
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_AUTHOR_NAME": "Build Watch Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
        "GIT_COMMITTER_NAME": "Build Watch Fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
    })
    with tempfile.TemporaryDirectory(prefix="aether-build-watch-") as temporary:
        root = Path(temporary)
        checkout = root / "checkout"
        package = make_package(
            checkout, repository / "apps/aether-gateway/build.rs",
            repository / "rust-toolchain.toml",
        )
        run(["git", "init", "-b", "main"], checkout, environment)
        run(["git", "add", "."], checkout, environment)
        run(["git", "commit", "-m", "fixture"], checkout, environment)
        run(["git", "tag", "v1.2.3"], checkout, environment)
        run(["git", "tag", "tunnel-v9.9.9"], checkout, environment)
        check(package, environment, "checkout initial (tunnel tag excluded)", "1.2.3", rerun=True)
        check(package, environment, "checkout unchanged", "1.2.3", rerun=False)

        worktree = root / "linked"
        run(["git", "worktree", "add", "-b", "linked", str(worktree)], checkout, environment)
        linked_package = worktree / "apps" / "gateway"
        check(linked_package, environment, "linked initial", "1.2.3", rerun=True)
        check(linked_package, environment, "linked unchanged", "1.2.3", rerun=False)

        for directory, crate, label in (
            (checkout, package, "checkout"), (worktree, linked_package, "linked"),
        ):
            run(["git", "commit", "--allow-empty", "-m", f"advance {label}"], directory, environment)
            revision = run(["git", "rev-parse", "--short", "HEAD"], directory, environment).strip()
            version = f"1.2.3-1-g{revision}"
            check(crate, environment, f"{label} branch advance", version, rerun=True)
            check(crate, environment, f"{label} advance unchanged", version, rerun=False)

        run(["git", "pack-refs", "--all"], checkout, environment)
        revision = run(["git", "rev-parse", "--short", "HEAD"], checkout, environment).strip()
        check(package, environment, "packed branch", f"1.2.3-1-g{revision}", rerun=True)
        # packed-only 的临时目录监听也必须保持 Fresh，避免另一条重复构建路径。
        check(package, environment, "packed branch unchanged", f"1.2.3-1-g{revision}", rerun=False)
        run(["git", "commit", "--allow-empty", "-m", "advance packed branch"], checkout, environment)
        revision = run(["git", "rev-parse", "--short", "HEAD"], checkout, environment).strip()
        saved = check(package, environment, "packed branch advance", f"1.2.3-2-g{revision}", rerun=True)
        for line in saved:
            if line.startswith("cargo:rerun-if-changed="):
                watched = package / line.split("=", 1)[1]
                assert watched.is_file() and watched.name != "packed-refs", line
        check(package, environment, "packed advance unchanged", f"1.2.3-2-g{revision}", rerun=False)

        run(["git", "checkout", "--detach", "v1.2.3"], worktree, environment)
        check(linked_package, environment, "detached HEAD change", "1.2.3", rerun=True)
        check(linked_package, environment, "detached unchanged", "1.2.3", rerun=False)
        run(["git", "commit", "--allow-empty", "-m", "advance detached"], worktree, environment)
        revision = run(["git", "rev-parse", "--short", "HEAD"], worktree, environment).strip()
        check(linked_package, environment, "detached advance", f"1.2.3-1-g{revision}", rerun=True)

        archive = make_package(
            root / "archive", repository / "apps/aether-gateway/build.rs",
            repository / "rust-toolchain.toml",
        )
        check(archive, environment, "archive initial", "0.0.0", rerun=True)
        check(archive, environment, "archive unchanged", "0.0.0", rerun=False)
        overrides = environment.copy()
        for name, value, expected in (
            ("GITHUB_REF_NAME", "v1.0.0", "1.0.0"),
            ("AETHER_VERSION", "v2.0.0", "2.0.0"),
            ("AETHER_BUILD_VERSION", " v3.0.0 ", "3.0.0"),
            ("AETHER_BUILD_VERSION", "v4.0.0", "4.0.0"),
            ("AETHER_BUILD_VERSION", "tunnel-v9.9.9", "2.0.0"),
        ):
            overrides[name] = value
            check(archive, overrides, f"archive {name}={value}", expected, rerun=True)
        overrides["AETHER_BUILD_TYPE"] = "local"
        check(archive, overrides, "archive build type", "2.0.0", rerun=True, build_type="local")
        check(archive, overrides, "archive explicit unchanged", "2.0.0", rerun=False, build_type="local")
    print(f"PASS: gateway build watch and version contracts ({time.monotonic() - started:.2f}s)")


if __name__ == "__main__":
    main()
