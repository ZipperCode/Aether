mod antigravity;

use std::time::{SystemTime, UNIX_EPOCH};

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use aether_provider_transport::antigravity::{
    antigravity_model_thinking_variant, AntigravityThinkingLevel,
};
use serde_json::{json, Map, Value};

use crate::provider::ProviderPoolMemberInput;
use crate::quota_sources::{source_account_exhausted, source_balance_below_minimum};
use crate::service::ProviderPoolService;

/// 标准余额快照的固定内部调度下限；各币种独立比较，不做汇率换算。
const PROVIDER_POOL_MINIMUM_SCHEDULABLE_BALANCE: f64 = 1.0;

/// 在没有请求模型上下文时判断 Key 的账号级可恢复额度是否耗尽。
pub fn provider_pool_key_account_quota_exhausted(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> bool {
    let adapter = ProviderPoolService::with_builtin_adapters().adapter(provider_type);
    adapter.quota_exhausted(&ProviderPoolMemberInput {
        provider_type,
        key,
        auth_config: None,
        provider_model_name: None,
    })
}

/// 判断 Key 的标准余额快照是否明确低于可调度下限。
///
/// 只有 fresh、非无限额度、余额非空且每项单位和有限数值都有效时才会阻断；
/// 任何未知或陈旧输入均放行，避免上游刷新失败后永久饿死 Key。
pub fn provider_pool_key_balance_below_minimum(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> bool {
    let Some(quota_snapshot) = provider_pool_member_quota_snapshot(key, provider_type) else {
        return false;
    };
    if let Some(below_minimum) =
        source_balance_below_minimum(quota_snapshot, PROVIDER_POOL_MINIMUM_SCHEDULABLE_BALANCE)
    {
        return below_minimum;
    }
    // 旧 OpenRouter 剩余额度可能由累计消费反推，旧智谱余额可能来自错误码补零。
    // 在正常刷新提供新来源证据前，这两类旧金额不参与自动阻断。
    if matches!(
        provider_type.trim().to_ascii_lowercase().as_str(),
        "openrouter" | "zhipu" | "minimax"
    ) {
        return false;
    }
    if !quota_snapshot
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("balance"))
        || !quota_snapshot
            .get("freshness")
            .and_then(Value::as_str)
            .is_some_and(|freshness| freshness.trim().eq_ignore_ascii_case("fresh"))
    {
        return false;
    }
    if quota_snapshot
        .get("unlimited")
        .is_some_and(|value| provider_pool_json_bool(Some(value)) != Some(false))
    {
        return false;
    }

    let Some(balances) = quota_snapshot
        .get("balances")
        .and_then(Value::as_array)
        .filter(|balances| !balances.is_empty())
    else {
        return false;
    };
    balances.iter().all(|balance| {
        let Some(balance) = balance.as_object() else {
            return false;
        };
        balance
            .get("unit")
            .and_then(Value::as_str)
            .is_some_and(|unit| !unit.trim().is_empty())
            && provider_pool_json_f64(balance.get("available"))
                .is_some_and(|available| available <= PROVIDER_POOL_MINIMUM_SCHEDULABLE_BALANCE)
    })
}

/// 判断 Key 是否命中 Provider 适配器声明的硬额度阻断。
///
/// 硬阻断独立于可配置的订阅耗尽开关，供 Pool 与普通候选路径统一消费。
pub fn provider_pool_key_quota_hard_blocked(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> bool {
    if provider_pool_key_runtime_quota_blocked(key) {
        return true;
    }
    let adapter = ProviderPoolService::with_builtin_adapters().adapter(provider_type);
    adapter.quota_hard_blocked(&ProviderPoolMemberInput {
        provider_type,
        key,
        auth_config: None,
        provider_model_name: None,
    })
}

/// 按模型查询 Provider 硬额度状态；需人工恢复的运行时阻断始终作用于整个 Key。
/// 只有 Provider 自身可恢复的账号信号才允许被可用模型桶覆盖。
pub fn provider_pool_key_model_quota_hard_blocked(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: &str,
) -> bool {
    if provider_pool_key_runtime_quota_blocked(key) {
        return true;
    }
    let adapter = ProviderPoolService::with_builtin_adapters().adapter(provider_type);
    adapter.quota_hard_blocked(&ProviderPoolMemberInput {
        provider_type,
        key,
        auth_config: None,
        provider_model_name: Some(provider_model_name),
    })
}

/// 判断 Antigravity Key 的已发现模型是否包含指定上游模型。
///
/// 该判断只补充账号级模型发现事实，不替代 scheduler 现有的 allowlist 与映射规则。
/// 只读取 `upstream_metadata.antigravity` 中首个对象形式的模型映射
/// （`quota_by_model`、`models`、`model_quotas`），空对象同样权威；显式
/// `disabled: true` 的同名条目不算可用。模型标识按 trim 后的完整
/// `eq_ignore_ascii_case` 匹配，不做模型族或后缀推断。映射缺失或格式不符时保留
/// 发现前的既有行为，其他 Provider 类型始终不受影响。
pub fn provider_pool_key_supports_discovered_model(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: &str,
) -> bool {
    if !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return true;
    }
    let Some(bucket) = key
        .upstream_metadata
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return true;
    };
    let Some(models) = provider_pool_authoritative_model_maps(bucket, true).next() else {
        return true;
    };
    let requested = provider_model_name.trim();
    models.iter().any(|(model_name, entry)| {
        let model_name = model_name.trim();
        !model_name.is_empty()
            && model_name.eq_ignore_ascii_case(requested)
            && !provider_pool_model_entry_disabled(entry)
    })
}

fn provider_pool_model_entry_disabled(entry: &Value) -> bool {
    entry
        .as_object()
        .is_some_and(|object| provider_pool_json_bool(object.get("disabled")) == Some(true))
}

