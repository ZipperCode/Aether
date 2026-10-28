//! Antigravity 动态 thinking 档位的真实 HTTP 回归。
//!
//! 复用父模块的 localhost 网关 + 执行运行时夹具：客户端只持有稳定的别名
//! `gemini-3.8-flash`（静态映射到 `-high`），网关必须按请求 effort 在真实账号
//! 已发现的档位中选择上游模型，并把最终 envelope（model + 规范化 thinkingLevel）
//! 与客户端可见完成结果都断言到底。原生 Gemini、转换 Chat、转换 Responses 都走
//! 同一个 plan 校验点，因此这里通过 `generation_plans` 捕获断言实际发出的 envelope。

use std::collections::{BTreeMap, BTreeSet};

use http::StatusCode;
use serde_json::{json, Value};

use super::super::run_gemini_cli_sync_test;
use super::*;

pub(super) const EFFORT_FAMILY_ALIAS: &str = "gemini-3.8-flash";
pub(super) const EFFORT_HIGH_MODEL: &str = "gemini-3.8-flash-high";
pub(super) const EFFORT_MEDIUM_MODEL: &str = "gemini-3.8-flash-medium";
pub(super) const EFFORT_LOW_MODEL: &str = "gemini-3.8-flash-low";
pub(super) const EFFORT_COMPLETION_TEXT: &str = "Dynamic effort completion";
const EFFORT_NATIVE_PROBE_FIELD: &str = "x_aether_effort_probe";

fn effort_models(entries: &[(&str, f64)]) -> BTreeMap<String, ModelQuotaFixture> {
    entries
        .iter()
        .map(|(model, fraction)| {
            (
                (*model).to_string(),
                ModelQuotaFixture::with_fraction(model, *fraction),
            )
        })
        .collect()
}

pub(super) fn all_variant_models(fraction: f64) -> BTreeMap<String, ModelQuotaFixture> {
    effort_models(&[
        (EFFORT_HIGH_MODEL, fraction),
        (EFFORT_MEDIUM_MODEL, fraction),
        (EFFORT_LOW_MODEL, fraction),
    ])
}

pub(super) fn high_medium_models(fraction: f64) -> BTreeMap<String, ModelQuotaFixture> {
    effort_models(&[
        (EFFORT_HIGH_MODEL, fraction),
        (EFFORT_MEDIUM_MODEL, fraction),
    ])
}

pub(super) fn high_low_models(
    high_fraction: f64,
    low_fraction: f64,
) -> BTreeMap<String, ModelQuotaFixture> {
    effort_models(&[
        (EFFORT_HIGH_MODEL, high_fraction),
        (EFFORT_LOW_MODEL, low_fraction),
    ])
}

pub(super) fn dynamic_effort_http_config(
    case: &str,
    mode_label: &str,
    pool_enabled: bool,
    enable_format_conversion: bool,
    key_a_models: BTreeMap<String, ModelQuotaFixture>,
    key_b_models: BTreeMap<String, ModelQuotaFixture>,
) -> AntigravityHttpConfig {
    let prefix = format!("antigravity-effort-{case}-{mode_label}");
    AntigravityHttpConfig {
        mode_label: mode_label.to_string(),
        pool_enabled,
        enable_format_conversion,
        provider_id: format!("provider-{prefix}"),
        endpoint_id: format!("endpoint-{prefix}"),
        model_id: format!("model-{prefix}"),
        global_model_name: EFFORT_FAMILY_ALIAS.to_string(),
        provider_model_name: EFFORT_HIGH_MODEL.to_string(),
        secondary_model_id: Some(format!("model-{prefix}-exact-low")),
        secondary_global_model_name: Some(EFFORT_LOW_MODEL.to_string()),
        secondary_provider_model_name: Some(EFFORT_LOW_MODEL.to_string()),
        generation_response_id: format!("resp-{prefix}"),
        generation_text: EFFORT_COMPLETION_TEXT.to_string(),
        reset_rfc3339: format_rfc3339_secs(current_unix_secs() + 3600),
        key_a: AccountFixtureSpec {
            key_id: format!("key-{prefix}-a"),
            key_name: "account-a".to_string(),
            priority: 1,
            access_token: format!("ya29.fake-{prefix}-token-a"),
            refresh_token: format!("rt-fake-{prefix}-a"),
            project_id: format!("project-{prefix}-a"),
            session_id: format!("sess-{prefix}-a"),
            subscription_tier: "pro".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: key_a_models,
        },
        key_b: AccountFixtureSpec {
            key_id: format!("key-{prefix}-b"),
            key_name: "account-b".to_string(),
            priority: 2,
            access_token: format!("ya29.fake-{prefix}-token-b"),
            refresh_token: format!("rt-fake-{prefix}-b"),
            project_id: format!("project-{prefix}-b"),
            session_id: format!("sess-{prefix}-b"),
            subscription_tier: "pro".to_string(),
            allowed_models: None,
            auto_fetch_models: false,
            models: key_b_models,
        },
    }
}

