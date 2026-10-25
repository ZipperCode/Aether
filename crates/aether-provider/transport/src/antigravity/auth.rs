use std::collections::BTreeMap;

use serde_json::Value;

use super::super::snapshot::GatewayProviderTransportSnapshot;
use crate::client_identity::VersionedClientIdentity;

pub const ANTIGRAVITY_PROVIDER_TYPE: &str = "antigravity";
const ANTIGRAVITY_CLIENT_NAME: &str = "antigravity";
const ANTIGRAVITY_GOOG_API_CLIENT: &str = "gl-node/18.18.2 fire/0.8.6 grpc/1.10.x";

// Official Antigravity CLI identity family: `antigravity/cli/<version> <os>/<arch>`, the wire
// identity of the official `agy` binary (public release channel
// `google-antigravity/antigravity-cli`). Pinned baseline 1.4.7 = official release 2026-10-24.
// A per-account `client_version` (auth config / upstream metadata) overrides the baseline for
// that account's requests only; absent or wire-invalid overrides fall back to the baseline.
// Identity is resolved per account and never published as global state.
const ANTIGRAVITY_CLI_BUILTIN_VERSION: &str = "1.4.7";
const ANTIGRAVITY_CLI_AGENT: &str = "antigravity/cli";

/// Platform tokens match the official binary's Go-style GOOS/GOARCH (`darwin|linux|windows`,
/// `arm64|amd64`), derived from the compile target.
fn antigravity_cli_platform_suffix() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    format!(" {os}/{arch}")
}