/// 按账号权威模型映射，把默认选中的 Antigravity 模型解析为请求 thinking 档位对应的
/// 实际已发现模型。
///
/// 只读取当前 Key `upstream_metadata.antigravity` 的首个对象形式模型映射
/// （`quota_by_model`、`models`、`model_quotas`），空对象同样权威；显式
/// `disabled: true` 的条目不算可用。客户端模型名上的已识别 thinking 后缀优先于
/// 请求体 effort，只有与选中模型族完全相同的精确已发现身份才直接沿用；其余情况
/// 只在该族内选择请求档位，缺失档位时拒绝，绝不跨账号、跨族或自造模型标识。
/// 无 effort 时保留默认映射；映射缺失或格式不符时保留既有基础模型行为。
/// 解析结果一旦改变默认上游模型，还必须仍被该 Key 的强 `allowed_models` 允许
/// （或客户端显式请求的别名本身就被允许），避免只授权 high 的 Key 被低档请求借用；
/// 对网关延迟行级 allowlist 的 effort 敏感 Gemini 请求，即使解析结果仍是默认映射，
/// 也必须命中解析目标或显式别名。非 effort 敏感请求及其他 Provider 保持枚举语义。
pub fn resolve_provider_pool_key_thinking_model<'a>(
    key: &'a StoredProviderCatalogKey,
    provider_type: &str,
    requested_model: &str,
    selected_provider_model: &'a str,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> Option<&'a str> {
    let resolved = provider_pool_resolve_thinking_model(
        key,
        provider_type,
        requested_model,
        selected_provider_model,
        thinking_level,
    )?;
    // 网关只为 Antigravity 的 Gemini thinking 请求延迟行级 allowlist，因此这些
    // 请求必须由这里的强 Key allowlist 做最终判定；其余路径沿用枚举结果。
    let effort_sensitive = provider_type.trim().eq_ignore_ascii_case("antigravity")
        && provider_pool_antigravity_thinking_model(selected_provider_model.trim())
        && (thinking_level.is_some()
            || antigravity_model_thinking_variant(requested_model.trim()).is_some());
    if !effort_sensitive {
        // 非 effort 敏感请求仍由既有枚举负责 allowlist；其他 Provider 不受影响。
        return Some(resolved);
    }
    provider_pool_key_allows_resolved_thinking_model(key, requested_model, resolved)
        .then_some(resolved)
}

fn provider_pool_resolve_thinking_model<'a>(
    key: &'a StoredProviderCatalogKey,
    provider_type: &str,
    requested_model: &str,
    selected_provider_model: &'a str,
    thinking_level: Option<AntigravityThinkingLevel>,
) -> Option<&'a str> {
    if !provider_type.trim().eq_ignore_ascii_case("antigravity") {
        return Some(selected_provider_model);
    }
    let Some(bucket) = provider_pool_metadata_bucket(key.upstream_metadata.as_ref(), provider_type)
    else {
        return Some(selected_provider_model);
    };
    // 缺失或格式不正确的映射保留发现前的既有行为。
    let Some(models) = provider_pool_authoritative_model_maps(bucket, true).next() else {
        return Some(selected_provider_model);
    };

    let selected = selected_provider_model.trim();
    if !provider_pool_antigravity_thinking_model(selected) {
        return Some(selected_provider_model);
    }
    // 权威空映射没有可服务的模型。
    if models.is_empty() {
        return None;
    }

    let selected_variant = antigravity_model_thinking_variant(selected);
    let family = selected_variant.map(|(base, _)| base).unwrap_or(selected);
    let client_variant = antigravity_model_thinking_variant(requested_model.trim());

    // 客户端显式后缀命中同族完整身份时直接沿用该身份，包括其禁用状态。
    let exact_client_variant = client_variant
        .filter(|(base, _)| base.eq_ignore_ascii_case(family))
        .and_then(|_| {
            models.iter().find(|(model_name, _)| {
                model_name
                    .trim()
                    .eq_ignore_ascii_case(requested_model.trim())
            })
        });
    if let Some((model_name, entry)) = exact_client_variant {
        return (!provider_pool_model_entry_disabled(entry)).then_some(model_name.trim());
    }

    // 只有客户端显式后缀才覆盖请求体 effort；默认映射上的后缀不是客户端选择。
    let Some(target) = client_variant.map(|(_, level)| level).or(thinking_level) else {
        return Some(selected_provider_model);
    };

    let variants_exist = provider_pool_antigravity_has_thinking_variants(models, family);
    if !variants_exist {
        // 基础模型既无映射后缀也无已发现同族变体时保留既有行为；带后缀的默认
        // 映射缺少任何已发现兄弟时，该账号不满足请求。
        return selected_variant
            .is_none()
            .then_some(selected_provider_model);
    }

    match target {
        // minimal 先取已发现 minimal；没有 minimal 时才使用 low 兼容档，
        // 已存在但禁用的 minimal 不得被绕过。
        AntigravityThinkingLevel::Minimal => {
            match provider_pool_pick_thinking_variant(
                models,
                family,
                AntigravityThinkingLevel::Minimal,
            ) {
                Some(found) => found,
                None => provider_pool_pick_thinking_variant(
                    models,
                    family,
                    AntigravityThinkingLevel::Low,
                )
                .flatten(),
            }
        }
        AntigravityThinkingLevel::Low
        | AntigravityThinkingLevel::Medium
        | AntigravityThinkingLevel::High => {
            provider_pool_pick_thinking_variant(models, family, target).flatten()
        }
    }
}

/// Antigravity effort 敏感请求的解析目标必须被 Key 的强 `allowed_models` 允许。
/// 未限制（`None`）放行；空策略列表拒绝全部；非空名单中的解析目标本身或客户端
/// 显式请求的别名任一命中即可，保留“别名授权映射模型”的既有语义。
fn provider_pool_key_allows_resolved_thinking_model(
    key: &StoredProviderCatalogKey,
    requested_model: &str,
    resolved_model: &str,
) -> bool {
    let Some(allowed_models) = key.allowed_models.as_ref() else {
        return true;
    };
    provider_pool_allowed_model_list_decision(allowed_models, resolved_model, requested_model)
        == Some(true)
}

/// 返回 `None` 表示策略列表没有任何有效条目（空列表，拒绝全部）；
/// `Some(true/false)` 表示列表非空且解析目标或显式客户端别名是否命中。
fn provider_pool_allowed_model_list_decision(
    allowed_models: &Value,
    resolved_model: &str,
    requested_model: &str,
) -> Option<bool> {
    let resolved_model = resolved_model.trim();
    let requested_model = requested_model.trim();
    let mut saw_item = false;
    let matched = provider_pool_allowed_model_list_any(allowed_models, &mut |candidate| {
        saw_item = true;
        (!resolved_model.is_empty() && candidate == resolved_model)
            || (!requested_model.is_empty() && candidate == requested_model)
    });
    if matched {
        Some(true)
    } else if saw_item {
        Some(false)
    } else {
        None
    }
}

/// 遍历 Key 策略列表的既有输入形态：字符串数组、历史内嵌 JSON 字符串，以及无法
/// 解码时作为单一模型名。空字符串条目不参与匹配。
fn provider_pool_allowed_model_list_any<F>(allowed_models: &Value, predicate: &mut F) -> bool
where
    F: FnMut(&str) -> bool,
{
    match allowed_models {
        Value::Array(items) => items.iter().any(|item| {
            item.as_str().is_some_and(|candidate| {
                let candidate = candidate.trim();
                !candidate.is_empty() && predicate(candidate)
            })
        }),
        Value::String(raw) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return false;
            }
            if let Ok(decoded) = serde_json::from_str::<Value>(raw) {
                return match decoded {
                    Value::Array(_) | Value::String(_) => {
                        provider_pool_allowed_model_list_any(&decoded, &mut *predicate)
                    }
                    _ => false,
                };
            }
            predicate(raw)
        }
        _ => false,
    }
}

