use serde_json::{json, Value};

/// 解释模型/分组额度的显式比例及上游耗尽时省略零值的 Timestamp 形态。
pub fn antigravity_quota_info_remaining_fraction(
    quota_info: &serde_json::Map<String, Value>,
) -> Option<f64> {
    if let Some(value) = quota_info
        .get("remainingFraction")
        .or_else(|| quota_info.get("remaining_fraction"))
    {
        // 显式无效值不等于省略，不能借 resetTime 将其误判为耗尽。
        return value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
            .filter(|fraction| fraction.is_finite())
            .map(|fraction| fraction.clamp(0.0, 1.0));
    }
    // 上游零值省略时只保留 resetTime；到期恢复由调度判断，不在解析时丢弃观察。
    let reset = quota_info
        .get("resetTime")
        .or_else(|| quota_info.get("reset_time"))?
        .as_str()?
        .trim();
    chrono::DateTime::parse_from_rfc3339(reset)
        .ok()
        .filter(|deadline| deadline.timestamp() >= 0)
        .map(|_| 0.0)
}

const ACCOUNT_INFO_TTL_SECONDS: u64 = 15 * 60;
const ACCOUNT_INFO_FAILURE_COOLDOWN_SECONDS: u64 = 60;
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AntigravityCodeAssistInfo {
    pub project_id: Option<String>,
    pub subscription_tier: Option<String>,
    pub paid_tier_id: Option<String>,
    pub current_tier_id: Option<String>,
}

/// 标准化 Antigravity 套餐标识：
/// - ultra / helium -> "ultra"
/// - free / starter -> "free"
/// - pro / premium / advanced -> "pro"
/// - standard-tier 等未知标识不映射为 Free，保留为 None (未知)
fn normalize_antigravity_subscription_tier(token: &str) -> Option<&'static str> {
    let has_word = |word: &str| {
        token
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.eq_ignore_ascii_case(word))
    };
    if has_word("ultra") || has_word("helium") {
        Some("ultra")
    } else if has_word("free") || has_word("starter") {
        Some("free")
    } else if has_word("pro") || has_word("premium") || has_word("advanced") {
        Some("pro")
    } else {
        None
    }
}

