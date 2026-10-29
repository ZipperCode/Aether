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

/// Read the client's requested thinking level from the requested model suffix
/// or format-native request body.
///
/// Precedence is:
/// 1. Explicit client model suffix (`-low`, `-medium`, `-high`, `-xhigh`, ...).
/// 2. Format-native body field (`thinkingLevel` > `thinkingBudget` for Gemini,
///    `reasoning_effort` for OpenAI Chat, `reasoning.effort` for Responses).
/// 3. `Medium` default for a bare `gemini-` model when the native thinking
///    field is absent.
///
/// An explicit malformed setting returns `None` rather than silently falling
/// back to `Medium`; in Gemini bodies an unrecognized `thinkingLevel` still
/// falls through to a valid numeric `thinkingBudget` when present.
pub fn antigravity_request_thinking_level(
    api_format: &str,
    body: &Value,
    requested_model: &str,
) -> Option<AntigravityThinkingLevel> {
    if let Some((_, level)) = antigravity_model_thinking_variant(requested_model) {
        return Some(level);
    }
    let default_level = default_bare_gemini_thinking_level(requested_model);
    match aether_ai_formats::normalize_api_format_alias(api_format).as_str() {
        "gemini:generate_content" => {
            let payload = gemini_request_payload(body).as_object()?;
            gemini_payload_thinking_level(payload, default_level)
        }
        "openai:chat" => {
            let body = body.as_object()?;
            // OpenAI treats `reasoning_effort: null` as omitted (`formats/openai/reasoning.rs`).
            match body.get("reasoning_effort") {
                None | Some(Value::Null) => default_level,
                Some(effort) => effort.as_str().and_then(AntigravityThinkingLevel::parse),
            }
        }
        "openai:responses" | "openai:responses:compact" => {
            let body = body.as_object()?;
            // OpenAI treats `reasoning: null` and `reasoning.effort: null` as omitted.
            let reasoning = match body.get("reasoning") {
                None | Some(Value::Null) => return default_level,
                Some(reasoning) => reasoning.as_object()?,
            };
            match reasoning.get("effort") {
                None | Some(Value::Null) => default_level,
                Some(effort) => effort.as_str().and_then(AntigravityThinkingLevel::parse),
            }
        }
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
/// explicit suffix always wins over a conflicting body level. Base-only Gemini
/// 3 wire models reuse the shared payload reader so absent thinking settings
/// default to `medium` and numeric budgets derive a level without synthesizing
/// a budget field; legacy Gemini 2.5 models remain budget-driven and are left
/// untouched unless a recognized `thinkingLevel` is already present.
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
        // Gate newly injected defaults and budget-derived levels on Gemini 3
        // `thinkingLevel` capability so Gemini 2.5 budget-only wire payloads
        // never acquire an unsupported `thinkingLevel` field.
        None if aether_ai_formats::gemini_model_uses_thinking_level(model) => {
            let Some(level) =
                gemini_payload_thinking_level(request, default_bare_gemini_thinking_level(model))
            else {
                return;
            };
            level
        }
        None => {
            let existing = request
                .get("generationConfig")
                .or_else(|| request.get("generation_config"))
                .and_then(Value::as_object)
                .and_then(|config| {
                    config
                        .get("thinkingConfig")
                        .or_else(|| config.get("thinking_config"))
                })
                .and_then(Value::as_object)
                .and_then(|config| {
                    config
                        .get("thinkingLevel")
                        .or_else(|| config.get("thinking_level"))
                })
                .and_then(Value::as_str);
            match existing.and_then(AntigravityThinkingLevel::parse) {
                Some(level) => level,
                None => return,
            }
        }
    };
    // Mutate existing camelCase or snake_case containers in place with the same
    // camel-first priority as the reader so same-format/pre-enveloped snake_case
    // payloads never acquire duplicate sibling config objects.
    let generation_key =
        if !request.contains_key("generationConfig") && request.contains_key("generation_config") {
            "generation_config"
        } else {
            "generationConfig"
        };
    let Some(generation_config) = request
        .entry(generation_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let thinking_key = if generation_config.contains_key("thinkingConfig") {
        "thinkingConfig"
    } else if generation_config.contains_key("thinking_config")
        || generation_key == "generation_config"
    {
        "thinking_config"
    } else {
        "thinkingConfig"
    };
    let Some(thinking_config) = generation_config
        .entry(thinking_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let level_key = if thinking_config.contains_key("thinkingLevel") {
        "thinkingLevel"
    } else if thinking_config.contains_key("thinking_level") || thinking_key == "thinking_config" {
        "thinking_level"
    } else {
        "thinkingLevel"
    };
    thinking_config.insert(
        level_key.to_string(),
        Value::String(level.as_str().to_string()),
    );
}

fn gemini_payload_thinking_level(
    payload: &Map<String, Value>,
    default_level: Option<AntigravityThinkingLevel>,
) -> Option<AntigravityThinkingLevel> {
    let Some(generation_config) = payload
        .get("generationConfig")
        .or_else(|| payload.get("generation_config"))
    else {
        return default_level;
    };
    let generation_config = generation_config.as_object()?;
    let Some(thinking_config) = generation_config
        .get("thinkingConfig")
        .or_else(|| generation_config.get("thinking_config"))
    else {
        return default_level;
    };
    let thinking_config = thinking_config.as_object()?;
    let raw_level = thinking_config
        .get("thinkingLevel")
        .or_else(|| thinking_config.get("thinking_level"));
    if let Some(level) = raw_level
        .and_then(Value::as_str)
        .and_then(AntigravityThinkingLevel::parse)
    {
        return Some(level);
    }
    let raw_budget = thinking_config
        .get("thinkingBudget")
        .or_else(|| thinking_config.get("thinking_budget"));
    if let Some(level) = raw_budget.and_then(parse_gemini_thinking_budget) {
        return Some(level);
    }
    // Absent level/budget (including `thinkingConfig` with only `includeThoughts`)
    // inherits `default_level`, whereas an explicit malformed level or budget
    // must fail closed to `None` instead of silently selecting `Medium`.
    if raw_level.is_some() || raw_budget.is_some() {
        return None;
    }
    default_level
}

fn parse_gemini_thinking_budget(raw: &Value) -> Option<AntigravityThinkingLevel> {
    let budget = raw.as_i64()?;
    if budget == -1 {
        return Some(AntigravityThinkingLevel::Medium);
    }
    let budget = u64::try_from(budget).ok()?;
    let effort =
        aether_ai_formats::formats::openai::shared::map_thinking_budget_to_openai_reasoning_effort(
            budget,
        );
    AntigravityThinkingLevel::parse(effort)
}

fn default_bare_gemini_thinking_level(model: &str) -> Option<AntigravityThinkingLevel> {
    let model = model.trim();
    (has_gemini_model_prefix(model)
        && model.len() > GEMINI_MODEL_PREFIX.len()
        && !model.ends_with('-'))
    .then_some(AntigravityThinkingLevel::Medium)
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
    fn request_reader_enforces_suffix_native_field_and_bare_gemini_medium_precedence() {
        // Explicit client suffix outranks conflicting or malformed body fields.
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"low","thinkingBudget":0}}}),
                "gemini-3.8-flash-high",
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:chat",
                &json!({"reasoning_effort":"ultra"}),
                "gemini-3.8-flash-low",
            ),
            Some(AntigravityThinkingLevel::Low)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:responses",
                &json!({"reasoning":{"effort":"high"}}),
                "gemini-3.8-flash-extra-low",
            ),
            Some(AntigravityThinkingLevel::Minimal)
        );

        // Explicit native fields override the bare-Gemini medium default.
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"HIGH"}}}),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({"request":{"contents":[],"generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}}),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::Low)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:chat",
                &json!({"reasoning_effort":"max","generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:responses",
                &json!({"reasoning":{"effort":"minimal"},"reasoning_effort":"high"}),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::Minimal)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "openai:responses:compact",
                &json!({"reasoning":{"effort":" extra "}}),
                "gpt-5.2",
            ),
            Some(AntigravityThinkingLevel::High)
        );

        // Bare Gemini defaults to medium when the format-native thinking field is absent,
        // including `thinkingConfig` with only `includeThoughts`, pre-enveloped requests,
        // and bodies carrying only foreign-format fields.
        for body in [
            json!({"contents":[{"role":"user","parts":[{"text":"hello"}]}]}),
            json!({"generationConfig":{"maxOutputTokens":256}}),
            json!({"generationConfig":{"thinkingConfig":{"includeThoughts":true}}}),
            json!({"request":{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}}),
            json!({"reasoning_effort":"high","reasoning":{"effort":"low"}}),
        ] {
            assert_eq!(
                antigravity_request_thinking_level(
                    "gemini:generate_content",
                    &body,
                    " Gemini-3.8-Flash ",
                ),
                Some(AntigravityThinkingLevel::Medium),
                "gemini body={body}"
            );
            assert_eq!(
                antigravity_request_thinking_level(
                    "gemini:generate_content",
                    &body,
                    "claude-opus-4-6",
                ),
                None,
                "non-gemini body={body}"
            );
        }

        for body in [
            json!({"messages":[{"role":"user","content":"hello"}],"generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}),
            json!({"messages":[{"role":"user","content":"hello"}],"reasoning_effort":null}),
        ] {
            assert_eq!(
                antigravity_request_thinking_level("openai:chat", &body, "gemini-3.8-flash"),
                Some(AntigravityThinkingLevel::Medium),
                "chat body={body}"
            );
            assert_eq!(
                antigravity_request_thinking_level("openai:chat", &body, "gpt-5.2"),
                None,
                "chat non-gemini body={body}"
            );
        }
        for format in ["openai:responses", "openai:responses:compact"] {
            for body in [
                json!({"input":"hello"}),
                json!({"input":"hello","reasoning":null}),
                json!({"input":"hello","reasoning":{"summary":"auto"}}),
                json!({"input":"hello","reasoning":{"effort":null,"summary":"auto"}}),
            ] {
                assert_eq!(
                    antigravity_request_thinking_level(format, &body, "gemini-3.8-flash"),
                    Some(AntigravityThinkingLevel::Medium),
                    "{format} body={body}"
                );
                assert_eq!(
                    antigravity_request_thinking_level(format, &body, "gpt-5.2"),
                    None,
                    "{format} non-gemini body={body}"
                );
            }
        }

        // Unrelated formats and explicit malformed settings never acquire the medium default.
        assert_eq!(
            antigravity_request_thinking_level(
                "claude:messages",
                &json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"high"}}}),
                "gemini-3.8-flash",
            ),
            None
        );
        for body in [
            json!({"generationConfig":{"thinkingConfig":{"thinkingLevel":"ultra"}}}),
            json!({"generationConfig":{"thinkingConfig":{"thinkingBudget":-2}}}),
            json!({"generationConfig":{"thinkingConfig":{"thinkingBudget":"medium"}}}),
            json!({"generationConfig":"invalid"}),
            json!({"generationConfig":{"thinkingConfig":"invalid"}}),
        ] {
            assert_eq!(
                antigravity_request_thinking_level(
                    "gemini:generate_content",
                    &body,
                    "gemini-3.8-flash",
                ),
                None,
                "malformed gemini body={body}"
            );
        }
        for body in [
            json!({"reasoning_effort":"ultra"}),
            json!({"reasoning_effort":123}),
        ] {
            assert_eq!(
                antigravity_request_thinking_level("openai:chat", &body, "gemini-3.8-flash"),
                None,
                "malformed chat body={body}"
            );
        }
        for body in [
            json!({"reasoning":{"effort":"ultra"}}),
            json!({"reasoning":{"effort":123}}),
            json!({"reasoning":"invalid"}),
        ] {
            assert_eq!(
                antigravity_request_thinking_level("openai:responses", &body, "gemini-3.8-flash"),
                None,
                "malformed responses body={body}"
            );
        }
    }

    #[test]
    fn gemini_request_reader_maps_legacy_thinking_budgets_to_levels() {
        for (budget, expected) in [
            (0, AntigravityThinkingLevel::Low),
            (1280, AntigravityThinkingLevel::Low),
            (2048, AntigravityThinkingLevel::Medium),
            (16384, AntigravityThinkingLevel::High),
            (-1, AntigravityThinkingLevel::Medium),
        ] {
            assert_eq!(
                antigravity_request_thinking_level(
                    "gemini:generate_content",
                    &json!({
                        "generationConfig": {
                            "thinkingConfig": { "thinkingBudget": budget }
                        }
                    }),
                    "gemini-3.8-flash",
                ),
                Some(expected),
                "budget={budget}"
            );
        }

        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({
                    "generation_config": {
                        "thinking_config": { "thinking_budget": 16384 }
                    }
                }),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({
                    "generationConfig": {
                        "thinkingConfig": {
                            "thinkingLevel": "low",
                            "thinkingBudget": 16384
                        }
                    }
                }),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::Low)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({
                    "generationConfig": {
                        "thinkingConfig": {
                            "thinkingLevel": "ultra",
                            "thinkingBudget": 16384
                        }
                    }
                }),
                "gemini-3.8-flash",
            ),
            Some(AntigravityThinkingLevel::High)
        );
        assert_eq!(
            antigravity_request_thinking_level(
                "gemini:generate_content",
                &json!({
                    "generationConfig": {
                        "thinkingConfig": { "thinkingBudget": -2 }
                    }
                }),
                "gemini-3.8-flash",
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
    fn base_only_gemini_envelope_normalizes_gemini3_defaults_and_preserves_gemini25_wire_contract()
    {
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

        // Base-only Gemini 3 wire models receive `thinkingLevel: "medium"` when
        // level/budget are absent (missing generationConfig, missing thinkingConfig,
        // includeThoughts-only, or pre-enveloped private request) without synthesizing
        // a numeric `thinkingBudget`.
        for body in [
            json!({"contents": [{"role": "user", "parts": [{"text": "hello"}]}]}),
            json!({
                "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
                "generationConfig": {"maxOutputTokens": 128}
            }),
            json!({
                "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
                "generationConfig": {"maxOutputTokens": 128, "thinkingConfig": {"includeThoughts": true}}
            }),
            json!({
                "request": {
                    "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
                    "generationConfig": {"maxOutputTokens": 128}
                }
            }),
        ] {
            let output = envelope(&sample_auth(), "gemini-3.8-flash", &body);
            let thinking_config = &output["request"]["generationConfig"]["thinkingConfig"];
            assert_eq!(thinking_config["thinkingLevel"], "medium", "body={body}");
            assert!(
                thinking_config.get("thinkingBudget").is_none(),
                "must not synthesize thinkingBudget: {body}"
            );
        }

        // Base-only Gemini 3 models with a legacy numeric budget derive `thinkingLevel`
        // while preserving the caller's numeric `thinkingBudget`.
        let budgeted_gemini3 = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingBudget": 16384, "includeThoughts": true}}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash", &budgeted_gemini3);
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "high"
        );
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            16384
        );
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["includeThoughts"],
            true
        );

        // Explicit malformed settings on base-only Gemini 3 are left untouched rather
        // than overwritten with `medium`.
        let unrecognized = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingLevel": "ultra"}}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash", &unrecognized);
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "ultra"
        );

        let invalid_budget = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingBudget": -2}}
        });
        let output = envelope(&sample_auth(), "gemini-3.8-flash", &invalid_budget);
        assert!(output["request"]["generationConfig"]["thinkingConfig"]
            .get("thinkingLevel")
            .is_none());
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            -2
        );

        // Legacy Gemini 2.5 budget-only wire models remain untouched both with and
        // without numeric `thinkingBudget`.
        let gemini25_budget = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"thinkingConfig": {"thinkingBudget": 2048, "includeThoughts": true}}
        });
        let output = envelope(&sample_auth(), "gemini-2.5-flash", &gemini25_budget);
        assert!(output["request"]["generationConfig"]["thinkingConfig"]
            .get("thinkingLevel")
            .is_none());
        assert_eq!(
            output["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            2048
        );

        let gemini25_missing = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generationConfig": {"maxOutputTokens": 128}
        });
        let output = envelope(&sample_auth(), "gemini-2.5-flash", &gemini25_missing);
        assert!(output["request"]["generationConfig"]
            .get("thinkingConfig")
            .is_none());
        assert_eq!(
            output["request"]["generationConfig"]["maxOutputTokens"],
            128
        );

        // Snake_case fresh and pre-enveloped wire payloads update their existing
        // container in place without creating duplicate camelCase sibling objects.
        let snake_missing = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generation_config": {
                "max_output_tokens": 128,
                "thinking_config": {"include_thoughts": true}
            }
        });
        for input in [&snake_missing, &json!({"request": snake_missing.clone()})] {
            let output = envelope(&sample_auth(), "gemini-3.8-flash", input);
            let req = &output["request"];
            assert!(req.get("generationConfig").is_none(), "input={input}");
            let gen_cfg = &req["generation_config"];
            assert!(gen_cfg.get("thinkingConfig").is_none(), "input={input}");
            assert_eq!(gen_cfg["max_output_tokens"], 128);
            let think_cfg = &gen_cfg["thinking_config"];
            assert_eq!(think_cfg["thinking_level"], "medium", "input={input}");
            assert_eq!(think_cfg["include_thoughts"], true, "input={input}");
            assert!(think_cfg.get("thinkingLevel").is_none(), "input={input}");
            assert!(think_cfg.get("thinking_budget").is_none(), "input={input}");
        }

        for (raw, expected) in [("LOW", "low"), ("xhigh", "high")] {
            let snake_explicit = json!({
                "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
                "generation_config": {
                    "thinking_config": {
                        "thinking_level": raw,
                        "thinking_budget": 4096,
                        "include_thoughts": true
                    }
                }
            });
            for input in [&snake_explicit, &json!({"request": snake_explicit.clone()})] {
                let output = envelope(&sample_auth(), "gemini-3.8-flash", input);
                let req = &output["request"];
                assert!(req.get("generationConfig").is_none(), "raw={raw}");
                let think_cfg = &req["generation_config"]["thinking_config"];
                assert_eq!(think_cfg["thinking_level"], expected, "raw={raw}");
                assert_eq!(think_cfg["thinking_budget"], 4096, "raw={raw}");
                assert_eq!(think_cfg["include_thoughts"], true, "raw={raw}");
                assert!(think_cfg.get("thinkingLevel").is_none(), "raw={raw}");
            }
        }

        let snake_budget_only = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generation_config": {
                "thinking_config": {
                    "thinking_budget": 16384,
                    "include_thoughts": true
                }
            }
        });
        for input in [
            &snake_budget_only,
            &json!({"request": snake_budget_only.clone()}),
        ] {
            let output = envelope(&sample_auth(), "gemini-3.8-flash", input);
            let req = &output["request"];
            assert!(req.get("generationConfig").is_none());
            let think_cfg = &req["generation_config"]["thinking_config"];
            assert_eq!(think_cfg["thinking_level"], "high");
            assert_eq!(think_cfg["thinking_budget"], 16384);
            assert_eq!(think_cfg["include_thoughts"], true);
            assert!(think_cfg.get("thinkingLevel").is_none());
        }
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