/// Antigravity 只有 `gemini-` 模型标识带可识别的 thinking 后缀。
fn provider_pool_antigravity_thinking_model(model: &str) -> bool {
    model
        .get(.."gemini-".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("gemini-"))
}

/// 是否存在仅剥离已识别终结后缀后与目标族完全相同的已发现 thinking 变体。
fn provider_pool_antigravity_has_thinking_variants(
    models: &Map<String, Value>,
    family: &str,
) -> bool {
    models.keys().any(|model_name| {
        antigravity_model_thinking_variant(model_name.trim())
            .is_some_and(|(base, _)| base.eq_ignore_ascii_case(family))
    })
}

/// 在已发现同族变体中选择指定档位：`None` 表示该档位不存在，`Some(None)` 表示
/// 存在但全部被禁用（调用方必须拒绝，不得回退到其他档位）。
fn provider_pool_pick_thinking_variant<'a>(
    models: &'a Map<String, Value>,
    family: &str,
    level: AntigravityThinkingLevel,
) -> Option<Option<&'a str>> {
    let mut present = false;
    for (model_name, entry) in models {
        let model_name = model_name.trim();
        let Some((base, candidate_level)) = antigravity_model_thinking_variant(model_name) else {
            continue;
        };
        if !base.eq_ignore_ascii_case(family) || candidate_level != level {
            continue;
        }
        present = true;
        if !provider_pool_model_entry_disabled(entry) {
            return Some(Some(model_name));
        }
    }
    present.then_some(None)
}

/// 判断运行时错误分类是否为当前 Key 写入了需管理员恢复的持久额度阻断。
///
/// 该状态属于 Key 级强事实，不随请求模型切换而解除。
pub fn provider_pool_key_runtime_quota_blocked(key: &StoredProviderCatalogKey) -> bool {
    key.status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("scheduling"))
        .and_then(Value::as_object)
        .is_some_and(|scheduling| {
            scheduling.get("blocked").and_then(Value::as_bool) == Some(true)
                && scheduling
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| code.eq_ignore_ascii_case("quota_exhausted"))
        })
}

pub fn provider_pool_member_quota_snapshot<'a>(
    key: &'a StoredProviderCatalogKey,
    provider_type: &str,
) -> Option<&'a Map<String, Value>> {
    let quota_snapshot = key
        .status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("quota"))
        .and_then(Value::as_object)?;
    provider_pool_quota_snapshot_matches_provider(quota_snapshot, provider_type)
        .then_some(quota_snapshot)
}

/// 分组身份只来自上游 ID 或完整桶 ID 集，不由展示名、模型前缀或套餐推断。
pub fn antigravity_quota_group_key(group: &Map<String, Value>) -> Option<String> {
    if let Some(id) = group
        .get("group_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Some(format!("id:{id}"));
    }
    let mut ids = group
        .get("buckets")?
        .as_array()?
        .iter()
        .filter_map(|bucket| bucket.get("bucket_id")?.as_str().map(str::trim))
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    (!ids.is_empty()).then(|| {
        format!(
            "buckets:{}",
            serde_json::to_string(&ids).expect("string IDs serialize")
        )
    })
}

/// 按上游模型、模型族或分组窗口解析可恢复额度；账号状态与较新的原始元数据共同参与。
/// 找不到模型桶时返回 None；自动归属组先按任一启用窗口耗尽阻断，独立模型窗口保持均耗尽规则。
///
/// Providers are free to expose quota windows in different shapes.  Newer
/// snapshots should put an explicit `model`/`models` (or `quota_group`) on a
/// window; legacy Codex snapshots use a family prefix in `code` (for example
/// `spark_5h`).  We deliberately do not name any product or model here: the
/// resolver compares the metadata supplied by the provider with the selected
/// model and only falls back to account-level exhaustion when no model bucket
/// can be identified.
pub(crate) fn provider_pool_model_quota_exhausted(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: &str,
) -> Option<bool> {
    if provider_type.trim().eq_ignore_ascii_case("antigravity")
        && antigravity::model_quota_exhausted(key, provider_model_name)
    {
        return Some(true);
    }
    // 官方来源已经包含套餐归属，不能再由旧扁平模型/通用窗口覆盖保守的账号判断。
    if let Some(snapshot) = provider_pool_member_quota_snapshot(key, provider_type) {
        if let Some(exhausted) = source_account_exhausted(
            snapshot,
            provider_pool_current_unix_secs().unwrap_or_default(),
        ) {
            return Some(exhausted);
        }
        if matches!(
            provider_type.trim().to_ascii_lowercase().as_str(),
            "zhipu" | "zai" | "kimi_coding" | "minimax"
        ) {
            return Some(false);
        }
    }
    provider_pool_quota_reaches_reserve(key, provider_type, Some(provider_model_name), 0.0)
}