fn extract_antigravity_tier_identifier(tier_value: Option<&serde_json::Value>) -> Option<&str> {
    let tier_value = tier_value?;
    tier_value
        .as_str()
        .into_iter()
        .chain(
            ["id", "name"]
                .into_iter()
                .filter_map(|field| tier_value.get(field)?.as_str()),
        )
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn extract_antigravity_companion_project_id(value: &serde_json::Value) -> Option<String> {
    let raw = value
        .get("cloudaicompanionProject")
        .or_else(|| value.get("cloudAiCompanionProject"))?;
    if let Some(project_id) = raw
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(project_id.to_string());
    }
    raw.as_object()
        .and_then(|object| {
            object
                .get("id")
                .or_else(|| object.get("project_id"))
                .or_else(|| object.get("projectId"))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// 解析 Antigravity loadCodeAssist 响应；
/// 权威判定规则：
/// 1. 优先读取 paidTier，非空 id 优先于 name；
/// 2. paidTier 存在有效标识但无法识别时（如 standard-tier），作为权威未知，不得回退至 currentTier；
/// 3. paidTier 缺失或 id/name 均为空白时，回退至 currentTier；
/// 4. 严禁从 allowedTiers 或可用模型列表推断套餐；
/// 5. 任何未知或缺失套餐证据均判定为未知（None），严禁默认判定为 Free。
pub fn parse_antigravity_code_assist_response(
    value: &serde_json::Value,
) -> Option<AntigravityCodeAssistInfo> {
    let object = value.as_object()?;
    if object.get("error").is_some() {
        return None;
    }
    let project_id = extract_antigravity_companion_project_id(value);
    let paid_tier_raw = extract_antigravity_tier_identifier(object.get("paidTier"));
    let subscription_tier = paid_tier_raw
        .or_else(|| extract_antigravity_tier_identifier(object.get("currentTier")))
        .and_then(normalize_antigravity_subscription_tier)
        .map(ToOwned::to_owned);
    Some(AntigravityCodeAssistInfo {
        project_id,
        subscription_tier,
        paid_tier_id: raw_tier_id(object.get("paidTier")).map(ToOwned::to_owned),
        current_tier_id: raw_tier_id(object.get("currentTier")).map(ToOwned::to_owned),
    })
}

fn raw_tier_id(tier: Option<&Value>) -> Option<&str> {
    let tier = tier?;
    tier.as_str()
        .or_else(|| tier.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// 失败只更新尝试状态，避免用失败请求覆盖最后一次真实套餐观察。
pub fn antigravity_account_observation_update(
    info: Option<&AntigravityCodeAssistInfo>,
    now: u64,
) -> Value {
    let mut observation = json!({
        "account_info_status": if info.is_some() { "success" } else { "error" },
        "account_info_checked_at": now,
    });
    if let Some(info) = info {
        let object = observation
            .as_object_mut()
            .expect("observation is an object");
        object.insert("subscription_tier".into(), json!(info.subscription_tier));
        object.insert("paid_tier_id".into(), json!(info.paid_tier_id));
        object.insert("current_tier_id".into(), json!(info.current_tier_id));
        object.insert("account_info_updated_at".into(), json!(now));
        if let Some(project) = &info.project_id {
            object.insert("project_id".into(), json!(project));
        }
    }
    json!({ "antigravity": observation })
}

/// 成功未知同样缓存；失败短暂退避，旧账号缺少观察时立即补查。
pub fn antigravity_account_observation_due(metadata: Option<&Value>, now: u64) -> bool {
    let Some(observation) = metadata.and_then(|value| value.get("antigravity")) else {
        return true;
    };
    let ttl = match observation
        .get("account_info_status")
        .and_then(Value::as_str)
    {
        Some("success") => ACCOUNT_INFO_TTL_SECONDS,
        Some("error") => ACCOUNT_INFO_FAILURE_COOLDOWN_SECONDS,
        _ => return true,
    };
    observation
        .get("account_info_checked_at")
        .and_then(Value::as_u64)
        .and_then(|checked| now.checked_sub(checked))
        .is_none_or(|elapsed| elapsed >= ttl)
}

/// 权益只能来自上游原始 ID，不能从展示等级反推 Google 协议值。
pub fn antigravity_entitlement_candidates(metadata: Option<&Value>) -> Vec<Option<String>> {
    let mut candidates = Vec::with_capacity(3);
    if let Some(observation) = metadata.and_then(|value| value.get("antigravity")) {
        for field in ["paid_tier_id", "current_tier_id"] {
            if let Some(id) = observation
                .get(field)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| {
                    !id.is_empty()
                        && !id.eq_ignore_ascii_case("free-tier")
                        && !id.eq_ignore_ascii_case("legacy-tier")
                })
            {
                if !candidates
                    .iter()
                    .any(|candidate: &Option<String>| candidate.as_deref() == Some(id))
                {
                    candidates.push(Some(id.to_owned()));
                }
            }
        }
    }
    candidates.push(None);
    candidates
}

pub fn build_antigravity_fetch_available_models_body(
    project_id: &str,
    tier: Option<&str>,
) -> Value {
    let mut body = json!({ "project": project_id });
    if let Some(tier) = tier.map(str::trim).filter(|tier| !tier.is_empty()) {
        body["entitlement"] = json!({ "userTier": tier });
    }
    body
}

/// 携带权益参数的 400 INVALID_ARGUMENT 按 Magpie 兼容策略降级；认证、限流与传输错误不能触发套餐重试。
pub fn is_antigravity_entitlement_rejection(status: u16, body: &Value) -> bool {
    let Some(error) = body.get("error") else {
        return false;
    };
    if status == 400 && error.get("status").and_then(Value::as_str) == Some("INVALID_ARGUMENT") {
        return true;
    }
    if status != 412 {
        return false;
    }
    if error.get("status").and_then(Value::as_str) != Some("FAILED_PRECONDITION") {
        return false;
    }
    let mentions_tier = |text: &str| {
        text.split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|word| {
                word.eq_ignore_ascii_case("entitlement")
                    || word.eq_ignore_ascii_case("userTier")
                    || word.eq_ignore_ascii_case("tier")
            })
    };
    error
        .get("message")
        .and_then(Value::as_str)
        .is_some_and(mentions_tier)
        || error
            .get("details")
            .and_then(Value::as_array)
            .is_some_and(|details| {
                details.iter().any(|detail| {
                    detail
                        .get("fieldViolations")
                        .and_then(Value::as_array)
                        .is_some_and(|violations| {
                            violations.iter().any(|violation| {
                                ["field", "description"].iter().any(|field| {
                                    violation
                                        .get(field)
                                        .and_then(Value::as_str)
                                        .is_some_and(mentions_tier)
                                })
                            })
                        })
                })
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_quota_info_explicit_fractions_are_finite_and_clamped() {
        for (value, expected) in [
            (json!(0.75), 0.75),
            (json!(" 0.25 "), 0.25),
            (json!(0), 0.0),
            (json!(-0.5), 0.0),
            (json!("1.5"), 1.0),
        ] {
            let info = json!({"remainingFraction": value});
            assert_eq!(
                antigravity_quota_info_remaining_fraction(info.as_object().unwrap()),
                Some(expected)
            );
        }
    }

    #[test]
    fn antigravity_quota_info_normalized_aliases_preserve_explicit_precedence() {
        for (info, expected) in [
            (json!({"remaining_fraction": "0.6"}), Some(0.6)),
            (json!({"reset_time": "2099-01-01T00:00:00Z"}), Some(0.0)),
            (
                json!({"remaining_fraction": null, "reset_time": "2099-01-01T00:00:00Z"}),
                None,
            ),
            (
                json!({"remainingFraction": null, "remaining_fraction": 0.7, "resetTime": "2099-01-01T00:00:00Z"}),
                None,
            ),
            (
                json!({"resetTime": null, "reset_time": "2099-01-01T00:00:00Z"}),
                None,
            ),
        ] {
            assert_eq!(
                antigravity_quota_info_remaining_fraction(info.as_object().unwrap()),
                expected,
                "{info}"
            );
        }
    }

    #[test]
    fn antigravity_quota_info_reset_only_requires_valid_nonnegative_timestamp() {
        for reset in [
            "2030-01-01T00:00:00Z",
            " 2000-01-01T00:00:00Z ",
            "1970-01-01T00:00:00Z",
        ] {
            let info = json!({"resetTime": reset});
            assert_eq!(
                antigravity_quota_info_remaining_fraction(info.as_object().unwrap()),
                Some(0.0)
            );
        }
        for info in [
            json!({}),
            json!({"displayName": "Gemini"}),
            json!({"resetTime": ""}),
            json!({"resetTime": "invalid"}),
            json!({"resetTime": "2030-02-30T00:00:00Z"}),
            json!({"resetTime": "1969-12-31T23:59:59Z"}),
            json!({"resetTime": 1893456000u64}),
            json!({"resetTime": true}),
            json!({"resetTime": {}}),
            json!({"resetTime": null}),
        ] {
            assert_eq!(
                antigravity_quota_info_remaining_fraction(info.as_object().unwrap()),
                None,
                "{info}"
            );
        }
    }

    #[test]
    fn antigravity_quota_info_explicit_invalid_fraction_never_falls_back() {
        for value in [
            json!(null),
            json!(""),
            json!(" "),
            json!("invalid"),
            json!("NaN"),
            json!("inf"),
            json!("-inf"),
            json!(true),
            json!({}),
            json!([]),
        ] {
            let info = json!({"remainingFraction": value, "resetTime": "2030-01-01T00:00:00Z"});
            assert_eq!(
                antigravity_quota_info_remaining_fraction(info.as_object().unwrap()),
                None,
                "{info}"
            );
        }
    }
    #[test]
    fn antigravity_tier_normalization_recognizes_aliases_and_rejects_unknown() {
        assert_eq!(
            normalize_antigravity_subscription_tier("ultra"),
            Some("ultra")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("helium"),
            Some("ultra")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("g1-ultra-tier"),
            Some("ultra")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("ULTRA"),
            Some("ultra")
        );

        assert_eq!(normalize_antigravity_subscription_tier("pro"), Some("pro"));
        assert_eq!(
            normalize_antigravity_subscription_tier("premium"),
            Some("pro")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("advanced"),
            Some("pro")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("g1-pro-tier"),
            Some("pro")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("  PRO  "),
            Some("pro")
        );

        assert_eq!(
            normalize_antigravity_subscription_tier("free"),
            Some("free")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("starter"),
            Some("free")
        );
        assert_eq!(
            normalize_antigravity_subscription_tier("free-tier"),
            Some("free")
        );

        // 未知套餐严禁推断为 Free
        assert_eq!(
            normalize_antigravity_subscription_tier("standard-tier"),
            None
        );
        assert_eq!(normalize_antigravity_subscription_tier("enterprise"), None);
        assert_eq!(
            normalize_antigravity_subscription_tier("professional-preview"),
            None
        );
        assert_eq!(normalize_antigravity_subscription_tier("freeform"), None);
        assert_eq!(normalize_antigravity_subscription_tier(""), None);
        assert_eq!(normalize_antigravity_subscription_tier("   "), None);
    }

    #[test]
    fn antigravity_code_assist_prefers_paid_tier_over_current_tier() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": {
                "id": "g1-pro-tier",
                "name": "Google One Pro"
            },
            "currentTier": {
                "id": "free-tier",
                "name": "Free Tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.project_id.as_deref(), Some("project-ant-1"));
        assert_eq!(info.subscription_tier.as_deref(), Some("pro"));
    }

    #[test]
    fn antigravity_code_assist_paid_id_takes_precedence_over_name() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": {
                "id": "g1-ultra-tier",
                "name": "Free Starter Name"
            },
            "currentTier": {
                "id": "free-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier.as_deref(), Some("ultra"));
    }

    #[test]
    fn antigravity_code_assist_blank_paid_id_falls_back_to_paid_name() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": {
                "id": "   ",
                "name": "Premium Tier"
            },
            "currentTier": {
                "id": "free-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier.as_deref(), Some("pro"));
    }

    #[test]
    fn antigravity_code_assist_blank_paid_id_and_name_falls_back_to_current_tier() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": {
                "id": "",
                "name": "   "
            },
            "currentTier": {
                "id": "free-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier.as_deref(), Some("free"));
    }

    #[test]
    fn antigravity_code_assist_unknown_paid_tier_blocks_fallback_to_free_current_tier() {
        // 当 paidTier 存在有效标识但无法映射时，作为权威未知处理，不得回退为 Free！
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": {
                "id": "standard-tier"
            },
            "currentTier": {
                "id": "free-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier, None);
    }

    #[test]
    fn antigravity_code_assist_missing_paid_tier_falls_back_to_current_tier() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "currentTier": {
                "id": "free-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier.as_deref(), Some("free"));

        let null_paid = json!({
            "cloudaicompanionProject": "project-ant-1",
            "paidTier": null,
            "currentTier": {
                "id": "g1-pro-tier"
            }
        });
        let info =
            parse_antigravity_code_assist_response(&null_paid).expect("response should parse");
        assert_eq!(info.subscription_tier.as_deref(), Some("pro"));
    }

    #[test]
    fn antigravity_code_assist_unknown_current_tier_does_not_guess_free() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "currentTier": {
                "id": "standard-tier"
            }
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier, None);
    }

    #[test]
    fn antigravity_code_assist_ignores_allowed_tiers_and_upgrade_menus() {
        let payload = json!({
            "cloudaicompanionProject": "project-ant-1",
            "allowedTiers": [
                { "id": "free-tier", "isDefault": true },
                { "id": "g1-pro-tier" }
            ]
        });
        let info = parse_antigravity_code_assist_response(&payload).expect("response should parse");
        assert_eq!(info.subscription_tier, None);
    }

    #[test]
    fn antigravity_code_assist_extracts_project_id_variants() {
        let from_str = json!({
            "cloudaicompanionProject": "project-str",
            "paidTier": { "id": "g1-pro-tier" }
        });
        assert_eq!(
            parse_antigravity_code_assist_response(&from_str).and_then(|info| info.project_id),
            Some("project-str".to_string())
        );

        let from_obj_id = json!({
            "cloudaicompanionProject": { "id": "project-obj-id" }
        });
        assert_eq!(
            parse_antigravity_code_assist_response(&from_obj_id).and_then(|info| info.project_id),
            Some("project-obj-id".to_string())
        );

        let from_camel_project_id = json!({
            "cloudAiCompanionProject": { "projectId": "project-camel-id" }
        });
        assert_eq!(
            parse_antigravity_code_assist_response(&from_camel_project_id)
                .and_then(|info| info.project_id),
            Some("project-camel-id".to_string())
        );
    }

    #[test]
    fn antigravity_code_assist_handles_error_and_empty() {
        let error_body = json!({
            "error": { "code": 403, "message": "The caller does not have permission" }
        });
        assert_eq!(parse_antigravity_code_assist_response(&error_body), None);

        let empty_body = json!({});
        let info = parse_antigravity_code_assist_response(&empty_body)
            .expect("empty json object should parse with None fields");
        assert_eq!(info.project_id, None);
        assert_eq!(info.subscription_tier, None);
    }

    #[test]
    fn raw_entitlements_are_not_inferred_from_display_names_or_upgrade_menus() {
        let info = parse_antigravity_code_assist_response(&json!({
            "paidTier": {"name": "Google AI Pro"},
            "currentTier": {"id": "standard-tier"},
            "allowedTiers": [{"id": "g1-ultra-tier"}]
        }))
        .unwrap();
        assert_eq!(info.subscription_tier.as_deref(), Some("pro"));
        assert_eq!(info.paid_tier_id, None);
        assert_eq!(info.current_tier_id.as_deref(), Some("standard-tier"));
        let metadata = antigravity_account_observation_update(Some(&info), 100);
        assert_eq!(
            antigravity_entitlement_candidates(Some(&metadata)),
            vec![Some("standard-tier".into()), None]
        );
    }

    #[test]
    fn entitlement_order_deduplicates_and_excludes_free_identifiers() {
        for (paid, current, expected) in [
            (
                "g1-pro-tier",
                "standard-tier",
                vec![
                    Some("g1-pro-tier".into()),
                    Some("standard-tier".into()),
                    None,
                ],
            ),
            (
                "g1-pro-tier",
                "g1-pro-tier",
                vec![Some("g1-pro-tier".into()), None],
            ),
            ("free-tier", "legacy-tier", vec![None]),
            (" ", "free-tier", vec![None]),
        ] {
            let metadata =
                json!({"antigravity": {"paid_tier_id": paid, "current_tier_id": current}});
            assert_eq!(
                antigravity_entitlement_candidates(Some(&metadata)),
                expected
            );
        }
        assert_eq!(
            antigravity_entitlement_candidates(Some(
                &json!({"antigravity":{"subscription_tier":"pro"}})
            )),
            vec![None]
        );
    }

    #[test]
    fn account_observation_ttl_distinguishes_unknown_success_and_failure() {
        assert!(antigravity_account_observation_due(None, 1000));
        let unknown = antigravity_account_observation_update(
            Some(&AntigravityCodeAssistInfo::default()),
            1000,
        );
        assert!(!antigravity_account_observation_due(Some(&unknown), 1899));
        assert!(antigravity_account_observation_due(Some(&unknown), 1900));
        assert!(antigravity_account_observation_due(Some(&unknown), 999));
        let failure = antigravity_account_observation_update(None, 1000);
        assert!(!antigravity_account_observation_due(Some(&failure), 1059));
        assert!(antigravity_account_observation_due(Some(&failure), 1060));
        assert!(failure["antigravity"].get("subscription_tier").is_none());
        assert_eq!(unknown["antigravity"]["subscription_tier"], Value::Null);
    }

    #[test]
    fn entitlement_rejection_does_not_retry_unrelated_failures() {
        let refusal = json!({"error":{"status":"FAILED_PRECONDITION", "message":"Invalid entitlement userTier"}});
        assert!(!is_antigravity_entitlement_rejection(400, &refusal));
        assert!(is_antigravity_entitlement_rejection(412, &refusal));
        for status in [401, 403, 429, 500] {
            assert!(!is_antigravity_entitlement_rejection(status, &refusal));
        }

        assert!(is_antigravity_entitlement_rejection(
            400,
            &json!({"error":{"status":"INVALID_ARGUMENT","message":"Request contains an invalid argument."}})
        ));
        assert!(is_antigravity_entitlement_rejection(
            400,
            &json!({"error":{"status":"INVALID_ARGUMENT","message":"Invalid project"}})
        ));
        assert!(!is_antigravity_entitlement_rejection(
            400,
            &json!({"error":{"status":"PERMISSION_DENIED","message":"Invalid entitlement"}})
        ));
        assert!(is_antigravity_entitlement_rejection(
            400,
            &json!({"error":{"status":"INVALID_ARGUMENT","details":[{"fieldViolations":[{"field":"entitlement.userTier"}]}]}})
        ));
    }
}
