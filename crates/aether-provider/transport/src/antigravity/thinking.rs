use serde_json::{Map, Value};

/// Canonical Antigravity thinking tiers. Gemini only accepts these four wire
/// values, so extended client spellings must be normalized before selection and
/// before the private envelope is finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

impl AntigravityThinkingLevel {
    /// Decode the shared reasoning vocabulary plus Gemini's `extra-low`
    /// compatibility spelling without allocating. `none`, `minimal` and
    /// `extra-low` share the minimal tier; extended high spellings collapse to
    /// `high`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        match value.len() {
            3 if value.eq_ignore_ascii_case("low") => Some(Self::Low),
            3 if value.eq_ignore_ascii_case("max") => Some(Self::High),
            4 if value.eq_ignore_ascii_case("none") => Some(Self::Minimal),
            4 if value.eq_ignore_ascii_case("high") => Some(Self::High),
            5 if value.eq_ignore_ascii_case("xhigh") => Some(Self::High),
            5 if value.eq_ignore_ascii_case("extra") => Some(Self::High),
            6 if value.eq_ignore_ascii_case("medium") => Some(Self::Medium),
            7 if value.eq_ignore_ascii_case("minimal") => Some(Self::Minimal),
            9 if value.eq_ignore_ascii_case("extra-low") => Some(Self::Minimal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

const GEMINI_MODEL_PREFIX: &[u8] = b"gemini-";

/// Recognized terminal effort suffixes paired with their canonical tier.
/// Longer suffixes must precede any suffix that could also match their tail
/// (`-extra-low` before `-low`).
const THINKING_SUFFIXES: [(&str, AntigravityThinkingLevel); 9] = [
    ("-extra-low", AntigravityThinkingLevel::Minimal),
    ("-minimal", AntigravityThinkingLevel::Minimal),
    ("-none", AntigravityThinkingLevel::Minimal),
    ("-low", AntigravityThinkingLevel::Low),
    ("-medium", AntigravityThinkingLevel::Medium),
    ("-high", AntigravityThinkingLevel::High),
    ("-xhigh", AntigravityThinkingLevel::High),
    ("-max", AntigravityThinkingLevel::High),
    ("-extra", AntigravityThinkingLevel::High),
];

/// Read the client's requested thinking level from the request body. Only the
/// field native to `api_format` is consulted so routing rules remain
/// authoritative; Gemini also accepts an already-enveloped private request.
pub fn antigravity_request_thinking_level(
    api_format: &str,
    body: &Value,
) -> Option<AntigravityThinkingLevel> {
    match aether_ai_formats::normalize_api_format_alias(api_format).as_str() {
        "gemini:generate_content" => gemini_request_payload(body)
            .get("generationConfig")?
            .get("thinkingConfig")?
            .get("thinkingLevel")?
            .as_str()
            .and_then(AntigravityThinkingLevel::parse),
        "openai:chat" => body
            .get("reasoning_effort")?
            .as_str()
            .and_then(AntigravityThinkingLevel::parse),
        "openai:responses" | "openai:responses:compact" => body
            .get("reasoning")?
            .get("effort")?
            .as_str()
            .and_then(AntigravityThinkingLevel::parse),
        _ => None,
    }
}

/// Split a client model into its borrowed family and a recognized terminal
/// thinking suffix. Only `gemini-` IDs participate and only complete suffix
/// words are removed; unknown suffixes and non-Gemini families are untouched.
pub fn antigravity_model_thinking_variant(model: &str) -> Option<(&str, AntigravityThinkingLevel)> {
    let model = model.trim();
    if !has_gemini_model_prefix(model) {
        return None;
    }
    for (suffix, level) in THINKING_SUFFIXES {
        let Some(candidate) = strip_terminal_suffix(model, suffix) else {
            continue;
        };
        let candidate = candidate.trim_end();
        if candidate.is_empty() || candidate.ends_with('-') || !has_gemini_model_prefix(candidate) {
            return None;
        }
        return Some((candidate, level));
    }
    None
}

/// Finalize the Gemini `thinkingLevel` at the private-envelope boundary. An
/// explicit suffix always wins over a conflicting body level; a base-only model
/// only canonicalizes a recognized level that is already present. Claude and
/// unrelated model families are intentionally untouched.
pub(super) fn normalize_antigravity_gemini_thinking_level(
    request: &mut Map<String, Value>,
    model: &str,
) {
    let model = model.trim();
    if !has_gemini_model_prefix(model) {
        return;
    }

    let level = match antigravity_model_thinking_variant(model) {
        Some((_, level)) => level,
        None => {
            let existing = request
                .get("generationConfig")
                .and_then(Value::as_object)
                .and_then(|config| config.get("thinkingConfig"))
                .and_then(Value::as_object)
                .and_then(|config| config.get("thinkingLevel"))
                .and_then(Value::as_str);
            match existing.and_then(AntigravityThinkingLevel::parse) {
                Some(level) => level,
                None => return,
            }
        }
    };

    let Some(generation_config) = request
        .entry("generationConfig".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let Some(thinking_config) = generation_config
        .entry("thinkingConfig".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    thinking_config.insert(
        "thinkingLevel".to_string(),
        Value::String(level.as_str().to_string()),
    );
}

fn gemini_request_payload(body: &Value) -> &Value {
    if let Some(inner) = body.get("request") {
        if inner
            .as_object()
            .is_some_and(|request| request.contains_key("contents"))
        {
            return inner;
        }
    }
    body
}

fn has_gemini_model_prefix(value: &str) -> bool {
    value
        .as_bytes()
        .get(..GEMINI_MODEL_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(GEMINI_MODEL_PREFIX))
}

fn strip_terminal_suffix<'a>(model: &'a str, suffix: &str) -> Option<&'a str> {
    let start = model.len().checked_sub(suffix.len())?;
    if !model.is_char_boundary(start) || !model[start..].eq_ignore_ascii_case(suffix) {
        return None;
    }
    Some(&model[..start])
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        antigravity_model_thinking_variant, antigravity_request_thinking_level,
        AntigravityThinkingLevel,
    };
    use crate::antigravity::request::{
        build_antigravity_safe_v1internal_request, AntigravityEnvelopeRequestType,
        AntigravityRequestEnvelopeSupport,
    };
    use crate::antigravity::AntigravityRequestAuth;

    fn sample_auth() -> AntigravityRequestAuth {
        AntigravityRequestAuth {
            project_id: "project-ant-123".to_string(),
            client_version: None,
            session_id: None,
        }
    }

    fn envelope(auth: &AntigravityRequestAuth, model: &str, body: &Value) -> Value {
        match build_antigravity_safe_v1internal_request(
            auth,
            "thinking-test",
            model,
            body,
            AntigravityEnvelopeRequestType::Agent,
        ) {
            AntigravityRequestEnvelopeSupport::Supported(envelope) => envelope,
            AntigravityRequestEnvelopeSupport::Unsupported(reason) => {
                panic!("expected supported envelope: {reason:?}")
            }
        }
    }

    #[test]
    fn thinking_level_parse_normalizes_extended_client_vocabulary() {
        for (raw, expected) in [
            ("high", AntigravityThinkingLevel::High),
            ("HIGH", AntigravityThinkingLevel::High),
            (" xhigh ", AntigravityThinkingLevel::High),
            ("max", AntigravityThinkingLevel::High),
            ("extra", AntigravityThinkingLevel::High),
            ("low", AntigravityThinkingLevel::Low),
            ("\tLow\n", AntigravityThinkingLevel::Low),
            ("medium", AntigravityThinkingLevel::Medium),
            ("none", AntigravityThinkingLevel::Minimal),
            ("minimal", AntigravityThinkingLevel::Minimal),
            (" extra-low ", AntigravityThinkingLevel::Minimal),
        ] {
            assert_eq!(
                AntigravityThinkingLevel::parse(raw),
                Some(expected),
                "{raw:?}"
            );
        }
        for raw in ["", "ultra", "lowest", "extra-lowish"] {
            assert_eq!(AntigravityThinkingLevel::parse(raw), None, "{raw:?}");
        }
        assert_eq!(AntigravityThinkingLevel::High.as_str(), "high");
        assert_eq!(AntigravityThinkingLevel::Minimal.as_str(), "minimal");
    }

    #[test]
    fn model_suffix_parser_keeps_borrowed_family_and_only_strips_known_gemini_words() {
        for (model, base, level) in [
            (
                "gemini-3.8-flash-high",
                "gemini-3.8-flash",
                AntigravityThinkingLevel::High,
            ),
            (
                "Gemini-3-Flash-XHigh",
                "Gemini-3-Flash",
                AntigravityThinkingLevel::High,
            ),
            (
                " gemini-3-flash-max ",
                "gemini-3-flash",
                AntigravityThinkingLevel::High,
            ),
            (
                "gemini-3-flash-extra",
                "gemini-3-flash",
                AntigravityThinkingLevel::High,
            ),
            (
                "gemini-3-flash-low",
                "gemini-3-flash",
                AntigravityThinkingLevel::Low,
            ),
            (
                "gemini-3-flash-medium",
                "gemini-3-flash",
                AntigravityThinkingLevel::Medium,
            ),
            (
                "gemini-3-flash-none",
                "gemini-3-flash",
                AntigravityThinkingLevel::Minimal,
            ),
            (
                "gemini-3-flash-minimal",
                "gemini-3-flash",
                AntigravityThinkingLevel::Minimal,
            ),
            (
                "gemini-3-flash-EXTRA-LOW",
                "gemini-3-flash",
                AntigravityThinkingLevel::Minimal,
            ),
        ] {
            assert_eq!(
                antigravity_model_thinking_variant(model),
                Some((base, level)),
                "{model:?}"
            );
        }

        // `-extra-low` is stripped as one suffix, not as `-low` on `-extra`.
        assert_eq!(
            antigravity_model_thinking_variant("gemini-3-flash-extra-low"),
            Some(("gemini-3-flash", AntigravityThinkingLevel::Minimal))
        );

        for model in [
            "gemini-3-flash",
            "gemini-3-flash-thinking",
            "gemini-3-flash-extra-lowest",
            "gemini-low",
            "gemini--high",
            "claude-opus-4-6-high",
            "gpt-5.2-high",
            "",
        ] {
            assert_eq!(antigravity_model_thinking_variant(model), None, "{model:?}");
        }
    }

    #[test]
    fn request_reader_uses_only_the_native_field_for_each_format() {
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"HIGH"}}}),
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"request":{"contents":[],"generationConfig":{"thinkingConfig":{"thinkingLevel":"medium"}}}}),
            ),
            Some(AntigravityThinkingLevel::Medium)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"reasoning_effort":"high","reasoning":{"effort":"low"}}),
            ),
            None
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:chat",
                &json!({"reasoning_effort":"max","generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}),
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:responses",
                &json!({"reasoning":{"effort":"minimal"},"reasoning_effort":"high"}),
            ),
            Some(AntigravityThinkingLevel::Minimal)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:responses:compact",
                &json!({"reasoning":{"effort":" extra "}}),
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "claude:messages",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}}}),
            ),
            None
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"ultra"}}}),
            ),
            None
        );
    }

    #[test]
    fn final_model_suffix_wins_over_conflicting_body_level_in_both_envelope_paths() {
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {
                "maxOutputTokens": 64000,
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": 4096,
                    "thinkingLevel": "low"
                }
            },
            "labels": {"trajectory_id": "trajectory-123"}
        });
        let wrapped = json!({"request": body});
        for (input, model, expected) in [
            (&body, "gemini-3.8-flash-high", "high"),
            (&body, "gemini-3.8-flash-xhigh", "high"),
            (&body, "gemini-3.8-flash-max", "high"),
            (&body, "gemini-3.8-flash-extra-low", "minimal"),
            (&wrapped, "gemini-3.8-flash-high", "high"),
            (&wrapped, "gemini-3.8-flash-max", "high"),
        ] {
            let original = (*input).clone();
            let output = envelope(&sample_auth(), model, input);
            let generation = &output["request"]["generationConfig"];
            assert_eq!(
                generation["thinkingConfig"]["thinkingLevel"], expected,
                "{model}"
            );
            assert_eq!(generation["maxOutputTokens"], 64000, "{model}");
            assert_eq!(
                generation["thinkingConfig"]["thinkingBudget"], 4096,
                "{model}"
            );
            assert_eq!(
                generation["thinkingConfig"]["includeThoughts"], true,
                "{model}"
            );
            assert_eq!(
                output["request"]["labels"]["trajectory_id"],
                "trajectory-123"
            );
            assert_eq!(output["model"], model);
            assert_eq!(*input, original, "input must not be mutated");
        }
    }

    #[test]
    fn base_only_gemini_canonicalizes_existing_recognized_body_level() {
        for (raw, expected) in [
            ("xhigh", "high"),
            ("max", "high"),
            ("extra", "high"),
            ("HIGH", "high"),
            ("none", "minimal"),
            ("extra-low", "minimal"),
            ("medium", "medium"),
            ("low", "low"),
        ] {
            let body = json!({
                "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
                "generationConfig": {"thinkingConfig": {"thinkingLevel": raw, "thinkingBudget": 2048}}
            });
            let output = envelope(&sample_auth(), "gemini-3.8-flash", &body);
            assert_eq!(
                output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"], expected,
                "{raw}"
            );
            assert_eq!(
                output["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"], 2048,
                "{raw}"
            );
        }

        let unrecognized = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingLevel": "ultra"}}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash", &unrecognized);
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "ultra"
        );

        let missing = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"maxOutputTokens": 128}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash", &missing);
        assert!(output["request"]["generationConfig"]
            .get("thinkingConfig")
            .is_none());
    }

    #[test]
    fn explicit_suffix_adds_level_without_dropping_budgets_or_claude_data() {
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"maxOutputTokens": 256}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash-low", &body);
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
        assert_eq!(
            output["request"]["generationConfig"]["maxOutputTokens"],
            256
        );

        let claude = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingLevel": "low", "thinkingBudget": 1024}}
        });
        let output = envelope(&sample_auth(), "claude-opus-4-6-high", &claude);
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            1024
        );
    }
}
