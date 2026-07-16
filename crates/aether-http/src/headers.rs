//! Shared request-header filtering for Aether HTTP transports.

use aether_contracts::{
    EXECUTION_REQUEST_MAX_RESPONSE_BODY_BYTES_HEADER, USAGE_SERVER_NOW_UNIX_MS_HEADER,
};

/// Returns whether a request header must stay inside the current Aether hop.
///
/// The filter covers standard hop-by-hop headers and Aether-internal control
/// headers that must never be forwarded to an upstream provider or tunnel
/// destination.
pub fn should_skip_request_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "x-aether-execution-path"
            | "x-aether-dependency-reason"
            | "x-aether-execution-loop-guard"
            | "x-aether-control-execute-fallback"
            | "x-aether-rate-limit-preflight"
            | EXECUTION_REQUEST_MAX_RESPONSE_BODY_BYTES_HEADER
            | USAGE_SERVER_NOW_UNIX_MS_HEADER
    )
}