/// 原生 Gemini 请求体：数值温度、可选 thinkingLevel 与一个上游未知的自定义字段。
fn native_effort_body(session_id: &str, thinking_level: Option<&str>) -> Value {
    let mut generation_config = serde_json::Map::new();
    generation_config.insert("temperature".to_string(), json!(0.2));
    if let Some(level) = thinking_level {
        generation_config.insert(
            "thinkingConfig".to_string(),
            json!({ "thinkingLevel": level }),
        );
    }
    let mut body = serde_json::Map::new();
    body.insert(
        "contents".to_string(),
        json!([{ "role": "user", "parts": [{ "text": "ping" }] }]),
    );
    body.insert(
        "generationConfig".to_string(),
        Value::Object(generation_config),
    );
    body.insert("session_id".to_string(), json!(session_id));
    body.insert(
        EFFORT_NATIVE_PROBE_FIELD.to_string(),
        json!({ "surface": "native-gemini" }),
    );
    Value::Object(body)
}

fn generation_deltas(
    before: &BTreeMap<(bool, String), usize>,
    after: &BTreeMap<(bool, String), usize>,
) -> Vec<((bool, String), usize)> {
    let mut keys: BTreeSet<&(bool, String)> = before.keys().collect();
    keys.extend(after.keys());
    keys.into_iter()
        .filter_map(|key| {
            let delta = after
                .get(key)
                .copied()
                .unwrap_or(0)
                .saturating_sub(before.get(key).copied().unwrap_or(0));
            (delta > 0).then(|| (key.clone(), delta))
        })
        .collect()
}

fn expected_generation(account_a: bool, model: &str, count: usize) -> ((bool, String), usize) {
    ((account_a, model.to_string()), count)
}

fn assert_plan_selection(
    plan: &Value,
    mode_label: &str,
    step: &str,
    expected_model: &str,
    expected_level: &str,
) {
    assert_eq!(
        plan.get("model").and_then(Value::as_str),
        Some(expected_model),
        "{mode_label}/{step}: upstream envelope must carry the account-resolved variant"
    );
    assert_eq!(
        plan.pointer("/request/generationConfig/thinkingConfig/thinkingLevel")
            .and_then(Value::as_str),
        Some(expected_level),
        "{mode_label}/{step}: upstream envelope must carry the canonical thinking level"
    );
    assert_eq!(
        plan.pointer("/request/generationConfig/temperature")
            .and_then(Value::as_f64),
        Some(0.2),
        "{mode_label}/{step}: numeric temperature must survive into the upstream envelope"
    );
}

fn assert_step_plan(
    harness: &AntigravityHttpHarness,
    plans_before: usize,
    mode_label: &str,
    step: &str,
    expected_model: &str,
    expected_level: &str,
) {
    let plans = harness.captured_generation_plans();
    assert_eq!(
        plans.len(),
        plans_before + 1,
        "{mode_label}/{step}: exactly one upstream generation plan expected"
    );
    assert_plan_selection(
        &plans[plans_before],
        mode_label,
        step,
        expected_model,
        expected_level,
    );
}