/// Central Antigravity client identity for headers, envelopes, loadCodeAssist/onboardUser and
/// quota fallbacks. `client_version` is the per-account override; wire-invalid values (see
/// `VersionedClientIdentity`) silently fall back to the pinned baseline instead of failing the
/// request or leaking the raw value onto the wire.
pub fn antigravity_client_identity(client_version: Option<&str>) -> VersionedClientIdentity {
    client_version
        .and_then(|version| {
            VersionedClientIdentity::new(
                version,
                ANTIGRAVITY_CLI_AGENT,
                &antigravity_cli_platform_suffix(),
            )
            .ok()
        })
        .unwrap_or_else(|| {
            VersionedClientIdentity::new(
                ANTIGRAVITY_CLI_BUILTIN_VERSION,
                ANTIGRAVITY_CLI_AGENT,
                &antigravity_cli_platform_suffix(),
            )
            .expect("builtin Antigravity CLI version is wire-safe")
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AntigravityRequestAuth {
    pub project_id: String,
    pub client_version: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityRequestAuthSupport {
    Supported(AntigravityRequestAuth),
    Unsupported(AntigravityRequestAuthUnsupportedReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntigravityRequestAuthUnsupportedReason {
    WrongProviderType,
    MissingAuthConfig,
    InvalidAuthConfigJson,
    ComplexDynamicAuthConfig,
    MissingProjectId,
}

pub fn resolve_local_antigravity_request_auth(
    transport: &GatewayProviderTransportSnapshot,
) -> AntigravityRequestAuthSupport {
    let auth_config = match resolve_antigravity_auth_config(transport) {
        Ok(auth_config) => auth_config,
        Err(reason) => return AntigravityRequestAuthSupport::Unsupported(reason),
    };

    let upstream_metadata = transport.key.upstream_metadata.as_ref();
    let Some(project_id) = find_antigravity_string(
        upstream_metadata,
        &auth_config,
        ANTIGRAVITY_PROJECT_ID_PATHS,
    ) else {
        return AntigravityRequestAuthSupport::Unsupported(
            AntigravityRequestAuthUnsupportedReason::MissingProjectId,
        );
    };
    let (client_version, session_id) =
        find_antigravity_identity_fields(upstream_metadata, &auth_config);

    AntigravityRequestAuthSupport::Supported(AntigravityRequestAuth {
        project_id,
        client_version,
        session_id,
    })
}

/// Account-local identity fields for every account request, including projectless calls
/// (loadCodeAssist / onboardUser). Resolution failures yield `None` fields so requests fall
/// back to the pinned baseline identity instead of failing.
pub fn resolve_antigravity_identity_fields(
    transport: &GatewayProviderTransportSnapshot,
) -> (Option<String>, Option<String>) {
    resolve_antigravity_auth_config(transport)
        .map(|auth_config| {
            find_antigravity_identity_fields(transport.key.upstream_metadata.as_ref(), &auth_config)
        })
        .unwrap_or((None, None))
}

fn resolve_antigravity_auth_config(
    transport: &GatewayProviderTransportSnapshot,
) -> Result<Value, AntigravityRequestAuthUnsupportedReason> {
    if !transport
        .provider
        .provider_type
        .trim()
        .eq_ignore_ascii_case(ANTIGRAVITY_PROVIDER_TYPE)
    {
        return Err(AntigravityRequestAuthUnsupportedReason::WrongProviderType);
    }

    let Some(raw_auth_config) = transport
        .key
        .decrypted_auth_config
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(AntigravityRequestAuthUnsupportedReason::MissingAuthConfig);
    };

    let Ok(auth_config) = serde_json::from_str::<Value>(raw_auth_config) else {
        return Err(AntigravityRequestAuthUnsupportedReason::InvalidAuthConfigJson);
    };

    if contains_blocked_auth_fields(&auth_config) {
        return Err(AntigravityRequestAuthUnsupportedReason::ComplexDynamicAuthConfig);
    }

    Ok(auth_config)
}

pub fn build_antigravity_static_identity_headers(
    auth: &AntigravityRequestAuth,
) -> BTreeMap<String, String> {
    build_antigravity_static_client_headers(
        auth.client_version.as_deref(),
        auth.session_id.as_deref(),
    )
}

pub fn build_antigravity_static_client_headers(
    client_version: Option<&str>,
    session_id: Option<&str>,
) -> BTreeMap<String, String> {
    let identity = antigravity_client_identity(client_version);
    let mut headers = BTreeMap::from([
        (
            String::from("x-client-name"),
            String::from(ANTIGRAVITY_CLIENT_NAME),
        ),
        (
            String::from("x-goog-api-client"),
            String::from(ANTIGRAVITY_GOOG_API_CLIENT),
        ),
        (String::from("user-agent"), identity.user_agent),
        (String::from("x-client-version"), identity.version),
    ]);

    if let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        headers.insert(String::from("x-vscode-sessionid"), session_id.to_string());
    }

    headers
}

const ANTIGRAVITY_PROJECT_ID_PATHS: &[&[&str]] = &[
    &["project_id"],
    &["projectId"],
    &["project", "id"],
    &["project", "project_id"],
    &["project", "projectId"],
    &["cloudaicompanionProject"],
    &["cloudaicompanionProject", "id"],
    &["cloudAiCompanionProject"],
    &["cloudAiCompanionProject", "id"],
    &["antigravity", "project_id"],
    &["antigravity", "projectId"],
    &["antigravity", "project", "id"],
    &["antigravity", "cloudaicompanionProject"],
    &["antigravity", "cloudaicompanionProject", "id"],
    &["antigravity", "cloudAiCompanionProject"],
    &["antigravity", "cloudAiCompanionProject", "id"],
    &["metadata", "project_id"],
    &["metadata", "projectId"],
    &["metadata", "cloudaicompanionProject"],
    &["metadata", "cloudaicompanionProject", "id"],
    &["metadata", "cloudAiCompanionProject"],
    &["metadata", "cloudAiCompanionProject", "id"],
];

const ANTIGRAVITY_CLIENT_VERSION_PATHS: &[&[&str]] = &[
    &["client_version"],
    &["clientVersion"],
    &["antigravity", "client_version"],
    &["antigravity", "clientVersion"],
    &["metadata", "client_version"],
    &["metadata", "clientVersion"],
];

const ANTIGRAVITY_SESSION_ID_PATHS: &[&[&str]] = &[
    &["session_id"],
    &["sessionId"],
    &["antigravity", "session_id"],
    &["antigravity", "sessionId"],
    &["metadata", "session_id"],
    &["metadata", "sessionId"],
];

fn find_antigravity_identity_fields(
    upstream_metadata: Option<&Value>,
    auth_config: &Value,
) -> (Option<String>, Option<String>) {
    (
        find_antigravity_string(
            upstream_metadata,
            auth_config,
            ANTIGRAVITY_CLIENT_VERSION_PATHS,
        ),
        find_antigravity_string(upstream_metadata, auth_config, ANTIGRAVITY_SESSION_ID_PATHS),
    )
}

fn find_antigravity_string(
    upstream_metadata: Option<&Value>,
    auth_config: &Value,
    paths: &[&[&str]],
) -> Option<String> {
    upstream_metadata
        .and_then(|metadata| find_string_by_paths(metadata, paths))
        .or_else(|| find_string_by_paths(auth_config, paths))
}

fn find_string_by_paths(value: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut current = value;
        let mut matched = true;
        for segment in *path {
            let Some(next) = current.get(*segment) else {
                matched = false;
                break;
            };
            current = next;
        }
        if !matched {
            continue;
        }
        if let Some(string) = current
            .as_str()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            return Some(string.to_string());
        }
        if let Some(string) = current
            .as_object()
            .and_then(|object| {
                object
                    .get("id")
                    .or_else(|| object.get("project_id"))
                    .or_else(|| object.get("projectId"))
            })
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            return Some(string.to_string());
        }
    }

    None
}

fn contains_blocked_auth_fields(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, inner)| {
            is_blocked_auth_key(key.as_str()) || contains_blocked_auth_fields(inner)
        }),
        Value::Array(items) => items.iter().any(contains_blocked_auth_fields),
        _ => false,
    }
}