fn provider_pool_quota_reaches_reserve(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: Option<&str>,
    reserve_ratio: f64,
) -> Option<bool> {
    let is_codex = provider_type.trim().eq_ignore_ascii_case("codex");
    let is_antigravity = provider_type.trim().eq_ignore_ascii_case("antigravity");
    if is_antigravity
        && reserve_ratio <= 0.0
        && provider_model_name.is_some_and(|model| model.trim().is_empty())
    {
        return None;
    }
    let requested = (!is_antigravity)
        .then(|| provider_model_name.map(provider_pool_identifier_tokens))
        .flatten();
    if !is_antigravity
        && reserve_ratio <= 0.0
        && requested.as_ref().is_some_and(|tokens| tokens.is_empty())
    {
        return None;
    }

    // Prefer the materialized status snapshot, but also inspect the raw
    // provider metadata.  A quota refresh and a request can race, leaving the
    // latter newer than the snapshot; resolving both avoids falling back to an
    // account-wide signal and incorrectly blocking an unrelated model bucket.
    let sources = [
        provider_pool_member_quota_snapshot(key, provider_type),
        provider_pool_metadata_bucket(key.upstream_metadata.as_ref(), provider_type),
    ];
    let mut resolved = None::<(Option<u64>, bool)>;
    let mut resolved_specific_bucket = false;
    for source in sources.into_iter().flatten() {
        let observed_at = provider_pool_timestamp_unix_secs(source.get("observed_at"))
            .or_else(|| provider_pool_timestamp_unix_secs(source.get("updated_at")));
        let account_signal = (is_codex && reserve_ratio <= 0.0)
            .then(|| provider_pool_codex_account_quota_signal(source, observed_at))
            .flatten();
        let mut windows = provider_pool_collect_quota_windows(source, is_antigravity);
        if is_codex {
            // Raw refresh metadata can be newer than the materialized windows.
            // Include all four legacy slots before selecting the model bucket.
            for prefix in ["primary", "secondary", "spark_primary", "spark_secondary"] {
                let Some(used_percent) =
                    provider_pool_json_f64(source.get(&format!("{prefix}_used_percent")))
                else {
                    continue;
                };
                if provider_pool_json_f64(source.get(&format!("{prefix}_window_minutes")))
                    == Some(0.0)
                {
                    continue;
                }
                let mut window = Map::from_iter([
                    ("code".to_string(), json!(prefix)),
                    ("used_percent".to_string(), json!(used_percent)),
                ]);
                for field in [
                    "reset_at",
                    "next_reset_at",
                    "reset_seconds",
                    "reset_after_seconds",
                ] {
                    if let Some(value) = source.get(&format!("{prefix}_{field}")) {
                        window.insert(field.to_string(), value.clone());
                    }
                }
                windows.push(window);
            }
            windows.retain(provider_pool_quota_window_has_observation);
            if let Some(exhausted) = account_signal {
                if !windows.iter().any(provider_pool_window_is_generic) {
                    // A flags-only quota refresh is still a newer account
                    // observation, but must not override a model-specific bucket.
                    windows.push(Map::from_iter([
                        ("code".to_string(), json!("account")),
                        ("is_exhausted".to_string(), json!(exhausted)),
                    ]));
                }
            }
        }
        if windows.is_empty() {
            continue;
        }
        let model_matches = windows
            .iter()
            .filter(|window| {
                provider_model_name.is_some_and(|model| {
                    provider_pool_window_explicitly_matches_model(window, model, is_antigravity)
                })
            })
            .collect::<Vec<_>>();
        if !model_matches.is_empty()
            && (!is_antigravity
                || model_matches
                    .iter()
                    .any(|window| provider_pool_quota_window_has_observation(window)))
        {
            let exhausted = if reserve_ratio > 0.0 {
                // Reserving quota protects every applicable rate-limit window,
                // including short and weekly limits for an explicit model.
                provider_pool_any_window_exhausted(model_matches, observed_at, reserve_ratio)
            } else {
                provider_pool_explicit_model_windows_exhausted(model_matches, observed_at)
            };
            if resolved.is_none()
                || ((is_codex || is_antigravity) && !resolved_specific_bucket)
                || provider_pool_should_replace_model_quota_resolution(
                    resolved.as_ref().and_then(|(observed_at, _)| *observed_at),
                    observed_at,
                )
            {
                resolved = Some((observed_at, exhausted));
                resolved_specific_bucket = true;
            }
            continue;
        }

        if !is_antigravity {
            // Legacy snapshots may not carry a model field.  Match an opaque
            // family token (the prefix before `_`/`:` in `code`, or an explicit
            // family key) against the model's tokens.  This keeps independent
            // windows isolated without baking in names such as "spark".
            let family_matches = windows
                .iter()
                .filter(|window| {
                    requested.as_ref().is_some_and(|tokens| {
                        provider_pool_window_family_matches_model(window, tokens)
                    })
                })
                .collect::<Vec<_>>();
            if !family_matches.is_empty() {
                let exhausted =
                    provider_pool_any_window_exhausted(family_matches, observed_at, reserve_ratio);
                if resolved.is_none()
                    || (is_codex && !resolved_specific_bucket)
                    || provider_pool_should_replace_model_quota_resolution(
                        resolved.as_ref().and_then(|(observed_at, _)| *observed_at),
                        observed_at,
                    )
                {
                    resolved = Some((observed_at, exhausted));
                    resolved_specific_bucket = true;
                }
                continue;
            }
        }

        // A newer account observation does not update an independent model
        // bucket. Only compare freshness between applicable model sources.
        if (is_codex || is_antigravity) && resolved_specific_bucket {
            continue;
        }

        // Account-scoped windows (for example the ordinary weekly and short
        // windows emitted by Codex) apply to every model that has no more
        // specific family.  Restrict this fallback to well-known structural
        // window names; opaque family names such as `alpha_weekly` must not
        // accidentally make an unrelated model look schedulable.
        let generic_matches = windows
            .iter()
            .filter(|window| {
                provider_pool_window_is_generic(window)
                    && (!is_antigravity
                        || (!provider_pool_window_is_quota_group_scoped(window)
                            && provider_pool_quota_window_has_observation(window)))
            })
            .collect::<Vec<_>>();
        if !generic_matches.is_empty() {
            let exhausted = account_signal.unwrap_or_else(|| {
                provider_pool_any_window_exhausted(generic_matches, observed_at, reserve_ratio)
            });
            if resolved.is_none()
                || provider_pool_should_replace_model_quota_resolution(
                    resolved.as_ref().and_then(|(observed_at, _)| *observed_at),
                    observed_at,
                )
            {
                resolved = Some((observed_at, exhausted));
            }
        }
    }

    resolved.map(|(_, exhausted)| exhausted)
}

fn provider_pool_codex_account_quota_signal(
    source: &Map<String, Value>,
    observed_at: Option<u64>,
) -> Option<bool> {
    let allowed = provider_pool_json_bool(source.get("allowed"));
    let limit_reached = provider_pool_json_bool(source.get("limit_reached"));
    if allowed == Some(false) || limit_reached == Some(true) {
        let reset_elapsed = provider_pool_current_unix_secs().is_some_and(|now| {
            provider_pool_reset_deadline_elapsed(source, observed_at, now)
                || ["primary", "secondary"].into_iter().any(|prefix| {
                    let window = [
                        "reset_at",
                        "next_reset_at",
                        "reset_seconds",
                        "reset_after_seconds",
                    ]
                    .into_iter()
                    .filter_map(|field| {
                        source
                            .get(&format!("{prefix}_{field}"))
                            .map(|value| (field.to_string(), value.clone()))
                    })
                    .collect::<Map<_, _>>();
                    provider_pool_reset_deadline_elapsed(&window, observed_at, now)
                })
        });
        return (!reset_elapsed).then_some(true);
    }
    (allowed == Some(true) || limit_reached == Some(false)).then_some(false)
}

fn provider_pool_should_replace_model_quota_resolution(
    previous_observed_at: Option<u64>,
    next_observed_at: Option<u64>,
) -> bool {
    match (previous_observed_at, next_observed_at) {
        (Some(previous), Some(next)) => next >= previous,
        (None, Some(_)) => true,
        (Some(_), None) => false,
        // Preserve source order when neither side carries freshness metadata;
        // the materialized status snapshot is preferred over raw metadata.
        (None, None) => false,
    }
}

/// Public adapter-independent model quota lookup used by schedulers that need
/// to prefilter candidates before constructing provider-pool signals.
pub fn provider_pool_key_model_quota_exhausted(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: &str,
) -> Option<bool> {
    provider_pool_model_quota_exhausted(key, provider_type, provider_model_name)
}

