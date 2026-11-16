pub mod chat_spec;
pub mod cli_spec;
mod partial_args;
pub mod request;
pub mod response;
pub mod spec;
pub mod stream;

use crate::formats::shared::family::LocalStandardSpec;

pub fn resolve_sync_spec(plan_kind: &str) -> Option<LocalStandardSpec> {
    if plan_kind == crate::formats::openai::audio::AUDIO_SYNC_PLAN_KIND {
        return Some(LocalStandardSpec {
            api_format: crate::formats::openai::audio::AUDIO_API_FORMAT,
            decision_kind: crate::formats::openai::audio::AUDIO_SYNC_PLAN_KIND,
            report_kind: crate::formats::openai::audio::AUDIO_REPORT_KIND,
            family: crate::formats::shared::family::LocalStandardSourceFamily::Standard,
            mode: crate::formats::shared::family::LocalStandardSourceMode::Chat,
            require_streaming: false,
        });
    }
    chat_spec::resolve_sync_spec(plan_kind).or_else(|| cli_spec::resolve_sync_spec(plan_kind))
}

pub fn resolve_stream_spec(plan_kind: &str) -> Option<LocalStandardSpec> {
    chat_spec::resolve_stream_spec(plan_kind).or_else(|| cli_spec::resolve_stream_spec(plan_kind))
}
