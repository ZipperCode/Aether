use base64::Engine as _;
use serde_json::{json, Value};

use super::image::request::{multipart_boundary, parse_multipart_fields};

pub const AUDIO_API_FORMAT: &str = "openai:audio:transcriptions";
pub const AUDIO_SYNC_PLAN_KIND: &str = "openai_audio_transcriptions_sync";
pub const AUDIO_REPORT_KIND: &str = "openai_audio_transcriptions_sync_finalize";

/// 仅接收可忠实转换的转录子集，不伪造 Whisper 时间戳或静默丢弃选项。
pub fn normalize_transcription(content_type: &str, body: &[u8]) -> Result<Value, &'static str> {
    let boundary = multipart_boundary(content_type).ok_or("Expected multipart/form-data")?;
    let fields = parse_multipart_fields(body, &boundary);
    if fields.is_empty() {
        return Err("Invalid multipart body");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut model = None;
    let mut audio = None;
    let mut prompt = String::new();
    let mut response_format = "json".to_string();
    for field in fields {
        if !seen.insert(field.name.clone()) {
            return Err("Duplicate transcription field");
        }
        if field.name == "file" {
            if field.data.is_empty() {
                return Err("Audio file must not be empty");
            }
            let mime = field
                .content_type
                .as_deref()
                .unwrap_or("")
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if !matches!(
                mime,
                "audio/wav"
                    | "audio/x-wav"
                    | "audio/mpeg"
                    | "audio/mp3"
                    | "audio/mp4"
                    | "audio/m4a"
                    | "audio/x-m4a"
                    | "audio/ogg"
                    | "audio/flac"
                    | "audio/x-flac"
                    | "audio/webm"
                    | "audio/aac"
            ) {
                return Err("Unsupported audio content type");
            }
            audio = Some(
                json!({"mimeType":mime,"data":base64::engine::general_purpose::STANDARD.encode(field.data)}),
            );
            continue;
        }
        let text = String::from_utf8(field.data).map_err(|_| "Invalid UTF-8 form field")?;
        match field.name.as_str() {
            "model" if !text.trim().is_empty() => model = Some(text.trim().to_string()),
            "prompt" => prompt = text,
            "response_format" if matches!(text.as_str(), "json" | "text") => response_format = text,
            _ => return Err("Unsupported transcription option or response format"),
        }
    }
    let model = model.ok_or("Missing model")?;
    let audio = audio.ok_or("Missing audio file")?;
    let instruction = if prompt.is_empty() {
        "Transcribe the speech. Return only the transcript.".to_string()
    } else {
        format!("Transcribe the speech. Return only the transcript.\nContext: {prompt}")
    };
    Ok(json!({"model":model,"response_format":response_format,
        "contents":[{"role":"user","parts":[{"text":instruction},{"inlineData":audio}]}]}))
}

pub fn transcript_from_gemini(body: &Value) -> Option<String> {
    let parts = body.pointer("/candidates/0/content/parts")?.as_array()?;
    let text: String = parts
        .iter()
        .filter(|part| part.get("thought").and_then(Value::as_bool) != Some(true))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect();
    (!text.is_empty()).then_some(text)
}

pub fn maybe_build_transcription_sync_product(
    status_code: u16,
    body_json: Option<&Value>,
    body_base64: Option<&str>,
) -> Result<
    Option<crate::formats::shared::sync_products::StandardCrossFormatSyncProduct>,
    crate::formats::shared::AiSurfaceFinalizeError,
> {
    if !(200..300).contains(&status_code) {
        return Ok(None);
    }
    let provider_body = if let Some(body) = body_json {
        body.clone()
    } else {
        let bytes =
            crate::formats::shared::decode_sync_report_body_base64(body_base64.unwrap_or(""))?;
        let Some(body) = serde_json::from_slice(&bytes).ok().or_else(|| {
            crate::formats::shared::sync_products::aggregate_gemini_stream_sync_response(&bytes)
        }) else {
            return Ok(None);
        };
        body
    };
    Ok(transcript_from_gemini(&provider_body).map(|text| {
        crate::formats::shared::sync_products::StandardCrossFormatSyncProduct {
            client_body_json: json!({"text":text}),
            provider_body_json: provider_body,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcription_response_supports_json_and_sse_without_thoughts() {
        let body = json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private"},{"text":"hello"},{"text":" world"}]}}],"usageMetadata":{"promptTokenCount":3}});
        let json_bytes = serde_json::to_vec(&body).unwrap();
        let sse_bytes = format!("data: {body}\n\n").into_bytes();
        for bytes in [&json_bytes, &sse_bytes] {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let product = maybe_build_transcription_sync_product(200, None, Some(&encoded))
                .unwrap()
                .unwrap();
            assert_eq!(product.client_body_json, json!({"text":"hello world"}));
            assert_eq!(
                product.provider_body_json["usageMetadata"]["promptTokenCount"],
                3
            );
        }
        assert!(
            maybe_build_transcription_sync_product(400, Some(&body), None)
                .unwrap()
                .is_none()
        );
        assert!(
            maybe_build_transcription_sync_product(200, Some(&json!({"candidates":[]})), None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn transcription_requires_explicit_model_audio_and_supported_options() {
        let form = |extra: &str| {
            format!("--b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\naccount-model\r\n--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"input.wav\"\r\nContent-Type: audio/wav\r\n\r\nhello\r\n{extra}--b--\r\n")
        };
        let normalized =
            normalize_transcription("multipart/form-data; boundary=b", form("").as_bytes())
                .unwrap();
        assert_eq!(normalized["model"], "account-model");
        assert_eq!(
            normalized["contents"][0]["parts"][1]["inlineData"]["data"],
            "aGVsbG8="
        );
        for extra in ["--b\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\nverbose_json\r\n", "--b\r\nContent-Disposition: form-data; name=\"language\"\r\n\r\nen\r\n", "--b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nother\r\n"] {
            assert!(normalize_transcription("multipart/form-data; boundary=b", form(extra).as_bytes()).is_err());
        }
        for malformed in [
            form("").replace("name=\"model\"", "name=\"prompt\""),
            form("").replace("name=\"file\"", "name=\"prompt\""),
            form("").replace("\r\nhello\r\n", "\r\n\r\n"),
            form("").replace("audio/wav", "application/octet-stream"),
            form("").replace("--b--\r\n", ""),
        ] {
            assert!(normalize_transcription(
                "multipart/form-data; boundary=b",
                malformed.as_bytes()
            )
            .is_err());
        }
        assert!(normalize_transcription("application/json", b"{}").is_err());
        assert!(normalize_transcription("multipart/form-data; boundary=b", b"--b--\r\n").is_err());
        assert_eq!(transcript_from_gemini(&json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private"},{"text":"hello"},{"text":" world"}]}}]})).as_deref(), Some("hello world"));
    }
}