fn provider_pool_explicit_model_windows_exhausted(
    windows: Vec<&Map<String, Value>>,
    snapshot_observed_at: Option<u64>,
) -> bool {
    let now_unix_secs = provider_pool_current_unix_secs();

    // Explicit model buckets represent independent windows for one model. The
    // model remains usable while at least one of those windows still has
    // capacity, so exhaustion is reported only when all are exhausted.
    windows.iter().all(|window| {
        provider_pool_quota_window_is_exhausted(window)
            && !now_unix_secs.is_some_and(|now| {
                provider_pool_reset_deadline_elapsed(window, snapshot_observed_at, now)
            })
    })
}

fn provider_pool_any_window_exhausted(
    windows: Vec<&Map<String, Value>>,
    snapshot_observed_at: Option<u64>,
    reserve_ratio: f64,
) -> bool {
    let now_unix_secs = provider_pool_current_unix_secs();
    windows.iter().any(|window| {
        provider_pool_quota_window_reaches_reserve(window, reserve_ratio)
            && !now_unix_secs.is_some_and(|now| {
                provider_pool_reset_deadline_elapsed(window, snapshot_observed_at, now)
            })
    })
}

fn provider_pool_quota_window_has_observation(window: &Map<String, Value>) -> bool {
    ["is_exhausted", "exhausted"]
        .into_iter()
        .any(|field| provider_pool_json_bool(window.get(field)).is_some())
        || [
            "used_ratio",
            "usage_ratio",
            "used_percent",
            "remaining_ratio",
            "remaining_fraction",
            "remaining_percent",
        ]
        .into_iter()
        .any(|field| provider_pool_json_f64(window.get(field)).is_some())
        || (provider_pool_json_f64(
            window
                .get("remaining")
                .or_else(|| window.get("remaining_value")),
        )
        .is_some()
            && provider_pool_json_f64(
                window
                    .get("limit")
                    .or_else(|| window.get("limit_value"))
                    .or_else(|| window.get("total")),
            )
            .is_some_and(|limit| limit > 0.0))
}

pub(crate) fn provider_pool_source_account_quota_exhausted(
    source: &Map<String, Value>,
) -> Option<bool> {
    let windows = provider_pool_collect_quota_windows(source, false);
    let account_windows = windows
        .iter()
        .filter(|window| provider_pool_window_is_generic(window))
        .filter(|window| provider_pool_quota_window_has_observation(window))
        .collect::<Vec<_>>();
    if account_windows.is_empty() {
        return None;
    }
    let observed_at = provider_pool_timestamp_unix_secs(source.get("observed_at"))
        .or_else(|| provider_pool_timestamp_unix_secs(source.get("updated_at")));
    Some(provider_pool_any_window_exhausted(
        account_windows,
        observed_at,
        0.0,
    ))
}

/// Whether Codex metadata contains an account quota observation rather than an
/// identity-only update or an independent model quota bucket.
pub fn provider_pool_codex_metadata_has_account_quota(source: &Map<String, Value>) -> bool {
    ["primary_used_percent", "secondary_used_percent"]
        .into_iter()
        .any(|field| provider_pool_json_f64(source.get(field)).is_some())
        || provider_pool_source_account_quota_exhausted(source).is_some()
        || ["allowed", "limit_reached", "has_credits"]
            .into_iter()
            .any(|field| provider_pool_json_bool(source.get(field)).is_some())
        || provider_pool_json_bool(source.get("credits_unlimited")) == Some(true)
}

/// Whether an applicable Codex window has at most 1% remaining. Missing quota
/// data and windows whose reset has elapsed do not trigger this opt-in guard.
pub fn provider_pool_key_minimum_quota_reached(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
    provider_model_name: Option<&str>,
) -> bool {
    if !provider_type.trim().eq_ignore_ascii_case("codex") {
        return false;
    }
    provider_pool_quota_reaches_reserve(key, provider_type, provider_model_name, 0.01)
        .unwrap_or(false)
}

fn provider_pool_quota_window_reaches_reserve(
    window: &Map<String, Value>,
    reserve_ratio: f64,
) -> bool {
    if provider_pool_quota_window_is_exhausted(window) {
        return true;
    }
    if reserve_ratio <= 0.0 {
        return false;
    }
    let used_ratio = provider_pool_json_f64(window.get("used_ratio"))
        .or_else(|| provider_pool_json_f64(window.get("usage_ratio")))
        .or_else(|| provider_pool_json_f64(window.get("used_percent")).map(|value| value / 100.0))
        .or_else(|| {
            provider_pool_json_f64(window.get("remaining_ratio"))
                .or_else(|| provider_pool_json_f64(window.get("remaining_fraction")))
                .map(|value| 1.0 - value)
        })
        .or_else(|| {
            provider_pool_json_f64(window.get("remaining_percent")).map(|value| 1.0 - value / 100.0)
        })
        .or_else(|| {
            let remaining = provider_pool_json_f64(
                window
                    .get("remaining")
                    .or_else(|| window.get("remaining_value")),
            )?;
            let limit = provider_pool_json_f64(
                window
                    .get("limit")
                    .or_else(|| window.get("limit_value"))
                    .or_else(|| window.get("total")),
            )?;
            (limit > 0.0).then_some(1.0 - remaining / limit)
        });
    used_ratio.is_some_and(|used_ratio| used_ratio >= 1.0 - reserve_ratio)
}

fn provider_pool_window_is_generic(window: &Map<String, Value>) -> bool {
    if provider_pool_window_has_explicit_model(window)
        || window
            .get("scope")
            .and_then(Value::as_str)
            .is_some_and(|scope| scope.trim().eq_ignore_ascii_case("model"))
    {
        return false;
    }

    let code = window
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let family = code
        .split_once(['_', ':', '/'])
        .map(|(prefix, _)| prefix)
        .unwrap_or(code)
        .trim()
        .to_ascii_lowercase();
    if family.is_empty() {
        return window
            .get("scope")
            .and_then(Value::as_str)
            .is_some_and(|scope| scope.trim().eq_ignore_ascii_case("account"));
    }
    [
        "weekly",
        "5h",
        "daily",
        "monthly",
        "primary",
        "secondary",
        "account",
        "quota",
        "window",
        "rate",
        "reset",
    ]
    .contains(&family.as_str())
}

