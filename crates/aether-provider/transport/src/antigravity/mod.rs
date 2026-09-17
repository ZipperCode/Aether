mod auth;
mod policy;
mod request;
mod url;

pub use auth::{
    build_antigravity_static_client_headers, build_antigravity_static_identity_headers,
    resolve_local_antigravity_request_auth, AntigravityRequestAuth, AntigravityRequestAuthSupport,
    AntigravityRequestAuthUnsupportedReason, ANTIGRAVITY_PROVIDER_TYPE,
    ANTIGRAVITY_REQUEST_USER_AGENT,
};
pub use policy::{
    classify_local_antigravity_request_support, is_antigravity_provider_transport,
    AntigravityRequestSideSpec, AntigravityRequestSideSupport,
    AntigravityRequestSideUnsupportedReason,
};
pub use request::{
    build_antigravity_safe_v1internal_request, classify_antigravity_safe_request_body,
    classify_antigravity_thought_signature_rejection, is_antigravity_corrupted_thought_signature,
    repair_antigravity_thought_signature_rejection, repair_antigravity_thought_signatures,
    AntigravityEnvelopeRequestType, AntigravityRequestEnvelopeSupport,
    AntigravityRequestEnvelopeUnsupportedReason, AntigravityThoughtSignatureRejection,
};
pub use url::{
    build_antigravity_v1internal_url, AntigravityRequestUrlAction,
    ANTIGRAVITY_V1INTERNAL_PATH_TEMPLATE,
};