fn assert_native_probe_preserved(plan: &Value, mode_label: &str, step: &str) {
    assert_eq!(
        plan.pointer(&format!("/request/{EFFORT_NATIVE_PROBE_FIELD}")),
        Some(&json!({ "surface": "native-gemini" })),
        "{mode_label}/{step}: native unknown request field must survive the envelope"
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_native_effort_step(
    harness: &AntigravityHttpHarness,
    mode_label: &str,
    step: &str,
    request_model: &str,
    thinking_level: Option<&str>,
    expected_model: &str,
    expected_level: &str,
    expected_account_a: bool,
) {
    let body = native_effort_body(&harness.client_session_id, thinking_level);
    let plans_before = harness.captured_generation_plans().len();
    let counts_before = harness.generation_counts();
    let response = harness.send_generate_body(request_model, &body, step).await;
    let completion =
        assert_gemini_model_completion(response, mode_label, step, expected_model).await;
    assert_eq!(
        completion
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str),
        Some(EFFORT_COMPLETION_TEXT),
        "{mode_label}/{step}: client-visible completion text mismatch"
    );
    let plans = harness.captured_generation_plans();
    assert_eq!(
        plans.len(),
        plans_before + 1,
        "{mode_label}/{step}: exactly one upstream generation plan expected"
    );
    assert_plan_selection(
        &plans[plans_before],
        mode_label,
        step,
        expected_model,
        expected_level,
    );
    assert_native_probe_preserved(&plans[plans_before], mode_label, step);
    assert_eq!(
        generation_deltas(&counts_before, &harness.generation_counts()),
        vec![expected_generation(expected_account_a, expected_model, 1)],
        "{mode_label}/{step}: request must be served by the expected account+variant only"
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_native_effort_stream_step(
    harness: &AntigravityHttpHarness,
    mode_label: &str,
    step: &str,
    request_model: &str,
    thinking_level: Option<&str>,
    expected_model: &str,
    expected_level: &str,
    expected_account_a: bool,
) {
    let body = native_effort_body(&harness.client_session_id, thinking_level);
    let plans_before = harness.captured_generation_plans().len();
    let counts_before = harness.generation_counts();
    let response = harness
        .send_stream_generate_body(request_model, &body, step)
        .await;
    assert_gemini_stream_model_completion(
        response,
        mode_label,
        step,
        expected_model,
        EFFORT_COMPLETION_TEXT,
    )
    .await;
    let plans = harness.captured_generation_plans();
    assert_eq!(
        plans.len(),
        plans_before + 1,
        "{mode_label}/{step}: native stream must emit exactly one upstream generation plan"
    );
    assert_plan_selection(
        &plans[plans_before],
        mode_label,
        step,
        expected_model,
        expected_level,
    );
    assert_native_probe_preserved(&plans[plans_before], mode_label, step);
    assert_eq!(
        generation_deltas(&counts_before, &harness.generation_counts()),
        vec![expected_generation(expected_account_a, expected_model, 1)],
        "{mode_label}/{step}: native stream must use the expected account+variant"
    );
}

async fn run_native_effort_failover_step(
    harness: &AntigravityHttpHarness,
    mode_label: &str,
    step: &str,
    request_model: &str,
    thinking_level: Option<&str>,
    expected_model: &str,
    expected_level: &str,
) {
    let body = native_effort_body(&harness.client_session_id, thinking_level);
    let plans_before = harness.captured_generation_plans().len();
    let counts_before = harness.generation_counts();
    let response = harness.send_generate_body(request_model, &body, step).await;
    assert_gemini_model_completion(response, mode_label, step, expected_model).await;
    let plans = harness.captured_generation_plans();
    assert_eq!(
        plans.len(),
        plans_before + 2,
        "{mode_label}/{step}: the scoped 429 must trigger exactly one failover plan"
    );
    for plan in &plans[plans_before..] {
        assert_plan_selection(plan, mode_label, step, expected_model, expected_level);
    }
    assert_eq!(
        generation_deltas(&counts_before, &harness.generation_counts()),
        vec![
            expected_generation(false, expected_model, 1),
            expected_generation(true, expected_model, 1),
        ],
        "{mode_label}/{step}: account A must attempt the scoped model once, then account B must serve it"
    );
}

async fn run_converted_chat_step(
    harness: &AntigravityHttpHarness,
    mode_label: &str,
    step: &str,
    reasoning_effort: &str,
    expected_model: &str,
    expected_level: &str,
    expected_account_a: bool,
) {
    let body = json!({
        "model": EFFORT_FAMILY_ALIAS,
        "messages": [{ "role": "user", "content": "ping" }],
        "temperature": 0.2,
        "reasoning_effort": reasoning_effort,
    });
    let plans_before = harness.captured_generation_plans().len();
    let counts_before = harness.generation_counts();
    let response = harness.send_chat_completion_body(&body, step).await;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .expect("chat completions response JSON should parse");
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/{step}: converted chat request must complete, payload={payload}"
    );
    assert_eq!(
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str),
        Some(EFFORT_COMPLETION_TEXT),
        "{mode_label}/{step}: chat client text field mismatch"
    );
    assert_eq!(
        payload.get("model").and_then(Value::as_str),
        Some(expected_model),
        "{mode_label}/{step}: chat completion must report the resolved upstream model"
    );
    assert_step_plan(
        harness,
        plans_before,
        mode_label,
        step,
        expected_model,
        expected_level,
    );
    assert_eq!(
        generation_deltas(&counts_before, &harness.generation_counts()),
        vec![expected_generation(expected_account_a, expected_model, 1)],
        "{mode_label}/{step}: converted chat request must use the expected account+variant"
    );
}

async fn run_converted_responses_step(
    harness: &AntigravityHttpHarness,
    mode_label: &str,
    step: &str,
    reasoning_effort: &str,
    expected_model: &str,
    expected_level: &str,
    expected_account_a: bool,
) {
    let body = json!({
        "model": EFFORT_FAMILY_ALIAS,
        "input": "ping",
        "temperature": 0.2,
        "reasoning": { "effort": reasoning_effort },
    });
    let plans_before = harness.captured_generation_plans().len();
    let counts_before = harness.generation_counts();
    let response = harness.send_responses_body(&body, step).await;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .expect("responses response JSON should parse");
    assert_eq!(
        status,
        StatusCode::OK,
        "{mode_label}/{step}: converted responses request must complete, payload={payload}"
    );
    assert_eq!(
        payload.get("output_text").and_then(Value::as_str),
        Some(EFFORT_COMPLETION_TEXT),
        "{mode_label}/{step}: responses client text field mismatch"
    );
    assert_eq!(
        payload.get("model").and_then(Value::as_str),
        Some(expected_model),
        "{mode_label}/{step}: responses completion must report the resolved upstream model"
    );
    assert_step_plan(
        harness,
        plans_before,
        mode_label,
        step,
        expected_model,
        expected_level,
    );
    assert_eq!(
        generation_deltas(&counts_before, &harness.generation_counts()),
        vec![expected_generation(expected_account_a, expected_model, 1)],
        "{mode_label}/{step}: converted responses request must use the expected account+variant"
    );
}

fn assert_no_unsupported_upstream_plan(harness: &AntigravityHttpHarness, mode_label: &str) {
    let counts = harness.counts();
    assert_eq!(
        (counts.unsupported_a, counts.unsupported_b),
        (0, 0),
        "{mode_label}: no upstream generation may target an account whose discovered catalog lacks the model"
    );
}

#[test]
fn gateway_antigravity_dynamic_effort_native_alias_variants() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_dynamic_effort_native_alias_variants",
        gateway_antigravity_dynamic_effort_native_alias_variants_impl,
    );
}