fn provider_pool_window_explicitly_matches_model(
    window: &Map<String, Value>,
    requested_model: &str,
    exact_only: bool,
) -> bool {
    if exact_only {
        let requested = requested_model.trim();
        if requested.is_empty() {
            return false;
        }
        return [
            "model",
            "model_name",
            "model_id",
            "quota_model",
            "quota_model_name",
            "target_model",
            "limit_name",
            "models",
            "model_ids",
        ]
        .iter()
        .filter_map(|key| window.get(*key))
        .any(|value| match value {
            Value::String(value) => requested.eq_ignore_ascii_case(value.trim()),
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .any(|value| requested.eq_ignore_ascii_case(value.trim())),
            _ => false,
        });
    }
    let requested = provider_pool_normalize_identifier(requested_model);
    let requested_tokens = provider_pool_identifier_tokens(requested_model);
    if requested.is_empty() {
        return false;
    }
    [
        "model",
        "model_name",
        "model_id",
        "quota_model",
        "quota_model_name",
        "target_model",
        "limit_name",
    ]
    .iter()
    .filter_map(|key| window.get(*key))
    .any(|value| match value {
        Value::String(value) => {
            provider_pool_identifiers_match(&requested, value, &requested_tokens)
        }
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .any(|value| provider_pool_identifiers_match(&requested, value, &requested_tokens)),
        _ => false,
    }) || ["models", "model_ids"]
        .iter()
        .filter_map(|key| window.get(*key).and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .any(|value| provider_pool_identifiers_match(&requested, value, &requested_tokens))
}

fn provider_pool_window_family_matches_model(
    window: &Map<String, Value>,
    requested_tokens: &std::collections::BTreeSet<String>,
) -> bool {
    let explicit_scope = window
        .get("scope")
        .and_then(Value::as_str)
        .map(|scope| scope.trim().to_ascii_lowercase());
    // A model-scoped window without an explicit model must not accidentally
    // match a token from its opaque code.
    if explicit_scope.as_deref() == Some("model") || provider_pool_window_has_explicit_model(window)
    {
        return false;
    }

    let mut families = Vec::new();
    for key in ["quota_group", "quota_family", "family", "bucket"] {
        if let Some(value) = window.get(key).and_then(Value::as_str) {
            families.push(value.to_string());
        }
    }
    if let Some(code) = window.get("code").and_then(Value::as_str) {
        let code = code.trim();
        if let Some((prefix, _)) = code.split_once(['_', ':', '/']) {
            families.push(prefix.to_string());
        }
    }
    families.into_iter().any(|family| {
        let normalized = provider_pool_normalize_identifier(&family);
        if normalized.is_empty()
            || [
                "account",
                "quota",
                "window",
                "primary",
                "secondary",
                "rate",
                "reset",
            ]
            .iter()
            .any(|generic| normalized == *generic)
        {
            return false;
        }
        requested_tokens.iter().any(|token| {
            token.len() >= 3
                && (token == &normalized
                    || token.contains(&normalized)
                    || normalized.contains(token))
        })
    })
}

fn provider_pool_identifiers_match(
    requested: &str,
    candidate: &str,
    requested_tokens: &std::collections::BTreeSet<String>,
) -> bool {
    let candidate_tokens = provider_pool_identifier_tokens(candidate);
    let candidate = provider_pool_normalize_identifier(candidate);
    if candidate.is_empty() {
        return false;
    }
    requested == candidate
        || candidate_tokens
            .iter()
            .any(|token| {
                requested_tokens.contains(token) && provider_pool_is_specific_model_token(token)
            })
        // Handle compact upstream identifiers such as `spark` embedded in a
        // provider model name (`vendor-codex-spark`) while avoiding accidental
        // one/two-character matches.
        || (candidate.len() >= 4
            && requested.len() >= 6
            && (requested.contains(&candidate) || candidate.contains(requested)))
}

fn provider_pool_is_specific_model_token(token: &str) -> bool {
    token.len() >= 4
        && !token.chars().all(|character| character.is_ascii_digit())
        && ![
            "auto",
            "base",
            "claude",
            "codex",
            "default",
            "fast",
            "flash",
            "free",
            "gemini",
            "gpt",
            "latest",
            "mini",
            "model",
            "plus",
            "pro",
            "reasoning",
            "team",
            "think",
            "thinking",
            "tiered",
            "vendor",
        ]
        .contains(&token)
}

fn provider_pool_window_has_explicit_model(window: &Map<String, Value>) -> bool {
    [
        "model",
        "model_name",
        "model_id",
        "quota_model",
        "quota_model_name",
        "target_model",
        "models",
        "model_ids",
    ]
    .iter()
    .any(|key| match window.get(*key) {
        Some(Value::String(value)) => !value.trim().is_empty(),
        Some(Value::Array(values)) => values
            .iter()
            .any(|value| value.as_str().is_some_and(|value| !value.trim().is_empty())),
        _ => false,
    })
}

fn provider_pool_normalize_identifier(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn provider_pool_identifier_tokens(value: &str) -> std::collections::BTreeSet<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| token.len() >= 3)
        .collect()
}

/// 依次借用 `quota_by_model`、`models`、`model_quotas` 中对象形式的模型映射；
/// `first_model_map_only` 只取首个对象（空对象也算），用于权威映射选择。
fn provider_pool_authoritative_model_maps<'a>(
    source: &'a Map<String, Value>,
    first_model_map_only: bool,
) -> impl Iterator<Item = &'a Map<String, Value>> + 'a {
    ["quota_by_model", "models", "model_quotas"]
        .into_iter()
        .filter_map(move |key| source.get(key).and_then(Value::as_object))
        .take(if first_model_map_only { 1 } else { usize::MAX })
}

/// Materialize quota windows from the small set of shapes emitted by current
/// and legacy adapters.  Model maps (`quota_by_model`/`models`) are converted
/// to the same window representation used by status snapshots, with the map
/// key retained as the model identity.  Keeping this normalization here means
/// provider adapters do not need to grow model-specific quota code whenever an
/// upstream introduces another independent bucket.
fn provider_pool_collect_quota_windows(
    source: &Map<String, Value>,
    first_model_map_only: bool,
) -> Vec<Map<String, Value>> {
    let mut windows = Vec::new();
    for key in ["windows", "additional_quota_windows"] {
        if let Some(values) = source.get(key).and_then(Value::as_array) {
            windows.extend(values.iter().filter_map(Value::as_object).cloned());
        }
    }
    for models in provider_pool_authoritative_model_maps(source, first_model_map_only) {
        for (model_name, item) in models {
            let Some(item) = item.as_object() else {
                continue;
            };
            let mut window = item.clone();
            window
                .entry("model".to_string())
                .or_insert_with(|| json!(model_name));
            window
                .entry("scope".to_string())
                .or_insert_with(|| json!("model"));
            window
                .entry("code".to_string())
                .or_insert_with(|| json!(format!("model:{model_name}")));
            windows.push(window);
        }
    }
    windows
}

