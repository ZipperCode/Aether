use std::sync::{LazyLock, OnceLock};

use crate::client_profile::ClientProfileStore;

static OS_INFO: LazyLock<os_info::Info> = LazyLock::new(os_info::get);

/// 内置 Codex CLI 基线版本：动态画像刷新（npm stable 标签）不可用时的回退值。
/// 跟随 #876 对齐 openai/codex rust-v0.159.3 模型目录；不要回退到更旧的基线。
/// 该常量同时用作缺省模型目录 `client_version` 期望值；客户端自带值仍优先。
/// 线上 User-Agent 由 `CodexClientProfile::cli` 按当前平台动态生成，因此不存在
/// 静态 UA 常量。
pub const CODEX_CLIENT_VERSION: &str = "0.159.3";
pub const CODEX_CLIENT_ORIGINATOR: &str = "codex_cli_rs";

/// 当前支持的 Codex 客户端类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexClientKind {
    Cli,
    Desktop,
}

/// Codex 上游请求使用的客户端画像。
///
/// 画像由网关后台任务更新，格式转换层只读取不可变快照，避免在请求路径执行网络操作。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexClientProfile {
    pub client_kind: CodexClientKind,
    pub codex_version: String,
    pub originator: String,
    pub user_agent: String,
}

impl CodexClientProfile {
    /// 从稳定版本号创建 CLI 画像；版本校验由发布检查器负责，构造器只拒绝明显非法值。
    pub fn cli(version: &str) -> Result<Self, &'static str> {
        let version = version.trim();
        if version.is_empty()
            || version.len() > 64
            || !version.bytes().all(|byte| (32..=126).contains(&byte))
        {
            return Err("invalid Codex CLI version");
        }
        let originator = "codex_cli_rs".to_owned();
        Ok(Self {
            client_kind: CodexClientKind::Cli,
            codex_version: version.to_owned(),
            // 按 CLI 格式使用当前网关的公开平台信息；架构缺失时与官方 CLI 一致
            // 回退 unknown，不使用 Rust 目标三元组。无客户端终端时不复制调用
            // 方终端后缀、安装标识或个人身份。
            user_agent: format!(
                "{}/{} ({} {}; {}) unknown",
                originator,
                version,
                OS_INFO.os_type(),
                OS_INFO.version(),
                OS_INFO.architecture().unwrap_or("unknown"),
            )
            .chars()
            .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
            .collect(),
            originator,
        })
    }
}

impl Default for CodexClientProfile {
    fn default() -> Self {
        // 最新已核验稳定版本（见 CODEX_CLIENT_VERSION）；后台版本刷新继续作为
        // 版本真源，远程发布检查不可用时保持现有线上行为，避免启动或请求被
        // 版本服务拖住。
        Self::cli(CODEX_CLIENT_VERSION).expect("built-in Codex CLI profile must be valid")
    }
}

static ACTIVE_PROFILE: OnceLock<ClientProfileStore<CodexClientProfile>> = OnceLock::new();

fn active_profile() -> &'static ClientProfileStore<CodexClientProfile> {
    ACTIVE_PROFILE.get_or_init(|| ClientProfileStore::new(CodexClientProfile::default()))
}

/// 返回当前画像的独立快照，调用方不会持有全局锁。
pub fn codex_client_profile() -> CodexClientProfile {
    (*active_profile().snapshot()).clone()
}

/// 原子替换当前画像，并返回替换前的画像。
pub fn set_codex_client_profile(profile: CodexClientProfile) -> CodexClientProfile {
    (*active_profile().publish(profile)).clone()
}

/// 发布一份新的 CLI 画像。
pub fn set_codex_cli_version(version: &str) -> Result<CodexClientProfile, &'static str> {
    let profile = CodexClientProfile::cli(version)?;
    Ok(set_codex_client_profile(profile))
}

/// 返回当前画像的 Codex Core 版本。
pub fn codex_client_version() -> String {
    codex_client_profile().codex_version
}

/// 返回当前画像的 User-Agent。
pub fn codex_client_user_agent() -> String {
    codex_client_profile().user_agent
}

/// 返回当前画像的 originator。
pub fn codex_client_originator() -> String {
    codex_client_profile().originator
}

#[cfg(test)]
mod tests {
    use super::{
        CodexClientKind, CodexClientProfile, CODEX_CLIENT_ORIGINATOR, CODEX_CLIENT_VERSION,
    };

    #[test]
    fn cli_profile_derives_wire_identity_from_version() {
        let profile = CodexClientProfile::cli("0.200.1").expect("valid version");
        assert_eq!(profile.client_kind, CodexClientKind::Cli);
        assert_eq!(profile.originator, "codex_cli_rs");
        // 括号段的架构位来自 os_info 快照（Darwin arm64 为 "arm64"），缺失时
        // 与官方 CLI 一致回退 "unknown"；不使用 Rust 目标三元组。
        let parenthetical = profile
            .user_agent
            .strip_prefix("codex_cli_rs/0.200.1 (")
            .and_then(|rest| rest.strip_suffix(") unknown"))
            .expect("codex_cli_rs/<version> (<os> <osver>; <arch>) unknown");
        let (_, arch) = parenthetical.rsplit_once("; ").expect("os/arch separator");
        assert_eq!(arch, super::OS_INFO.architecture().unwrap_or("unknown"));
    }

    #[test]
    fn cli_profile_rejects_empty_or_control_values() {
        assert!(CodexClientProfile::cli("").is_err());
        assert!(CodexClientProfile::cli("0.1.0\nspoof").is_err());
    }

    #[test]
    fn built_in_client_constants_match_default_profile() {
        let profile = CodexClientProfile::default();
        assert_eq!(profile.codex_version, CODEX_CLIENT_VERSION);
        assert_eq!(profile.originator, CODEX_CLIENT_ORIGINATOR);
        // User-Agent 由平台信息动态生成，无静态常量可比对；基线版本必须落在
        // 动态 UA 的版本位上。
        assert!(profile
            .user_agent
            .starts_with(&format!("codex_cli_rs/{CODEX_CLIENT_VERSION} (")));
        assert!(profile.user_agent.ends_with(") unknown"));
    }
}