fn is_blocked_auth_key(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "private_key"
            | "privateKey"
            | "private_key_id"
            | "privateKeyId"
            | "service_account"
            | "serviceAccount"
            | "service_account_json"
            | "serviceAccountJson"
            | "service_account_key"
            | "serviceAccountKey"
            | "credential_source"
            | "credentialSource"
            | "token_url"
            | "tokenUrl"
            | "auth_uri"
            | "authUri"
            | "subject"
            | "audience"
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        antigravity_cli_platform_suffix, antigravity_client_identity,
        build_antigravity_static_client_headers, resolve_local_antigravity_request_auth,
        AntigravityRequestAuth, AntigravityRequestAuthSupport,
    };
    use crate::snapshot::{
        GatewayProviderTransportEndpoint, GatewayProviderTransportKey,
        GatewayProviderTransportProvider, GatewayProviderTransportSnapshot,
    };

    fn sample_transport(auth_config: &str) -> GatewayProviderTransportSnapshot {
        GatewayProviderTransportSnapshot {
            provider: GatewayProviderTransportProvider {
                id: "provider-1".to_string(),
                name: "Antigravity".to_string(),
                provider_type: "antigravity".to_string(),
                website: None,
                is_active: true,
                keep_priority_on_conversion: false,
                enable_format_conversion: true,
                concurrent_limit: None,
                max_retries: None,
                proxy: None,
                request_timeout_secs: None,
                stream_first_byte_timeout_secs: None,
                config: None,
            },
            endpoint: GatewayProviderTransportEndpoint {
                id: "endpoint-1".to_string(),
                provider_id: "provider-1".to_string(),
                api_format: "gemini:generate_content".to_string(),
                api_family: Some("gemini".to_string()),
                endpoint_kind: Some("generate_content".to_string()),
                is_active: true,
                base_url: "https://daily-cloudcode-pa.googleapis.com".to_string(),
                header_rules: None,
                body_rules: None,
                max_retries: None,
                custom_path: None,
                config: None,
                format_acceptance_config: None,
                proxy: None,
            },
            key: GatewayProviderTransportKey {
                id: "key-1".to_string(),
                provider_id: "provider-1".to_string(),
                name: "key".to_string(),
                auth_type: "oauth".to_string(),
                is_active: true,
                api_formats: Some(vec!["gemini:generate_content".to_string()]),
                auth_type_by_format: None,
                allow_auth_channel_mismatch_formats: None,
                allowed_models: None,
                capabilities: None,
                rate_multipliers: None,
                global_priority_by_format: None,
                expires_at_unix_secs: None,
                proxy: None,
                fingerprint: None,
                upstream_metadata: None,
                decrypted_api_key: "__placeholder__".to_string(),
                decrypted_auth_config: Some(auth_config.to_string()),
            },
        }
    }

    #[test]
    fn resolves_cloudaicompanion_project_object_from_auth_config() {
        let transport = sample_transport(
            r#"{
                "provider_type":"antigravity",
                "refresh_token":"rt",
                "cloudaicompanionProject":{"id":"project-from-auth-config"}
            }"#,
        );

        assert_eq!(
            resolve_local_antigravity_request_auth(&transport),
            AntigravityRequestAuthSupport::Supported(AntigravityRequestAuth {
                project_id: "project-from-auth-config".to_string(),
                client_version: None,
                session_id: None,
            })
        );
    }

    #[test]
    fn resolves_identity_from_antigravity_upstream_metadata() {
        let mut transport = sample_transport(
            r#"{
                "provider_type":"antigravity",
                "refresh_token":"rt"
            }"#,
        );
        transport.key.upstream_metadata = Some(json!({
            "antigravity": {
                "project_id": "project-from-metadata",
                "client_version": "1.99.0",
                "session_id": "session-from-metadata"
            }
        }));

        assert_eq!(
            resolve_local_antigravity_request_auth(&transport),
            AntigravityRequestAuthSupport::Supported(AntigravityRequestAuth {
                project_id: "project-from-metadata".to_string(),
                client_version: Some("1.99.0".to_string()),
                session_id: Some("session-from-metadata".to_string()),
            })
        );
    }

    #[test]
    fn client_identity_falls_back_to_the_pinned_cli_baseline() {
        let builtin = antigravity_client_identity(None);
        assert_eq!(builtin.version, "1.4.7");
        assert_eq!(
            builtin.user_agent,
            format!("antigravity/cli/1.4.7{}", antigravity_cli_platform_suffix())
        );

        // Wire-invalid overrides never reach the wire; they fall back to the baseline.
        for invalid in ["", "1.0 0", "1.0\r\nx: y"] {
            let fallback = antigravity_client_identity(Some(invalid));
            assert_eq!(fallback.version, "1.4.7");
            assert_eq!(fallback.user_agent, builtin.user_agent);
        }
    }

    #[test]
    fn account_client_version_override_applies_without_leaking_across_accounts() {
        let headers = build_antigravity_static_client_headers(Some("1.5.2"), Some("session-abc"));

        assert_eq!(
            headers.get("x-client-version").map(String::as_str),
            Some("1.5.2")
        );
        let expected_user_agent =
            format!("antigravity/cli/1.5.2{}", antigravity_cli_platform_suffix());
        assert_eq!(
            headers.get("user-agent").map(String::as_str),
            Some(expected_user_agent.as_str())
        );
        assert_eq!(
            headers.get("x-client-name").map(String::as_str),
            Some("antigravity")
        );
        assert_eq!(
            headers.get("x-goog-api-client").map(String::as_str),
            Some("gl-node/18.18.2 fire/0.8.6 grpc/1.10.x")
        );
        assert_eq!(
            headers.get("x-vscode-sessionid").map(String::as_str),
            Some("session-abc")
        );

        // The override is account-local: other accounts still resolve the pinned baseline.
        let default_headers = build_antigravity_static_client_headers(None, None);
        assert_eq!(
            default_headers.get("x-client-version").map(String::as_str),
            Some("1.4.7")
        );
        assert_eq!(
            antigravity_client_identity(None).user_agent,
            format!("antigravity/cli/1.4.7{}", antigravity_cli_platform_suffix())
        );
    }

    #[test]
    fn invalid_account_client_version_override_falls_back_on_the_wire() {
        let headers = build_antigravity_static_client_headers(Some("1.0 0"), None);

        assert_eq!(
            headers.get("x-client-version").map(String::as_str),
            Some("1.4.7")
        );
        assert_eq!(
            headers.get("user-agent").map(String::as_str),
            Some(format!("antigravity/cli/1.4.7{}", antigravity_cli_platform_suffix()).as_str())
        );
    }
}