pub fn provider_pool_quota_snapshot_updated_at(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> Option<u64> {
    let quota_snapshot = provider_pool_member_quota_snapshot(key, provider_type)?;
    provider_pool_timestamp_unix_secs(quota_snapshot.get("updated_at"))
}

pub fn provider_pool_quota_metadata_updated_at(
    upstream_metadata: Option<&Value>,
    provider_type: &str,
) -> Option<u64> {
    let bucket = provider_pool_metadata_bucket(upstream_metadata, provider_type)?;
    provider_pool_timestamp_unix_secs(bucket.get("updated_at"))
}

pub fn provider_pool_quota_metadata_provider_type(metadata_update: &Value) -> Option<String> {
    let object = metadata_update.as_object()?;
    let service = ProviderPoolService::with_builtin_adapters();
    let known_provider_type = service
        .provider_types()
        .find(|provider_type| object.contains_key(*provider_type))
        .map(ToOwned::to_owned);
    known_provider_type.or_else(|| {
        object
            .iter()
            .find(|(_, value)| value.is_object())
            .map(|(provider_type, _)| provider_type.clone())
    })
}

pub fn provider_pool_key_scheduling_label(
    is_active: bool,
    cooldown_reason: Option<&str>,
    cooldown_ttl_seconds: Option<u64>,
) -> (String, String, String, Vec<Value>) {
    if !is_active {
        return (
            "blocked".to_string(),
            "inactive".to_string(),
            "已禁用".to_string(),
            vec![json!({
                "code": "inactive",
                "label": "已禁用",
                "blocking": true,
                "source": "manual",
                "ttl_seconds": Value::Null,
                "detail": Value::Null,
            })],
        );
    }
    if let Some(reason) = cooldown_reason {
        return (
            "degraded".to_string(),
            "cooldown".to_string(),
            "冷却中".to_string(),
            vec![json!({
                "code": "cooldown",
                "label": "冷却中",
                "blocking": true,
                "source": "pool",
                "ttl_seconds": cooldown_ttl_seconds,
                "detail": reason,
            })],
        );
    }
    (
        "available".to_string(),
        "available".to_string(),
        "可用".to_string(),
        Vec::new(),
    )
}

pub(crate) fn provider_pool_metadata_bucket<'a>(
    upstream_metadata: Option<&'a Value>,
    provider_type: &str,
) -> Option<&'a Map<String, Value>> {
    upstream_metadata
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(&provider_type.trim().to_ascii_lowercase()))
        .and_then(Value::as_object)
}

pub(crate) fn provider_pool_json_bool(value: Option<&Value>) -> Option<bool> {
    match value {
        Some(Value::Bool(value)) => Some(*value),
        Some(Value::String(value)) => {
            let value = value.trim();
            if value.eq_ignore_ascii_case("true") || value == "1" {
                Some(true)
            } else if value.eq_ignore_ascii_case("false") || value == "0" {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn provider_pool_json_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(value)) => value.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite())
}

pub(crate) fn provider_pool_timestamp_unix_secs(value: Option<&Value>) -> Option<u64> {
    let mut timestamp = match provider_pool_json_f64(value) {
        Some(timestamp) => timestamp,
        None => {
            return chrono::DateTime::parse_from_rfc3339(value?.as_str()?.trim())
                .ok()?
                .timestamp()
                .try_into()
                .ok();
        }
    };
    if timestamp <= 0.0 {
        return None;
    }
    if timestamp > 1_000_000_000_000.0 {
        timestamp /= 1000.0;
    }
    Some(timestamp as u64)
}

pub(crate) fn provider_pool_current_unix_secs() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn provider_pool_reset_deadline_unix_secs(
    item: &Map<String, Value>,
    fallback_observed_at: Option<u64>,
) -> Option<u64> {
    provider_pool_timestamp_unix_secs(item.get("reset_at"))
        .or_else(|| provider_pool_timestamp_unix_secs(item.get("next_reset_at")))
        .or_else(|| provider_pool_timestamp_unix_secs(item.get("reset_time")))
        .or_else(|| provider_pool_timestamp_unix_secs(item.get("next_reset_time")))
        .or_else(|| {
            let reset_seconds = provider_pool_json_f64(item.get("reset_seconds"))
                .filter(|value| *value >= 0.0)
                .or_else(|| {
                    provider_pool_json_f64(item.get("reset_after_seconds"))
                        .filter(|value| *value >= 0.0)
                })?;
            let base = provider_pool_timestamp_unix_secs(item.get("observed_at"))
                .or_else(|| provider_pool_timestamp_unix_secs(item.get("updated_at")))
                .or(fallback_observed_at)?;
            Some(base.saturating_add(reset_seconds.ceil() as u64))
        })
}

/// Whether the quota window's reset deadline has already passed.
///
/// The deadline is taken from `reset_at`/`next_reset_at`/`reset_time`/`next_reset_time`
/// when present, otherwise derived from `reset_seconds`/`reset_after_seconds` anchored at
/// the window (or fallback) observation time. Scheduling ignores exhausted windows once
/// this is true; read paths can reuse the same predicate so the displayed quota matches
/// the scheduling decision after a reset.
pub fn provider_pool_reset_deadline_elapsed(
    item: &Map<String, Value>,
    fallback_observed_at: Option<u64>,
    now_unix_secs: u64,
) -> bool {
    provider_pool_reset_deadline_unix_secs(item, fallback_observed_at)
        .is_some_and(|reset_at| reset_at <= now_unix_secs)
}

fn provider_pool_quota_window_is_exhausted(window: &Map<String, Value>) -> bool {
    provider_pool_json_bool(window.get("is_exhausted"))
        .or_else(|| provider_pool_json_bool(window.get("exhausted")))
        .or_else(|| {
            provider_pool_json_f64(
                window
                    .get("used_ratio")
                    .or_else(|| window.get("usage_ratio")),
            )
            .map(|value| value >= 1.0 - 1e-6)
        })
        .or_else(|| {
            provider_pool_json_f64(window.get("used_percent")).map(|value| value >= 100.0 - 1e-6)
        })
        .or_else(|| {
            provider_pool_json_f64(
                window
                    .get("remaining_ratio")
                    .or_else(|| window.get("remaining_fraction")),
            )
            .map(|value| value <= 1e-6)
        })
        .or_else(|| {
            provider_pool_json_f64(window.get("remaining_percent")).map(|value| value <= 1e-6)
        })
        .or_else(|| {
            let remaining = provider_pool_json_f64(
                window
                    .get("remaining")
                    .or_else(|| window.get("remaining_value")),
            )?;
            let limit = provider_pool_json_f64(
                window
                    .get("limit")
                    .or_else(|| window.get("limit_value"))
                    .or_else(|| window.get("total")),
            )?;
            (limit > 0.0).then_some(remaining <= 0.0)
        })
        .unwrap_or(false)
}

fn provider_pool_window_is_model_scoped(window: &Map<String, Value>) -> bool {
    window
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| scope.trim().eq_ignore_ascii_case("model"))
        || provider_pool_window_has_explicit_model(window)
}

