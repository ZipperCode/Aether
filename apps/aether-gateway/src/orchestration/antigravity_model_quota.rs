//! Scoped, resettable Antigravity per-model runtime quota observations.
//!
//! An upstream Antigravity quota exhaustion belongs to one serving credential
//! generation and one exact upstream model.  It is deliberately kept out of the
//! durable account scheduling snapshot: the existing account-level runtime ban
//! remains the administrator-recoverable fact, while this module only
//! suppresses the same Provider/Key/credential generation/model tuple until a
//! bounded deadline, a newer successful exact-model quota refresh, or a
//! credential replacement.
//!
//! The shared `RuntimeState` backend is used so a Redis deployment gives every
//! gateway instance the same scoped fact.  Reads use one `kv_get_many` call for
//! a candidate page; a backend failure is returned as an error and must be
//! treated as "state unavailable", never as "quota exhausted".

use std::collections::BTreeMap;
use std::time::Duration;

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_runtime_state::RuntimeState;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::handlers::shared::provider_pool::{
    parse_google_quota_cooldown_seconds_at, parse_retry_after_seconds,
};
use crate::{AppState, GatewayError};

const ANTIGRAVITY_MODEL_QUOTA_KEY_PREFIX: &str = "aether:antigravity:model-quota:v1";
const ANTIGRAVITY_MODEL_QUOTA_RECORD_VERSION: u8 = 1;
const ANTIGRAVITY_MODEL_QUOTA_MAX_DEADLINE_SECONDS: u64 = 31 * 24 * 60 * 60;
/// A configured zero cooldown must not turn a resettable upstream quota error
/// into an unbounded suppression; fall back to the ordinary transient cooldown.
const ANTIGRAVITY_MODEL_QUOTA_DEFAULT_COOLDOWN_SECONDS: u64 = 300;
const QUOTA_POSITIVITY_EPSILON: f64 = 1e-6;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AntigravityModelQuotaCandidate<'a> {
    pub(crate) key: &'a StoredProviderCatalogKey,
    pub(crate) provider_type: &'a str,
    /// Trimmed selected upstream model name.
    pub(crate) model: &'a str,
}

/// Narrow evidence that an Antigravity response is a resettable model quota
/// exhaustion rather than a generic rate limit or account-level failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AntigravityModelQuotaEvidence;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AntigravityModelQuotaRecord {
    version: u8,
    provider_id: String,
    key_id: String,
    credential_generation: String,
    model: String,
    deadline_unix_secs: u64,
    #[serde(default)]
    observation_id: String,
    source: String,
}

/// Returns one boolean per candidate in the same order, using each candidate's
/// decrypted serving auth config.  A single shared backend read serves the whole
/// page; errors mean the backend was unavailable and callers must not treat them
/// as exhausted quota.  Candidates without a stable credential generation
/// cannot match a scoped fact.  The refresh-token component keeps the identity
/// stable across routine OAuth rotation.
pub(crate) async fn antigravity_model_quota_blocked_many_with_auth_configs(
    runtime: &RuntimeState,
    candidates: &[AntigravityModelQuotaCandidate<'_>],
    auth_configs: &[Option<&str>],
    now_unix_secs: u64,
) -> Result<Vec<bool>, GatewayError> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut identities = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let model = candidate.model.trim();
        if model.is_empty() || !is_antigravity(candidate.provider_type) {
            identities.push(None);
            continue;
        }
        let Some(generation) = credential_generation_with_auth_config(
            candidate.key,
            auth_configs.get(index).copied().flatten(),
        ) else {
            identities.push(None);
            continue;
        };
        identities.push(Some((
            antigravity_model_quota_key(
                &candidate.key.provider_id,
                &candidate.key.id,
                &generation,
                model,
            ),
            generation,
        )));
    }

    let lookup_keys = identities
        .iter()
        .flatten()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let values = if lookup_keys.is_empty() {
        Vec::new()
    } else {
        runtime
            .kv_get_many(&lookup_keys)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))?
    };
    let mut values = values.into_iter();

    let mut blocked = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let Some((key, generation)) = identities[index].as_ref() else {
            blocked.push(false);
            continue;
        };
        let value = values.next().unwrap_or(None);
        blocked.push(antigravity_model_quota_record_blocks(
            value.as_deref(),
            candidate.key,
            candidate.model.trim(),
            now_unix_secs,
            key,
            generation,
        ));
    }
    Ok(blocked)
}

/// Persists the scoped observation for the strongly read `key` that actually
/// served the failed request.  Callers must validate the request credential
/// fence before invoking this so a response from replaced credentials can
/// never poison the replacement.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_antigravity_model_quota_exhaustion_with_auth_config(
    runtime: &RuntimeState,
    key: &StoredProviderCatalogKey,
    auth_config_plaintext: Option<&str>,
    provider_type: &str,
    model: &str,
    deadline_unix_secs: u64,
    now_unix_secs: u64,
    source: &str,
) -> Result<bool, GatewayError> {
    if !is_antigravity(provider_type) {
        return Ok(false);
    }
    let model = normalize_model(model);
    if model.is_empty() || deadline_unix_secs <= now_unix_secs {
        return Ok(false);
    }

    let Some(generation) = credential_generation_with_auth_config(key, auth_config_plaintext)
    else {
        return Ok(false);
    };
    let record = AntigravityModelQuotaRecord {
        version: ANTIGRAVITY_MODEL_QUOTA_RECORD_VERSION,
        provider_id: key.provider_id.clone(),
        key_id: key.id.clone(),
        credential_generation: generation.clone(),
        model: model.clone(),
        deadline_unix_secs,
        observation_id: uuid::Uuid::now_v7().to_string(),
        source: source.to_string(),
    };
    let value =
        serde_json::to_string(&record).map_err(|err| GatewayError::Internal(err.to_string()))?;
    let ttl_seconds = deadline_unix_secs
        .saturating_sub(now_unix_secs)
        .clamp(1, ANTIGRAVITY_MODEL_QUOTA_MAX_DEADLINE_SECONDS);
    let key_hash =
        antigravity_model_quota_key(&record.provider_id, &record.key_id, &generation, &model);
    runtime
        .kv_set(&key_hash, value, Some(Duration::from_secs(ttl_seconds)))
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?;

    info!(
        event_name = "antigravity_model_quota_scoped",
        log_type = "event",
        provider_id = %record.provider_id,
        key_id = %record.key_id,
        model = %record.model,
        deadline_unix_secs,
        ttl_seconds,
        source,
        "gateway recorded Antigravity model-scoped quota exhaustion"
    );
    Ok(true)
}

