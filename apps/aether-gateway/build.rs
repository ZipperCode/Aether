use std::env;
use std::path::PathBuf;
use std::process::Command;

/// 声明版本输入并将网关版本和构建类型传给下游编译，保持显式版本优先级不变。
fn main() {
    println!("cargo:rerun-if-env-changed=AETHER_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=AETHER_BUILD_TYPE");
    println!("cargo:rerun-if-env-changed=AETHER_VERSION");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    // 源码包没有 Git 元数据时仍使用稳定输入，避免 Cargo 默认扫描整个包。
    println!("cargo:rerun-if-changed=build.rs");
    watch_git_ref("HEAD");
    if let Some(branch) = git_output(&["symbolic-ref", "--quiet", "HEAD"]) {
        watch_git_ref(&branch);
    }

    let package_version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let version = env::var("AETHER_BUILD_VERSION")
        .ok()
        .and_then(|value| normalize_gateway_version_source(&value))
        .or_else(|| {
            env::var("AETHER_VERSION")
                .ok()
                .and_then(|value| normalize_gateway_version_source(&value))
        })
        .or_else(|| {
            env::var("GITHUB_REF_NAME")
                .ok()
                .and_then(|value| normalize_gateway_version_source(&value))
        })
        .or_else(git_describe_version)
        .filter(|value| !value.is_empty())
        .unwrap_or(package_version);

    println!("cargo:rustc-env=AETHER_BUILD_VERSION={version}");

    let build_type = env::var("AETHER_BUILD_TYPE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "source".to_string());
    println!("cargo:rustc-env=AETHER_BUILD_TYPE={build_type}");
}

/// 由 Git 解析 HEAD/分支的真实位置，兼容 linked worktree 的公共引用目录。
fn watch_git_ref(git_ref: &str) {
    let Some(path) = git_output(&["rev-parse", "--git-path", git_ref]) else {
        return;
    };
    let mut path = PathBuf::from(path);
    if !path.is_file() {
        // packed 分支尚无 loose ref：暂时监听 packed-refs 和最窄现有父目录。
        // 首次提交创建 loose ref 后，下次脚本执行即回到精确文件监听。
        if let Some(packed) = git_output(&["rev-parse", "--git-path", "packed-refs"]) {
            if PathBuf::from(&packed).is_file() {
                println!("cargo:rerun-if-changed={packed}");
            }
        }
        while !path.is_dir() {
            if !path.pop() {
                return;
            }
        }
    }
    println!("cargo:rerun-if-changed={}", path.display());
}

/// 读取 Git 的单行结果；源码包、Git 不可用或无对应引用时不提供该输入。
fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

/// 沿用网关 tag 匹配与 dirty 描述规则，排除 tunnel 独立发布 tag。
fn git_describe_version() -> Option<String> {
    let version = git_output(&[
        "describe", "--tags", "--match", "v[0-9]*", "--always", "--dirty",
    ])?;
    normalize_gateway_version_source(&version)
}

fn normalize_gateway_version_source(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.starts_with("tunnel-v") {
        return None;
    }
    Some(trimmed.strip_prefix('v').unwrap_or(trimmed).to_string())
}