fn provider_pool_window_is_quota_group_scoped(window: &Map<String, Value>) -> bool {
    window
        .get("scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| scope.trim().eq_ignore_ascii_case("quota_group"))
        || window
            .get("quota_group")
            .and_then(Value::as_str)
            .is_some_and(|group| !group.trim().is_empty())
}

fn provider_pool_quota_snapshot_matches_provider(
    quota_snapshot: &Map<String, Value>,
    provider_type: &str,
) -> bool {
    let normalized_provider_type = provider_type.trim().to_ascii_lowercase();
    match quota_snapshot
        .get("provider_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(provider_type) => provider_type.eq_ignore_ascii_case(&normalized_provider_type),
        None => {
            provider_pool_json_bool(quota_snapshot.get("exhausted")) == Some(true)
                || quota_snapshot
                    .get("code")
                    .and_then(Value::as_str)
                    .is_some_and(|code| !code.trim().eq_ignore_ascii_case("unknown"))
                || quota_snapshot
                    .get("updated_at")
                    .is_some_and(|value| !value.is_null())
                || quota_snapshot
                    .get("observed_at")
                    .is_some_and(|value| !value.is_null())
                || quota_snapshot
                    .get("usage_ratio")
                    .is_some_and(|value| !value.is_null())
                || quota_snapshot
                    .get("reset_seconds")
                    .is_some_and(|value| !value.is_null())
                || quota_snapshot
                    .get("windows")
                    .and_then(Value::as_array)
                    .is_some_and(|windows| !windows.is_empty())
                || quota_snapshot
                    .get("additional_quota_windows")
                    .and_then(Value::as_array)
                    .is_some_and(|windows| !windows.is_empty())
                || ["quota_by_model", "models", "model_quotas"]
                    .iter()
                    .any(|key| {
                        quota_snapshot
                            .get(*key)
                            .and_then(Value::as_object)
                            .is_some_and(|models| !models.is_empty())
                    })
                || quota_snapshot
                    .get("credits")
                    .and_then(Value::as_object)
                    .is_some_and(|credits| !credits.is_empty())
        }
    }
}

pub(crate) fn provider_pool_quota_snapshot_exhausted_decision(
    key: &StoredProviderCatalogKey,
    provider_type: &str,
) -> Option<bool> {
    let quota_snapshot = key
        .status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("quota"))
        .and_then(Value::as_object)?;
    if !provider_pool_quota_snapshot_matches_provider(quota_snapshot, provider_type) {
        return None;
    }
    if let Some(exhausted) = source_account_exhausted(
        quota_snapshot,
        provider_pool_current_unix_secs().unwrap_or_default(),
    ) {
        return Some(exhausted);
    }
    // 旧官方快照无法证明单位及套餐归属；等待正常刷新，不沿用旧查询错误形成的阻断。
    if matches!(
        provider_type.trim().to_ascii_lowercase().as_str(),
        "zhipu" | "zai" | "kimi_coding" | "minimax"
    ) {
        return Some(false);
    }
    if quota_snapshot
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("balance"))
    {
        return Some(false);
    }
    let is_antigravity = provider_type.trim().eq_ignore_ascii_case("antigravity");
    let exhausted = provider_pool_json_bool(quota_snapshot.get("exhausted"))?;
    if exhausted {
        let now_unix_secs = provider_pool_current_unix_secs();
        let snapshot_observed_at =
            provider_pool_timestamp_unix_secs(quota_snapshot.get("observed_at"))
                .or_else(|| provider_pool_timestamp_unix_secs(quota_snapshot.get("updated_at")));

        let materialized_windows =
            provider_pool_collect_quota_windows(quota_snapshot, is_antigravity);
        if !materialized_windows.is_empty() {
            // Model-scoped windows are evaluated only when the request model
            // is known.  They must not turn the account-level fallback into
            // an exhausted state for unrelated models.
            let account_scoped_windows = materialized_windows
                .iter()
                .filter(|window| {
                    !(provider_pool_window_is_model_scoped(window)
                        || is_antigravity && provider_pool_window_is_quota_group_scoped(window))
                })
                .collect::<Vec<_>>();
            // A snapshot containing only model-scoped buckets has no
            // account-wide signal to apply when the caller did not provide a
            // model name (for example, an admin status listing). Do not let a
            // single exhausted model poison every sibling bucket.
            if account_scoped_windows.is_empty() {
                return Some(false);
            }
            let windows = account_scoped_windows;
            let mut saw_exhausted_window = false;
            let mut saw_active_exhausted_window = false;
            let mut windows_max_ratio = None::<f64>;

            for window in windows.iter() {
                if let Some(ratio) = provider_pool_json_f64(window.get("used_ratio")) {
                    windows_max_ratio =
                        Some(windows_max_ratio.map_or(ratio, |current| current.max(ratio)));
                }
                if provider_pool_quota_window_is_exhausted(window) {
                    saw_exhausted_window = true;
                    let reset_elapsed = now_unix_secs.is_some_and(|now| {
                        provider_pool_reset_deadline_elapsed(window, snapshot_observed_at, now)
                    });
                    if !reset_elapsed {
                        saw_active_exhausted_window = true;
                    }
                }
            }

            if saw_exhausted_window {
                return Some(saw_active_exhausted_window);
            }
            if windows_max_ratio.is_some_and(|ratio| ratio < 1.0 - 1e-6) {
                return Some(false);
            }
        } else if now_unix_secs.is_some_and(|now| {
            provider_pool_reset_deadline_elapsed(quota_snapshot, snapshot_observed_at, now)
        }) {
            return Some(false);
        }
    }
    Some(exhausted)
}

pub(crate) fn provider_pool_quota_usage_ratio(key: &StoredProviderCatalogKey) -> Option<f64> {
    key.status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("quota"))
        .and_then(Value::as_object)
        .and_then(|quota| provider_pool_json_f64(quota.get("usage_ratio")))
}

pub(crate) fn provider_pool_quota_reset_seconds(key: &StoredProviderCatalogKey) -> Option<f64> {
    key.status_snapshot
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|snapshot| snapshot.get("quota"))
        .and_then(Value::as_object)
        .and_then(|quota| provider_pool_json_f64(quota.get("reset_seconds")))
}

pub(crate) fn provider_pool_account_blocked(key: &StoredProviderCatalogKey) -> bool {
    key.oauth_invalid_reason.as_deref().is_some_and(|reason| {
        let normalized = reason.trim().to_ascii_lowercase();
        !normalized.is_empty()
            && [
                "banned",
                "forbidden",
                "blocked",
                "suspend",
                "deactivated",
                "disabled",
                "verification",
                "workspace",
                "受限",
                "封",
                "禁",
            ]
            .iter()
            .any(|hint| normalized.contains(hint))
    })
}