async fn gateway_antigravity_dynamic_effort_native_alias_variants_impl() {
    run_native_alias_variants_mode(false).await;
    run_native_alias_variants_mode(true).await;
}

/// 同一个静态 high 别名在 ordinary 与 Pool 模式下按 effort 选择真实档位：
/// 数值温度与原生未知字段保留、xhigh/max 规范化为 high、显式 `-low` 别名压过
/// 正文 high、原生 streaming 同样生效，且同一 session 的 warm cache 不串档。
async fn run_native_alias_variants_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let harness = start_antigravity_http_harness(dynamic_effort_http_config(
        "native-aliases",
        mode_label,
        pool_enabled,
        false,
        all_variant_models(0.9),
        high_medium_models(0.9),
    ))
    .await;
    harness.materialize_pool_scores().await;

    run_native_effort_step(
        &harness,
        mode_label,
        "step1-no-effort-default",
        EFFORT_FAMILY_ALIAS,
        None,
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step2-low",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step3-high",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step4-medium",
        EFFORT_FAMILY_ALIAS,
        Some("medium"),
        EFFORT_MEDIUM_MODEL,
        "medium",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step5-xhigh-normalizes-high",
        EFFORT_FAMILY_ALIAS,
        Some("xhigh"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step6-max-normalizes-high",
        EFFORT_FAMILY_ALIAS,
        Some("max"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    // 同一 session 已 warm 过 high 之后，再次低档请求不得复用缓存的 high 选择。
    run_native_effort_step(
        &harness,
        mode_label,
        "step7-warm-cache-no-effort",
        EFFORT_FAMILY_ALIAS,
        None,
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step8-low-after-warm-high",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    // 客户端显式低档模型后缀优先于冲突的正文 high。
    run_native_effort_step(
        &harness,
        mode_label,
        "step9-exact-low-alias-beats-body-high",
        EFFORT_LOW_MODEL,
        Some("high"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    run_native_effort_stream_step(
        &harness,
        mode_label,
        "step10-native-stream-medium",
        EFFORT_FAMILY_ALIAS,
        Some("medium"),
        EFFORT_MEDIUM_MODEL,
        "medium",
        true,
    )
    .await;

    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
            Some(harness.key_a_id.as_str()),
            "{mode_label}: the same session should remain sticky to account A"
        );
    }
    assert_no_unsupported_upstream_plan(&harness, mode_label);
}

#[test]
fn gateway_antigravity_dynamic_effort_account_variant_admission() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_dynamic_effort_account_variant_admission",
        gateway_antigravity_dynamic_effort_account_variant_admission_impl,
    );
}

async fn gateway_antigravity_dynamic_effort_account_variant_admission_impl() {
    run_account_variant_admission_mode(false).await;
    run_account_variant_admission_mode(true).await;
}

/// 缺失档位只在账号内拒绝：A 没有 medium 时由 B 直接服务，不产生对 A 的
/// 不受支持上游调用；A 上 high 的 scoped 429 不能阻断同账号的 low，Pool sticky
/// 在账号变更后重新按新账号的发现目录解析档位。
async fn run_account_variant_admission_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let harness = start_antigravity_http_harness(dynamic_effort_http_config(
        "variant-admission",
        mode_label,
        pool_enabled,
        false,
        high_low_models(0.9, 0.9),
        high_medium_models(0.9),
    ))
    .await;
    harness.materialize_pool_scores().await;

    run_native_effort_step(
        &harness,
        mode_label,
        "step1-high-warm",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    // A 的权威目录没有 medium：跳过 A，直接由 B 服务。
    run_native_effort_step(
        &harness,
        mode_label,
        "step2-medium-missing-on-a",
        EFFORT_FAMILY_ALIAS,
        Some("medium"),
        EFFORT_MEDIUM_MODEL,
        "medium",
        false,
    )
    .await;
    assert_no_unsupported_upstream_plan(&harness, mode_label);
    // B 没有 low：本次请求必须回到 A，并重新按 A 的目录解析。
    run_native_effort_step(
        &harness,
        mode_label,
        "step3-low-only-on-a",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;

    // A 的 high 触发真实 scoped 429：先尝试 A，再由 B 服务。
    harness.set_generation_quota_error(true, EFFORT_HIGH_MODEL, 30);
    run_native_effort_failover_step(
        &harness,
        mode_label,
        "step4-high-scoped-429-fails-over",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
    )
    .await;
    let penalty = harness.read_penalty_snapshot(&harness.key_a_id).await;
    print_antigravity_runtime_quota_observation(
        mode_label,
        "step4-high-scoped-429-fails-over",
        Some(StatusCode::OK),
        harness.model_generation_counts(EFFORT_HIGH_MODEL, EFFORT_LOW_MODEL),
        &penalty,
    );
    assert_no_account_wide_penalty(&penalty, mode_label, "step4-high-scoped-429-fails-over");

    // high 的 scoped 429 不能阻断同账号 low；B 没有 low，只有 A 能服务。
    run_native_effort_step(
        &harness,
        mode_label,
        "step5-low-after-high-scoped-429",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    harness.clear_generation_quota_error(true, EFFORT_HIGH_MODEL);

    assert_no_unsupported_upstream_plan(&harness, mode_label);
    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
            Some(harness.key_a_id.as_str()),
            "{mode_label}: low 只能由 A 服务，sticky 必须重新绑定回 A"
        );
    }
}

#[test]
fn gateway_antigravity_dynamic_effort_quota_resolution_precedes_default_variant() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_dynamic_effort_quota_resolution_precedes_default_variant",
        gateway_antigravity_dynamic_effort_quota_resolution_precedes_default_variant_impl,
    );
}

