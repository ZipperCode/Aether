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
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.starts_with("x-aether-") {
        return true;
    }
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

#[cfg(test)]
mod tests {
    use super::should_skip_request_header;

    #[test]
    fn strips_all_aether_internal_request_headers() {
        for header in [
            "x-aether-grok-runtime",
            "x-aether-future-control",
            "X-Aether-Tunnel-Forwarded-By",
            "  x-aether-spaced-control  ",
        ] {
            assert!(should_skip_request_header(header), "should skip {header}");
        }
    }

    #[test]
    fn keeps_non_aether_application_headers() {
        assert!(!should_skip_request_header("x-custom-header"));
    }
}