/// Computes the scoped observation deadline.  Upstream quota reset/retry
/// evidence wins; otherwise the configured transient cooldown is used as a
/// finite model-only fallback.
pub(crate) fn antigravity_model_quota_deadline_unix_secs(
    response_headers: Option<&BTreeMap<String, String>>,
    error_body: Option<&str>,
    configured_cooldown_seconds: u64,
    now_unix_secs: u64,
) -> u64 {
    let upstream_seconds = parse_google_quota_cooldown_seconds_at(error_body, now_unix_secs)
        .or_else(|| parse_retry_after_seconds(response_headers));
    let fallback_seconds = if configured_cooldown_seconds == 0 {
        ANTIGRAVITY_MODEL_QUOTA_DEFAULT_COOLDOWN_SECONDS
    } else {
        configured_cooldown_seconds
    };
    let seconds = upstream_seconds.unwrap_or(fallback_seconds);
    now_unix_secs.saturating_add(seconds.clamp(1, ANTIGRAVITY_MODEL_QUOTA_MAX_DEADLINE_SECONDS))
}

/// Narrows an Antigravity HTTP 429 to explicit model-quota evidence.  Generic
/// `RESOURCE_EXHAUSTED`/`RATE_LIMIT_EXCEEDED` responses, account bans and
/// non-Antigravity providers intentionally return `None` and keep the existing
/// account-level path.
pub(crate) fn classify_antigravity_model_quota_exhaustion(
    provider_type: &str,
    status_code: u16,
    response_text: Option<&str>,
) -> Option<AntigravityModelQuotaEvidence> {
    if status_code != 429 || !is_antigravity(provider_type) {
        return None;
    }
    let response_text = response_text
        .map(str::trim)
        .filter(|text| !text.is_empty())?;
    let payload = serde_json::from_str::<Value>(response_text).ok()?;
    let error = payload
        .get("error")
        .and_then(Value::as_object)
        .or_else(|| {
            payload.as_object().filter(|object| {
                object.contains_key("code")
                    || object.contains_key("status")
                    || object.contains_key("message")
            })
        })?;

    let serialized = response_text.to_ascii_lowercase();
    const ACCOUNT_FAILURE_MARKERS: &[&str] = &[
        "banned",
        "suspended",
        "deactivated",
        "account has been disabled",
        "account_disabled",
        "workspace has been disabled",
        "workspace_deactivated",
        "permission_denied",
        "unauthenticated",
    ];
    if ACCOUNT_FAILURE_MARKERS
        .iter()
        .any(|marker| serialized.contains(marker))
    {
        return None;
    }

    let mut quota_specific = false;
    let mut has_rate_limit_evidence = false;

    if let Some(details) = error.get("details").and_then(Value::as_array) {
        for detail in details {
            let Some(detail) = detail.as_object() else {
                continue;
            };
            let reason = detail
                .get("reason")
                .and_then(Value::as_str)
                .map(normalize_code);
            if reason
                .as_deref()
                .is_some_and(|reason| matches!(reason, "quota_exhausted" | "quota_exceeded"))
            {
                quota_specific = true;
            }
            if reason
                .as_deref()
                .is_some_and(|reason| matches!(reason, "rate_limit_exceeded" | "rate_limit_error"))
            {
                has_rate_limit_evidence = true;
            }
            if detail
                .get("@type")
                .and_then(Value::as_str)
                .map(|value| value.to_ascii_lowercase())
                .is_some_and(|value| value.contains("quotafailure"))
            {
                // A QuotaFailure alone (including per-minute request quotas)
                // is not evidence of an exhausted resettable model bucket.
                let serialized_detail = serde_json::to_string(detail)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if serialized_detail.contains("per minute")
                    || serialized_detail.contains("per_minute")
                    || serialized_detail.contains("perminute")
                    || serialized_detail.contains("rpm")
                    || serialized_detail.contains("rate limit")
                    || serialized_detail.contains("requests per")
                {
                    has_rate_limit_evidence = true;
                }
            }
            if let Some(metadata) = detail.get("metadata").and_then(Value::as_object) {
                if metadata.keys().any(|key| {
                    matches!(
                        normalize_code(key).as_str(),
                        "quotaresettimestamp"
                            | "quotaresettime"
                            | "quotaresetdelay"
                            | "quotaresetseconds"
                    )
                }) {
                    quota_specific = true;
                }
            }
        }
    }

    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if message.contains("rate limit")
        || message.contains("too many requests")
        || message.contains("per minute")
        || message.contains("rpm")
    {
        has_rate_limit_evidence = true;
    }
    // Explicit rate/RPM evidence always keeps the account/adaptive path,
    // even when the payload also carries quota wording or RetryInfo.  Only a
    // structured QUOTA_EXHAUSTED reason or quota-reset metadata qualifies.
    if has_rate_limit_evidence || !quota_specific {
        return None;
    }

    Some(AntigravityModelQuotaEvidence)
}