async fn gateway_antigravity_dynamic_effort_quota_resolution_precedes_default_variant_impl() {
    run_quota_resolution_mode(false).await;
    run_quota_resolution_mode(true).await;
}

/// 先按请求档位解析实际上游模型，再做账号额度过滤：默认 high 在 A 上耗尽或
/// 完全未被发现时，low 仍必须由 A 立即服务（B 没有 low，若错误地按默认 high
/// 过滤将直接 503）；high 请求则只按解析后的 high 做账号级额度判断。
async fn run_quota_resolution_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let harness = start_antigravity_http_harness(dynamic_effort_http_config(
        "quota-resolution",
        mode_label,
        pool_enabled,
        false,
        high_low_models(0.9, 0.9),
        effort_models(&[(EFFORT_HIGH_MODEL, 0.9)]),
    ))
    .await;
    harness.materialize_pool_scores().await;

    run_native_effort_step(
        &harness,
        mode_label,
        "step1-high-warm",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_native_effort_step(
        &harness,
        mode_label,
        "step2-low-warm",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;

    // A 的 high 已被真实额度耗尽，low 仍有额度。
    harness.set_account_models(true, high_low_models(0.0, 0.9));
    let refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        refresh["success"],
        json!(2),
        "{mode_label}: refresh should succeed"
    );
    assert_eq!(
        refresh["failed"],
        json!(0),
        "{mode_label}: refresh should fail none"
    );

    run_native_effort_step(
        &harness,
        mode_label,
        "step3-low-despite-exhausted-default-high",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    // 解析后的 high 才是额度过滤对象：A 的 high 耗尽时只跳过 A，B 服务 high。
    run_native_effort_step(
        &harness,
        mode_label,
        "step4-high-quota-filtered-to-b",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
        false,
    )
    .await;

    // A 的目录完全不再包含 high（静态默认映射的档位缺失），low 仍可由 A 服务。
    harness.set_account_models(true, effort_models(&[(EFFORT_LOW_MODEL, 0.9)]));
    let refresh = harness.send_refresh_quota(None).await;
    assert_eq!(
        refresh["success"],
        json!(2),
        "{mode_label}: refresh should succeed"
    );
    assert_eq!(
        refresh["failed"],
        json!(0),
        "{mode_label}: refresh should fail none"
    );

    run_native_effort_step(
        &harness,
        mode_label,
        "step5-low-despite-missing-default-high",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;

    assert_no_unsupported_upstream_plan(&harness, mode_label);
}

#[test]
fn gateway_antigravity_dynamic_effort_converted_chat_and_responses() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_dynamic_effort_converted_chat_and_responses",
        gateway_antigravity_dynamic_effort_converted_chat_and_responses_impl,
    );
}

