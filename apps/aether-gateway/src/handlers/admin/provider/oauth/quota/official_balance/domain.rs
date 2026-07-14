use aether_contracts::ProxySnapshot;
use aether_provider_pool::{
    ProviderQuotaRefreshState, ProviderQuotaSnapshotContract, ProviderQuotaSnapshotKind,
};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QuotaKind {
    Balance,
    Subscription,
}

impl QuotaKind {
    pub(super) fn from_spec(value: &str) -> Result<Self, StableErrorClass> {
        match value.trim() {
            "balance" => Ok(Self::Balance),
            "subscription" => Ok(Self::Subscription),
            _ => Err(StableErrorClass::RequestInvalid),
        }
    }

    pub(super) fn empty_snapshot(
        self,
        provider_type: &str,
        now_unix_secs: u64,
    ) -> ProviderQuotaSnapshotContract {
        match self {
            Self::Balance => ProviderQuotaSnapshotContract::balance(provider_type, Vec::new()),
            Self::Subscription => ProviderQuotaSnapshotContract::subscription(
                provider_type,
                Vec::new(),
                now_unix_secs,
            ),
        }
    }

    pub(super) const fn snapshot_kind(self) -> ProviderQuotaSnapshotKind {
        match self {
            Self::Balance => ProviderQuotaSnapshotKind::Balance,
            Self::Subscription => ProviderQuotaSnapshotKind::Subscription,
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Balance => "balance",
            Self::Subscription => "subscription",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StableErrorClass {
    EndpointForeign,
    EndpointInactive,
    EndpointUnofficial,
    RequestInvalid,
    TransportUnavailable,
    TransportFailed,
    HttpUnauthorized,
    HttpForbidden,
    HttpRateLimited,
    HttpClient,
    HttpServer,
    HttpUnexpected,
    ParseFailed,
    PersistenceFailed,
}

impl StableErrorClass {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::EndpointForeign => "endpoint_foreign",
            Self::EndpointInactive => "endpoint_inactive",
            Self::EndpointUnofficial => "endpoint_unofficial",
            Self::RequestInvalid => "request_invalid",
            Self::TransportUnavailable => "transport_unavailable",
            Self::TransportFailed => "transport_failed",
            Self::HttpUnauthorized => "http_unauthorized",
            Self::HttpForbidden => "http_forbidden",
            Self::HttpRateLimited => "http_rate_limited",
            Self::HttpClient => "http_client_error",
            Self::HttpServer => "http_server_error",
            Self::HttpUnexpected => "http_unexpected_status",
            Self::ParseFailed => "parse_failed",
            Self::PersistenceFailed => "persistence_failed",
        }
    }

    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::EndpointForeign => "selected endpoint does not belong to provider",
            Self::EndpointInactive => "selected endpoint is inactive",
            Self::EndpointUnofficial => "selected endpoint is not an official origin",
            Self::RequestInvalid => "official quota request is invalid",
            Self::TransportUnavailable => "quota transport is unavailable",
            Self::TransportFailed => "quota transport failed",
            Self::HttpUnauthorized => "quota upstream rejected authentication",
            Self::HttpForbidden => "quota upstream denied access",
            Self::HttpRateLimited => "quota upstream rate limited the request",
            Self::HttpClient => "quota upstream rejected the request",
            Self::HttpServer => "quota upstream is temporarily unavailable",
            Self::HttpUnexpected => "quota upstream returned an unexpected status",
            Self::ParseFailed => "quota upstream returned an invalid response",
            Self::PersistenceFailed => "quota snapshot could not be stored",
        }
    }

    pub(super) fn persisted_error(self) -> String {
        format!("{}: {}", self.code(), self.message())
    }

    pub(super) const fn from_http_status(status_code: u16) -> Self {
        match status_code {
            401 => Self::HttpUnauthorized,
            403 => Self::HttpForbidden,
            429 => Self::HttpRateLimited,
            400..=499 => Self::HttpClient,
            500..=599 => Self::HttpServer,
            _ => Self::HttpUnexpected,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AttemptResult {
    Success {
        snapshot: ProviderQuotaSnapshotContract,
        status_code: u16,
        quota_kind: QuotaKind,
    },
    HttpFailure {
        status_code: u16,
        headers: BTreeMap<String, String>,
        class: StableErrorClass,
        quota_kind: QuotaKind,
    },
    ParseFailure {
        class: StableErrorClass,
        quota_kind: QuotaKind,
    },
    TransportFailure {
        class: StableErrorClass,
        quota_kind: Option<QuotaKind>,
    },
}

impl AttemptResult {
    pub(super) const fn quota_kind(&self) -> Option<QuotaKind> {
        match self {
            Self::Success { quota_kind, .. }
            | Self::HttpFailure { quota_kind, .. }
            | Self::ParseFailure { quota_kind, .. } => Some(*quota_kind),
            Self::TransportFailure { quota_kind, .. } => *quota_kind,
        }
    }

    pub(super) const fn failure_class(&self) -> Option<StableErrorClass> {
        match self {
            Self::Success { .. } => None,
            Self::HttpFailure { class, .. }
            | Self::ParseFailure { class, .. }
            | Self::TransportFailure { class, .. } => Some(*class),
        }
    }

    pub(super) const fn status_code(&self) -> Option<u16> {
        match self {
            Self::Success { status_code, .. } | Self::HttpFailure { status_code, .. } => {
                Some(*status_code)
            }
            Self::ParseFailure { .. } | Self::TransportFailure { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteSource {
    ExplicitOverride,
    Key,
    Endpoint,
    Provider,
    System,
    Direct,
}

impl RouteSource {
    pub(super) fn configured(value: Option<&str>) -> Self {
        match value {
            Some("key") => Self::Key,
            Some("endpoint") => Self::Endpoint,
            Some("provider") => Self::Provider,
            Some("system") => Self::System,
            Some(_) | None => Self::Direct,
        }
    }

    pub(super) const fn identity(self) -> &'static str {
        match self {
            Self::ExplicitOverride => "explicit_override",
            Self::Key => "key",
            Self::Endpoint => "endpoint",
            Self::Provider => "provider",
            Self::System => "system",
            Self::Direct => "direct",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ExecutionRoute {
    pub(super) proxy: Option<ProxySnapshot>,
    pub(super) source: RouteSource,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FlightScope<'a> {
    pub(super) provider_id: &'a str,
    pub(super) key_id: &'a str,
    pub(super) endpoint_id: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ItemStatus {
    Success,
    Error,
    Backoff,
}

impl ItemStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Backoff => "backoff",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct OfficialQuotaItem {
    pub(super) key_id: String,
    pub(super) key_name: String,
    pub(super) status: ItemStatus,
    pub(super) status_code: Option<u16>,
    pub(super) error_class: Option<StableErrorClass>,
    pub(super) message: Option<String>,
    pub(super) quota_snapshot: Option<Value>,
    pub(super) refresh_state: ProviderQuotaRefreshState,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PersistedSnapshot {
    pub(super) snapshot: Value,
    pub(super) refresh_state: ProviderQuotaRefreshState,
}