/// Extracts the upstream model explicitly named by an Antigravity quota error
/// payload.  Returns `None` when the response does not name a model, in which
/// case the caller scopes the observation to the selected request model.
pub(crate) fn antigravity_quota_upstream_model(response_text: Option<&str>) -> Option<String> {
    let payload = serde_json::from_str::<Value>(response_text?.trim()).ok()?;
    let error = payload
        .get("error")
        .and_then(Value::as_object)
        .or_else(|| {
            payload.as_object().filter(|object| {
                object.contains_key("code")
                    || object.contains_key("status")
                    || object.contains_key("message")
            })
        })?;
    let mut model = None;
    if let Some(details) = error.get("details").and_then(Value::as_array) {
        for detail in details {
            if let Some(metadata) = detail.get("metadata").and_then(Value::as_object) {
                model = model.or_else(|| model_from_metadata(metadata));
            }
        }
    }
    model.or_else(|| model_from_text(response_text?))
}

fn antigravity_model_quota_record_blocks(
    value: Option<&str>,
    key: &StoredProviderCatalogKey,
    model: &str,
    now_unix_secs: u64,
    expected_key: &str,
    generation: &str,
) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Ok(record) = serde_json::from_str::<AntigravityModelQuotaRecord>(value) else {
        return false;
    };
    if record.version != ANTIGRAVITY_MODEL_QUOTA_RECORD_VERSION
        || record.provider_id != key.provider_id
        || record.key_id != key.id
        || record.credential_generation != generation
        || !record.model.eq_ignore_ascii_case(model)
    {
        return false;
    }
    if expected_key
        != antigravity_model_quota_key(
            &record.provider_id,
            &record.key_id,
            &record.credential_generation,
            &record.model,
        )
        .as_str()
    {
        return false;
    }
    now_unix_secs < record.deadline_unix_secs
}

fn antigravity_model_entry_positive(entry: &Value) -> bool {
    let Some(object) = entry.as_object() else {
        return false;
    };
    if let Some(remaining) = json_f64(object.get("remaining_fraction")) {
        return remaining > QUOTA_POSITIVITY_EPSILON;
    }
    if let Some(used_percent) = json_f64(object.get("used_percent")) {
        return used_percent < 100.0 - QUOTA_POSITIVITY_EPSILON;
    }
    if let Some(exhausted) = object.get("exhausted").and_then(Value::as_bool) {
        return !exhausted;
    }
    if let Some(exhausted) = object.get("is_exhausted").and_then(Value::as_bool) {
        return !exhausted;
    }
    if let Some(remaining) = json_f64(object.get("remaining")) {
        if let Some(limit) = json_f64(object.get("limit").or_else(|| object.get("total"))) {
            return limit > 0.0 && remaining > QUOTA_POSITIVITY_EPSILON;
        }
    }
    false
}

fn antigravity_metadata_bucket(key: &StoredProviderCatalogKey) -> Option<&Map<String, Value>> {
    key.upstream_metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
}

fn antigravity_authoritative_model_map(bucket: &Map<String, Value>) -> Option<&Map<String, Value>> {
    ["quota_by_model", "models", "model_quotas"]
        .into_iter()
        .find_map(|field| bucket.get(field).and_then(Value::as_object))
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), value))
                .collect::<BTreeMap<_, _>>();
            let inner = sorted
                .iter()
                .map(|(key, value)| {
                    format!("{}:{}", Value::String(key.clone()), canonical_json(value))
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{inner}}}")
        }
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => other.to_string(),
    }
}