async fn gateway_antigravity_dynamic_effort_converted_chat_and_responses_impl() {
    run_converted_formats_mode(false).await;
    run_converted_formats_mode(true).await;
}

/// 开启 Provider 格式转换后，Chat `reasoning_effort` 与 Responses
/// `reasoning.effort` 仍按请求档位选择最终上游模型；xhigh/max 只在上游 envelope
/// 中规范化为 high，客户端各自完成文本字段与最终模型一致。
async fn run_converted_formats_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let harness = start_antigravity_http_harness(dynamic_effort_http_config(
        "converted",
        mode_label,
        pool_enabled,
        true,
        all_variant_models(0.9),
        high_medium_models(0.9),
    ))
    .await;
    harness.materialize_pool_scores().await;

    run_converted_chat_step(
        &harness,
        mode_label,
        "chat-low",
        "low",
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    run_converted_chat_step(
        &harness,
        mode_label,
        "chat-xhigh-normalizes-high",
        "xhigh",
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    run_converted_responses_step(
        &harness,
        mode_label,
        "responses-low",
        "low",
        EFFORT_LOW_MODEL,
        "low",
        true,
    )
    .await;
    run_converted_responses_step(
        &harness,
        mode_label,
        "responses-max-normalizes-high",
        "max",
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;

    assert_no_unsupported_upstream_plan(&harness, mode_label);
}

#[test]
fn gateway_antigravity_dynamic_effort_key_allowed_models_respect_resolved_variant() {
    run_gemini_cli_sync_test(
        "gateway_antigravity_dynamic_effort_key_allowed_models_respect_resolved_variant",
        gateway_antigravity_dynamic_effort_key_allowed_models_respect_resolved_variant_impl,
    );
}

async fn gateway_antigravity_dynamic_effort_key_allowed_models_respect_resolved_variant_impl() {
    run_key_allowed_models_mode(false).await;
    run_key_allowed_models_mode(true).await;
}

/// 账号级强约束必须在“按 effort 解析后的实际模型”上生效，而不能在枚举阶段拿
/// 静态默认映射（high）去预筛受限账号：
/// - A 允许 high：高档请求仍由 A 服务；
/// - A 不允许 low：正文 low 解析出的 low 不在 A 列表里，必须跳过 A；
/// - B 只允许 low：带静态 high 映射的别名不能被枚举阶段直接拒绝，必须交给
///   强校验按实际选择的 low 重新判定，最终由 B 服务。
async fn run_key_allowed_models_mode(pool_enabled: bool) {
    let mode_label = if pool_enabled { "pool" } else { "ordinary" };
    let mut config = dynamic_effort_http_config(
        "key-allowed-models",
        mode_label,
        pool_enabled,
        false,
        all_variant_models(0.9),
        all_variant_models(0.9),
    );
    config.key_a.allowed_models = Some(vec![EFFORT_HIGH_MODEL.to_string()]);
    config.key_b.allowed_models = Some(vec![EFFORT_LOW_MODEL.to_string()]);
    let harness = start_antigravity_http_harness(config).await;
    harness.materialize_pool_scores().await;

    // 静态映射与 A 的允许列表一致：高档请求仍由 A 服务，B 的 low-only 限制
    // 也不能让 B 抢占 high。
    run_native_effort_step(
        &harness,
        mode_label,
        "step1-high-allowed-on-a",
        EFFORT_FAMILY_ALIAS,
        Some("high"),
        EFFORT_HIGH_MODEL,
        "high",
        true,
    )
    .await;
    // A 解析出的 low 不被允许；B 的静态默认 high 虽不在其列表里，但目标档位
    // low 是允许的，枚举必须延迟到强校验后由 B 服务。
    run_native_effort_step(
        &harness,
        mode_label,
        "step2-low-not-allowed-on-a",
        EFFORT_FAMILY_ALIAS,
        Some("low"),
        EFFORT_LOW_MODEL,
        "low",
        false,
    )
    .await;

    assert_no_unsupported_upstream_plan(&harness, mode_label);
    if pool_enabled {
        assert_eq!(
            harness.read_sticky().await.as_deref(),
            Some(harness.key_b_id.as_str()),
            "{mode_label}: low 必须绕过 A 并让 sticky 重新绑定到 B"
        );
    }
}