/// Account-stable credential generation.
///
/// OAuth access-token rotation rewrites the encrypted catalog fields on every
/// successful refresh while the account credential is unchanged. Hashing those
/// fields would release an active model quota on each rotation, so the refresh
/// token identifies the credential generation. Access-token-only credentials
/// have no rotation-stable secret and fall back to the scoped Provider/Key
/// identity; credential replacement is fenced by callers before persistence.
fn credential_generation_with_auth_config(
    key: &StoredProviderCatalogKey,
    auth_config_plaintext: Option<&str>,
) -> Option<String> {
    if let Some(generation) = antigravity_account_generation(
        &key.provider_id,
        &key.id,
        &key.auth_type,
        key.upstream_metadata.as_ref(),
        auth_config_plaintext,
    ) {
        return Some(generation);
    }
    // Access-token-only credentials have no refresh token.  Bind the fact to
    // the stored API credential ciphertext (same on writer and reader) plus
    // the normalized config so a token replacement changes the generation and
    // an old fact cannot block the replacement.
    let mut digest = Sha256::new();
    digest.update(b"access-token");
    for value in [
        key.auth_type.trim().to_ascii_lowercase().as_str(),
        key.provider_id.as_str(),
        key.id.as_str(),
        key.encrypted_api_key.as_deref().unwrap_or_default(),
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    if let Some(config) = auth_config_plaintext {
        let normalized = normalize_json_text(config);
        digest.update((normalized.len() as u64).to_be_bytes());
        digest.update(normalized.as_bytes());
    }
    Some(hex_digest(digest.finalize()))
}

/// Account generation for a strongly read Key plus its decrypted auth config.
pub(crate) fn antigravity_key_account_generation(
    key: &StoredProviderCatalogKey,
    auth_config_plaintext: Option<&str>,
) -> Option<String> {
    credential_generation_with_auth_config(key, auth_config_plaintext)
}

/// Account-stable generation shared by the runtime reader, the failure writer
/// and the request report context.  It intentionally excludes rotating OAuth
/// access-token/auth-config ciphertexts.
pub(crate) fn antigravity_account_generation(
    provider_id: &str,
    key_id: &str,
    auth_type: &str,
    upstream_metadata: Option<&Value>,
    auth_config_plaintext: Option<&str>,
) -> Option<String> {
    let auth_config_plaintext = auth_config_plaintext
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut digest = Sha256::new();
    digest.update(auth_type.trim().to_ascii_lowercase().as_bytes());
    let project_id = upstream_metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
        .and_then(|bucket| bucket.get("project_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match project_id {
        Some(project_id) => {
            digest.update(b"project");
            digest.update((project_id.len() as u64).to_be_bytes());
            digest.update(project_id.as_bytes());
        }
        None => {
            digest.update(b"key");
            digest.update((provider_id.len() as u64).to_be_bytes());
            digest.update(provider_id.as_bytes());
            digest.update((key_id.len() as u64).to_be_bytes());
            digest.update(key_id.as_bytes());
        }
    }
    // Routine OAuth refresh rotates access tokens/expiry only; the refresh
    // token identifies the account credential generation and is absent from
    // access-token rotation.
    let refresh_token = serde_json::from_str::<Value>(auth_config_plaintext)
        .ok()
        .and_then(|config| {
            let object = config.as_object()?;
            object
                .get("refresh_token")
                .or_else(|| object.get("refreshToken"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });
    let has_refresh_token = refresh_token.is_some();
    if let Some(refresh_token) = refresh_token {
        digest.update(b"refresh");
        digest.update((refresh_token.len() as u64).to_be_bytes());
        digest.update(refresh_token.as_bytes());
    }
    if has_refresh_token {
        Some(hex_digest(digest.finalize()))
    } else {
        None
    }
}

fn antigravity_model_quota_key(
    provider_id: &str,
    key_id: &str,
    credential_generation: &str,
    model: &str,
) -> String {
    format!(
        "{ANTIGRAVITY_MODEL_QUOTA_KEY_PREFIX}:{}:{}:{credential_generation}:{}",
        opaque_identifier(provider_id),
        opaque_identifier(key_id),
        opaque_identifier(&model.to_ascii_lowercase()),
    )
}

fn opaque_identifier(identifier: &str) -> String {
    hex_digest(Sha256::digest(identifier.as_bytes()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn normalize_json_text(value: &str) -> String {
    serde_json::from_str::<Value>(value)
        .ok()
        .map(|value| canonical_json(&value))
        .unwrap_or_else(|| value.trim().to_string())
}

fn normalize_model(model: &str) -> String {
    model.trim().to_string()
}

fn is_antigravity(provider_type: &str) -> bool {
    provider_type.trim().eq_ignore_ascii_case("antigravity")
}

fn normalize_code(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' ', '.'], "_")
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

fn model_from_metadata(metadata: &Map<String, Value>) -> Option<String> {
    const MODEL_FIELDS: &[&str] = &[
        "model",
        "model_name",
        "modelName",
        "model_id",
        "modelId",
        "modelVersion",
        "model_version",
    ];
    MODEL_FIELDS.iter().find_map(|field| {
        metadata
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| routable_model_token(value))
            .map(ToOwned::to_owned)
    })
}

fn model_from_text(text: &str) -> Option<String> {
    static MODEL_IN_TEXT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)\bmodel(?:s)?[\s:=/`"']+([A-Za-z0-9][A-Za-z0-9._:-]+)"#)
            .expect("antigravity model extraction regex should compile")
    });
    MODEL_IN_TEXT
        .captures_iter(text)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str()))
        .map(|value| {
            value.trim_matches(|character: char| {
                matches!(character, '.' | ',' | ';' | ':' | '-' | '_')
            })
        })
        .find(|value| looks_like_model_identifier(value))
        .map(ToOwned::to_owned)
}

fn looks_like_model_identifier(value: &str) -> bool {
    routable_model_token(value)
        && value
            .chars()
            .any(|character| character.is_ascii_digit() || matches!(character, '-' | '_' | ':'))
        && !value
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | '-' | '_'))
}

fn routable_model_token(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

/// A captured scoped-KV snapshot used by quota-refresh handlers.  Capture
/// happens before the upstream request; clearing re-reads under the shared
/// quota projection lock and deletes only byte-identical observations.
#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityModelQuotaRefreshCapture {
    provider_id: String,
    key_id: String,
    credential_generation: String,
    entries: Vec<(String, String)>,
}

pub(crate) async fn capture_antigravity_model_quota_records(
    runtime: &RuntimeState,
    provider_id: &str,
    key_id: &str,
    credential_generation: &str,
    models: &[&str],
) -> Result<AntigravityModelQuotaRefreshCapture, GatewayError> {
    let keys = models
        .iter()
        .filter(|model| !normalize_model(model).is_empty())
        .map(|model| {
            antigravity_model_quota_key(
                provider_id,
                key_id,
                credential_generation,
                &normalize_model(model),
            )
        })
        .collect::<Vec<_>>();
    let values = if keys.is_empty() {
        Vec::new()
    } else {
        runtime
            .kv_get_many(&keys)
            .await
            .map_err(|err| GatewayError::Internal(err.to_string()))?
    };
    Ok(AntigravityModelQuotaRefreshCapture {
        provider_id: provider_id.to_string(),
        key_id: key_id.to_string(),
        credential_generation: credential_generation.to_string(),
        entries: keys
            .into_iter()
            .zip(values)
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect(),
    })
}

/// Must be called while holding the same Provider/Key quota projection lock
/// the failure writer uses.  Deletes only entries whose stored observation is
/// still byte-identical to the pre-request snapshot, so a fact written after
/// the refresh started (including a same-second 429) survives.
pub(crate) async fn clear_captured_antigravity_model_quota_records(
    runtime: &RuntimeState,
    capture: &AntigravityModelQuotaRefreshCapture,
) -> Result<usize, GatewayError> {
    if capture.entries.is_empty() {
        return Ok(0);
    }
    let keys = capture
        .entries
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    let current = runtime
        .kv_get_many(&keys)
        .await
        .map_err(|err| GatewayError::Internal(err.to_string()))?;
    let mut cleared = 0;
    for ((key, captured), current) in capture.entries.iter().zip(current) {
        if current.as_deref() == Some(captured.as_str()) {
            cleared += usize::from(
                runtime
                    .kv_delete(key)
                    .await
                    .map_err(|err| GatewayError::Internal(err.to_string()))?,
            );
        }
    }
    Ok(cleared)
}

/// Model names present in the authoritative Antigravity quota map.
pub(crate) fn antigravity_quota_model_names(upstream_metadata: Option<&Value>) -> Vec<String> {
    let Some(bucket) = upstream_metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    antigravity_authoritative_model_map(bucket)
        .map(|models| {
            models
                .keys()
                .map(|name| normalize_model(name))
                .filter(|name| !name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Model names whose exact bucket proves positive availability.
pub(crate) fn antigravity_positive_quota_models(upstream_metadata: Option<&Value>) -> Vec<String> {
    let Some(bucket) = upstream_metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    antigravity_authoritative_model_map(bucket)
        .map(|models| {
            models
                .iter()
                .filter(|(_, entry)| antigravity_model_entry_positive(entry))
                .map(|(name, _)| normalize_model(name))
                .filter(|name| !name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Restricts a capture to the given exact models before clearing.
pub(crate) fn antigravity_capture_subset(
    capture: &AntigravityModelQuotaRefreshCapture,
    provider_id: &str,
    key_id: &str,
    credential_generation: &str,
    models: &[String],
) -> AntigravityModelQuotaRefreshCapture {
    let allowed = models
        .iter()
        .map(|model| {
            antigravity_model_quota_key(
                provider_id,
                key_id,
                credential_generation,
                &normalize_model(model),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    AntigravityModelQuotaRefreshCapture {
        provider_id: capture.provider_id.clone(),
        key_id: capture.key_id.clone(),
        credential_generation: capture.credential_generation.clone(),
        entries: capture
            .entries
            .iter()
            .filter(|(key, _)| allowed.contains(key))
            .cloned()
            .collect(),
    }
}

/// Captures the scoped facts for a Key before its upstream quota request.
/// The materialized serving transport is part of the fence: a strong read that
/// no longer matches its credential cannot capture (and later delete) a fact.
pub(crate) async fn capture_antigravity_model_quota_refresh(
    state: &AppState,
    serving_transport: &crate::provider_transport::GatewayProviderTransportSnapshot,
) -> Result<Option<AntigravityModelQuotaRefreshCapture>, GatewayError> {
    let Some(key) = state
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&serving_transport.key.id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    if key.id != serving_transport.key.id || key.provider_id != serving_transport.provider.id {
        return Ok(None);
    }
    if antigravity_metadata_bucket(&key).is_none() {
        return Ok(None);
    }
    let current_auth_config = state.decrypt_provider_catalog_key_auth_config(&key)?;
    let Some(current_generation) =
        antigravity_key_account_generation(&key, current_auth_config.as_deref())
    else {
        return Ok(None);
    };
    if is_antigravity_refresh_token_credential(current_auth_config.as_deref())
        || is_antigravity_refresh_token_credential(
            serving_transport.key.decrypted_auth_config.as_deref(),
        )
    {
        let Some(serving_generation) = antigravity_key_account_generation(
            &key,
            serving_transport.key.decrypted_auth_config.as_deref(),
        ) else {
            return Ok(None);
        };
        if serving_generation != current_generation {
            return Ok(None);
        }
    } else {
        // Access-token-only credentials: compare the decrypted serving
        // credential with the strongly read Key credential.
        let current_api_key = state.decrypt_provider_catalog_key_api_key(&key)?;
        if current_api_key.as_deref() != Some(serving_transport.key.decrypted_api_key.as_str())
            || current_auth_config.as_deref()
                != serving_transport.key.decrypted_auth_config.as_deref()
        {
            return Ok(None);
        }
    }
    let models = antigravity_quota_model_names(key.upstream_metadata.as_ref());
    let models = models.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(Some(
        capture_antigravity_model_quota_records(
            state.runtime_state(),
            &key.provider_id,
            &key.id,
            &current_generation,
            &models,
        )
        .await?,
    ))
}

fn is_antigravity_refresh_token_credential(auth_config: Option<&str>) -> bool {
    auth_config
        .and_then(|config| serde_json::from_str::<Value>(config).ok())
        .is_some_and(|config| {
            let Some(object) = config.as_object() else {
                return false;
            };
            object
                .get("refresh_token")
                .or_else(|| object.get("refreshToken"))
                .and_then(Value::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
        })
}

/// Reconciles a captured refresh with the strongly persisted Key state.
/// Positivity is the intersection of the durable Key metadata and the incoming
/// persisted positive exact models.  The projection lock is held from before
/// the strong read through clear, and released on every path.
pub(crate) async fn reconcile_antigravity_model_quota_refresh(
    state: &AppState,
    capture: &AntigravityModelQuotaRefreshCapture,
    upstream_metadata: Option<&Value>,
) -> Result<(), GatewayError> {
    if capture.entries.is_empty() {
        return Ok(());
    }
    let projection_lock = state
        .acquire_provider_key_quota_projection_lock(&capture.provider_id, &capture.key_id)
        .await?;
    let result = reconcile_locked(state, capture, upstream_metadata).await;
    state
        .release_provider_key_quota_projection_lock(projection_lock)
        .await;
    result
}

async fn reconcile_locked(
    state: &AppState,
    capture: &AntigravityModelQuotaRefreshCapture,
    upstream_metadata: Option<&Value>,
) -> Result<(), GatewayError> {
    let Some(key) = state
        .list_provider_catalog_keys_by_ids_strong(std::slice::from_ref(&capture.key_id))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(());
    };
    if key.provider_id != capture.provider_id {
        return Ok(());
    }
    let auth_config = state.decrypt_provider_catalog_key_auth_config(&key)?;
    let Some(generation) = antigravity_key_account_generation(&key, auth_config.as_deref()) else {
        return Ok(());
    };
    if generation != capture.credential_generation {
        return Ok(());
    }
    let durable_positive = antigravity_positive_quota_models(key.upstream_metadata.as_ref());
    let incoming_positive = antigravity_positive_quota_models(upstream_metadata);
    let positive = durable_positive
        .into_iter()
        .filter(|model| {
            incoming_positive
                .iter()
                .any(|incoming| incoming.eq_ignore_ascii_case(model))
        })
        .collect::<Vec<_>>();
    let subset = antigravity_capture_subset(
        capture,
        &capture.provider_id,
        &capture.key_id,
        &capture.credential_generation,
        &positive,
    );
    let _ = clear_captured_antigravity_model_quota_records(state.runtime_state(), &subset).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use aether_runtime_state::{MemoryRuntimeStateConfig, RuntimeState};
    use serde_json::json;

    use super::{
        antigravity_model_quota_blocked_many_with_auth_configs,
        antigravity_model_quota_deadline_unix_secs, classify_antigravity_model_quota_exhaustion,
        record_antigravity_model_quota_exhaustion_with_auth_config, AntigravityModelQuotaCandidate,
    };

    const TEST_AUTH: &str = r#"{"refresh_token":"rt-1"}"#;

    async fn blocked(
        runtime: &RuntimeState,
        key: &super::StoredProviderCatalogKey,
        model: &str,
        auth_config: &str,
        now_unix_secs: u64,
    ) -> bool {
        let candidates = [AntigravityModelQuotaCandidate {
            key,
            provider_type: "antigravity",
            model,
        }];
        antigravity_model_quota_blocked_many_with_auth_configs(
            runtime,
            &candidates,
            &[Some(auth_config)],
            now_unix_secs,
        )
        .await
        .expect("read should succeed")
        .into_iter()
        .next()
        .unwrap_or(false)
    }

    fn runtime() -> RuntimeState {
        RuntimeState::memory(MemoryRuntimeStateConfig::default())
    }

    fn key(provider_id: &str, key_id: &str, api_key: &str) -> super::StoredProviderCatalogKey {
        let mut key = super::StoredProviderCatalogKey::new(
            key_id.to_string(),
            provider_id.to_string(),
            key_id.to_string(),
            "oauth".to_string(),
            None,
            true,
        )
        .expect("key should build");
        key.encrypted_api_key = Some(format!("sealed-api-key-{api_key}"));
        key.encrypted_auth_config = Some("sealed-auth-config".to_string());
        key
    }

    fn key_with_quota(
        provider_id: &str,
        key_id: &str,
        api_key: &str,
        updated_at: u64,
        models: serde_json::Value,
    ) -> super::StoredProviderCatalogKey {
        key_with_account_quota(provider_id, key_id, api_key, key_id, updated_at, models)
    }

    fn key_with_account_quota(
        provider_id: &str,
        key_id: &str,
        api_key: &str,
        project_id: &str,
        updated_at: u64,
        models: serde_json::Value,
    ) -> super::StoredProviderCatalogKey {
        let mut key = key(provider_id, key_id, api_key);
        key.upstream_metadata = Some(json!({
            "antigravity": {
                "project_id": project_id,
                "updated_at": updated_at,
                "account_info_status": "success",
                "quota_by_model": models,
            }
        }));
        key
    }

    fn quota_entry(remaining: f64) -> serde_json::Value {
        json!({
            "remaining_fraction": remaining,
            "used_percent": (1.0 - remaining) * 100.0,
        })
    }

    #[test]
    fn classification_requires_explicit_quota_evidence() {
        let generic = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "Too many requests",
                "details": [{"reason": "RATE_LIMIT_EXCEEDED"}]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&generic.to_string())
        )
        .is_none());

        let structured = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "Model quota exhausted",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "QUOTA_EXHAUSTED",
                    "metadata": {
                        "model": "gemini-3-pro-preview",
                        "quotaResetDelay": "30s"
                    }
                }]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&structured.to_string()),
        )
        .is_some());
        assert_eq!(
            super::antigravity_quota_upstream_model(Some(&structured.to_string())).as_deref(),
            Some("gemini-3-pro-preview")
        );

        assert!(classify_antigravity_model_quota_exhaustion(
            "openai",
            429,
            Some(&structured.to_string())
        )
        .is_none());
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            400,
            Some(&structured.to_string())
        )
        .is_none());
    }

    #[test]
    fn classification_accepts_model_scoped_retry_info_fixture() {
        let body = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "You have exhausted your capacity on this model.",
                "details": [
                    {
                        "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                        "reason": "QUOTA_EXHAUSTED",
                        "metadata": {
                            "model": "gemini-3-pro-preview",
                            "model_id": "gemini-3-pro-preview"
                        }
                    },
                    {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "30s"}
                ]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&body.to_string())
        )
        .is_some());
    }

    #[test]
    fn classification_keeps_generic_retry_info_rate_limit_on_account_path() {
        let body = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "Too many requests",
                "details": [
                    {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "2s"}
                ]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&body.to_string())
        )
        .is_none());
    }

    #[test]
    fn classification_keeps_per_minute_quota_failure_on_account_path() {
        let body = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "Quota exceeded for quota metric 'Generate Content API requests per minute'.",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.QuotaFailure",
                    "violations": [{
                        "quotaId": "GenerateContentInputTokensPerModelPerMinute",
                        "quotaMetric": "generativelanguage.googleapis.com/generate_content_requests"
                    }]
                }]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&body.to_string())
        )
        .is_none());
    }

    #[test]
    fn classification_rejects_account_failures() {
        let account_failure = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "account banned quota exhausted",
                "details": [{"reason": "QUOTA_EXHAUSTED"}]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&account_failure.to_string())
        )
        .is_none());
    }

    #[test]
    fn classification_accepts_quota_wording_with_model_reset() {
        let body = json!({
            "error": {
                "code": 429,
                "status": "RESOURCE_EXHAUSTED",
                "message": "You have exhausted your capacity on this model. Your quota will reset after 30s for model gemini-3-pro-preview.",
                "details": [{"reason": "QUOTA_EXHAUSTED"}]
            }
        });
        assert!(classify_antigravity_model_quota_exhaustion(
            "antigravity",
            429,
            Some(&body.to_string()),
        )
        .is_some());
        assert_eq!(
            super::antigravity_quota_upstream_model(Some(&body.to_string())).as_deref(),
            Some("gemini-3-pro-preview")
        );
    }

    #[test]
    fn deadline_prefers_upstream_reset_then_configured_fallback() {
        let headers =
            std::collections::BTreeMap::from([("retry-after".to_string(), "42".to_string())]);
        let body = json!({
            "error": {
                "message": "quota will reset after 90s"
            }
        })
        .to_string();
        assert_eq!(
            antigravity_model_quota_deadline_unix_secs(Some(&headers), Some(&body), 300, 1_000),
            1_090
        );
        assert_eq!(
            antigravity_model_quota_deadline_unix_secs(None, None, 300, 1_000),
            1_300
        );
        // A configured zero must stay finite instead of suppressing forever.
        assert_eq!(
            antigravity_model_quota_deadline_unix_secs(None, None, 0, 1_000),
            1_300
        );
    }

    #[tokio::test]
    async fn scoped_observation_matches_exact_provider_key_credential_and_model() {
        let runtime = runtime();
        let origin_key = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({"gemini-3-pro-preview": quota_entry(0.0)}),
        );
        assert!(record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &origin_key,
            Some(TEST_AUTH),
            "antigravity",
            "gemini-3-pro-preview",
            1_300,
            1_000,
            "test",
        )
        .await
        .expect("record should succeed"));

        assert!(
            blocked(
                &runtime,
                &origin_key,
                "Gemini-3-Pro-Preview",
                TEST_AUTH,
                1_100
            )
            .await
        );

        // A different provider, key, account or model is unaffected.
        for candidate in [
            (
                key_with_account_quota(
                    "provider-2",
                    "key-1",
                    "token-a",
                    "key-1",
                    100,
                    json!({"gemini-3-pro-preview": quota_entry(0.0)}),
                ),
                "gemini-3-pro-preview",
            ),
            (
                key_with_account_quota(
                    "provider-1",
                    "key-2",
                    "token-a",
                    "key-2",
                    100,
                    json!({"gemini-3-pro-preview": quota_entry(0.0)}),
                ),
                "gemini-3-pro-preview",
            ),
            (
                key_with_account_quota(
                    "provider-1",
                    "key-1",
                    "replacement-token",
                    "different-project",
                    100,
                    json!({"gemini-3-pro-preview": quota_entry(0.0)}),
                ),
                "gemini-3-pro-preview",
            ),
            (origin_key.clone(), "claude-sonnet-4-6"),
        ] {
            assert!(
                !blocked(&runtime, &candidate.0, candidate.1, TEST_AUTH, 1_100).await,
                "unrelated identity must stay available"
            );
        }

        // Automatic OAuth rotation rewrites the encrypted access token and
        // auth config for the same account; the scoped quota must survive it.
        let rotated = key_with_quota(
            "provider-1",
            "key-1",
            "rotated-token",
            100,
            json!({"gemini-3-pro-preview": quota_entry(0.0)}),
        );
        assert!(blocked(&runtime, &rotated, "gemini-3-pro-preview", TEST_AUTH, 1_100).await);

        // Deterministic credential fence: same refresh token with a rotated
        // access token keeps the fact; a replaced refresh token releases it.
        assert!(
            blocked(
                &runtime,
                &origin_key,
                "gemini-3-pro-preview",
                r#"{"access_token":"rotated","refresh_token":"rt-1"}"#,
                1_100,
            )
            .await
        );
        assert!(
            !blocked(
                &runtime,
                &origin_key,
                "gemini-3-pro-preview",
                r#"{"refresh_token":"rt-2"}"#,
                1_100,
            )
            .await
        );

        assert!(
            !blocked(
                &runtime,
                &origin_key,
                "gemini-3-pro-preview",
                TEST_AUTH,
                1_300
            )
            .await
        );
    }

    #[tokio::test]
    async fn batch_read_preserves_input_order_and_single_identity_semantics() {
        let runtime = runtime();
        let exhausted = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({"gemini-3-pro-preview": quota_entry(0.0)}),
        );
        let sibling = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({"claude-sonnet-4-6": quota_entry(0.7)}),
        );
        record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &exhausted,
            Some(TEST_AUTH),
            "antigravity",
            "gemini-3-pro-preview",
            1_300,
            1_000,
            "test",
        )
        .await
        .expect("record should succeed");

        let candidates = [
            AntigravityModelQuotaCandidate {
                key: &exhausted,
                provider_type: "antigravity",
                model: "gemini-3-pro-preview",
            },
            AntigravityModelQuotaCandidate {
                key: &sibling,
                provider_type: "antigravity",
                model: "claude-sonnet-4-6",
            },
            AntigravityModelQuotaCandidate {
                key: &sibling,
                provider_type: "openai",
                model: "claude-sonnet-4-6",
            },
        ];
        assert_eq!(
            antigravity_model_quota_blocked_many_with_auth_configs(
                &runtime,
                &candidates,
                &[Some(TEST_AUTH), Some(TEST_AUTH), None],
                1_100,
            )
            .await
            .expect("batch read should succeed"),
            vec![true, false, false]
        );
    }

    #[tokio::test]
    async fn stale_inflight_capture_never_clears_a_newer_429_fact() {
        let runtime = runtime();
        let key = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({"gemini-3-pro-preview": quota_entry(0.0)}),
        );
        let generation = super::antigravity_key_account_generation(&key, Some(TEST_AUTH))
            .expect("generation should exist");
        record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &key,
            Some(TEST_AUTH),
            "antigravity",
            "gemini-3-pro-preview",
            5_000,
            1_000,
            "test",
        )
        .await
        .expect("first record");

        // Refresh starts: capture the existing observation.
        let captured = super::capture_antigravity_model_quota_records(
            &runtime,
            "provider-1",
            "key-1",
            &generation,
            &["gemini-3-pro-preview"],
        )
        .await
        .expect("capture");

        // Same-second new runtime 429 writes a distinct observation id.
        record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &key,
            Some(TEST_AUTH),
            "antigravity",
            "gemini-3-pro-preview",
            5_000,
            1_000,
            "test",
        )
        .await
        .expect("second record");

        assert_eq!(
            super::clear_captured_antigravity_model_quota_records(&runtime, &captured)
                .await
                .expect("clear"),
            0,
            "stale capture must not clear the newer same-second fact"
        );
        assert!(blocked(&runtime, &key, "gemini-3-pro-preview", TEST_AUTH, 1_100).await);
    }

    #[tokio::test]
    async fn positive_exact_model_only_clears_matching_fact_and_failed_refresh_does_not() {
        let runtime = runtime();
        let key = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({
                "gemini-3-pro-preview": quota_entry(0.0),
                "claude-sonnet-4-6": quota_entry(0.0),
            }),
        );
        let generation = super::antigravity_key_account_generation(&key, Some(TEST_AUTH))
            .expect("generation should exist");
        for model in ["gemini-3-pro-preview", "claude-sonnet-4-6"] {
            record_antigravity_model_quota_exhaustion_with_auth_config(
                &runtime,
                &key,
                Some(TEST_AUTH),
                "antigravity",
                model,
                5_000,
                1_000,
                "test",
            )
            .await
            .expect("record");
        }
        let captured = super::capture_antigravity_model_quota_records(
            &runtime,
            "provider-1",
            "key-1",
            &generation,
            &["gemini-3-pro-preview", "claude-sonnet-4-6"],
        )
        .await
        .expect("capture");

        // Failed refresh / non-positive result: no positive subset -> no clear.
        let failed_metadata = json!({
            "antigravity": {
                "updated_at": 1_001,
                "quota_by_model": {
                    "gemini-3-pro-preview": quota_entry(0.0),
                    "claude-sonnet-4-6": quota_entry(0.0),
                }
            }
        });
        let failed_positive = super::antigravity_positive_quota_models(Some(&failed_metadata));
        assert!(failed_positive.is_empty());
        let failed_subset = super::antigravity_capture_subset(
            &captured,
            "provider-1",
            "key-1",
            &generation,
            &failed_positive,
        );
        assert_eq!(
            super::clear_captured_antigravity_model_quota_records(&runtime, &failed_subset)
                .await
                .expect("clear failed"),
            0
        );
        assert!(blocked(&runtime, &key, "gemini-3-pro-preview", TEST_AUTH, 1_100).await);
        assert!(blocked(&runtime, &key, "claude-sonnet-4-6", TEST_AUTH, 1_100).await);

        // Successful refresh: only the positive exact model clears.
        let refreshed_metadata = json!({
            "antigravity": {
                "updated_at": 1_002,
                "quota_by_model": {
                    "gemini-3-pro-preview": quota_entry(0.8),
                    "claude-sonnet-4-6": quota_entry(0.0),
                }
            }
        });
        let positive = super::antigravity_positive_quota_models(Some(&refreshed_metadata));
        assert_eq!(positive, vec!["gemini-3-pro-preview".to_string()]);
        let subset = super::antigravity_capture_subset(
            &captured,
            "provider-1",
            "key-1",
            &generation,
            &positive,
        );
        assert_eq!(
            super::clear_captured_antigravity_model_quota_records(&runtime, &subset)
                .await
                .expect("clear positive"),
            1
        );
        assert!(!blocked(&runtime, &key, "gemini-3-pro-preview", TEST_AUTH, 1_100).await);
        assert!(blocked(&runtime, &key, "claude-sonnet-4-6", TEST_AUTH, 1_100).await);
    }

    #[tokio::test]
    async fn ordered_refresh_restoration_and_stale_metadata_retention() {
        let runtime = runtime();
        let exhausted = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            100,
            json!({
                "gemini-3-pro-preview": quota_entry(0.0),
                "claude-sonnet-4-6": quota_entry(0.0),
            }),
        );
        record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &exhausted,
            Some(TEST_AUTH),
            "antigravity",
            "gemini-3-pro-preview",
            5_000,
            1_000,
            "test",
        )
        .await
        .expect("record should succeed");
        record_antigravity_model_quota_exhaustion_with_auth_config(
            &runtime,
            &exhausted,
            Some(TEST_AUTH),
            "antigravity",
            "claude-sonnet-4-6",
            5_000,
            1_000,
            "test",
        )
        .await
        .expect("record should succeed");

        // Failed/stale refresh: older revision cannot clear.
        let stale = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            99,
            json!({"gemini-3-pro-preview": quota_entry(1.0)}),
        );
        assert!(blocked(&runtime, &stale, "gemini-3-pro-preview", TEST_AUTH, 1_100).await);

        // Unrelated model success cannot clear the scoped model.
        let unrelated = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            1_001,
            json!({
                "gemini-3-pro-preview": quota_entry(0.0),
                "claude-sonnet-4-6": quota_entry(0.9),
            }),
        );
        assert!(
            blocked(
                &runtime,
                &unrelated,
                "gemini-3-pro-preview",
                TEST_AUTH,
                1_100
            )
            .await
        );

        // A same-second snapshot, even with changed content, cannot prove it
        // was observed after the runtime 429.
        let same_second = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            1_000,
            json!({"gemini-3-pro-preview": quota_entry(0.8)}),
        );
        assert!(
            blocked(
                &runtime,
                &same_second,
                "gemini-3-pro-preview",
                TEST_AUTH,
                1_100
            )
            .await
        );

        // Metadata rewritten with a later updated_at (e.g. a model-fetch
        // response that started before the 429) still cannot clear the fact.
        // Only ordered capture+reconcile restores eligibility.
        let delayed_metadata = key_with_quota(
            "provider-1",
            "key-1",
            "token-a",
            1_002,
            json!({
                "gemini-3-pro-preview": quota_entry(0.8),
                "claude-sonnet-4-6": quota_entry(0.0),
            }),
        );
        assert!(
            blocked(
                &runtime,
                &delayed_metadata,
                "gemini-3-pro-preview",
                TEST_AUTH,
                1_100
            )
            .await
        );
        assert!(
            blocked(
                &runtime,
                &delayed_metadata,
                "claude-sonnet-4-6",
                TEST_AUTH,
                1_100
            )
            .await
        );
    }
}
