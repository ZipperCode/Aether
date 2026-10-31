// Quota-family selection adapted from OmniRoute PR #18026, revision
// 848f2573cf3d32a91f93bacf5b4ab3148dffa1c6:
// open-sse/services/antigravityQuotaFamily.ts and usage/antigravityWeeklyQuota.ts.
// MIT License
// Copyright (c) 2026 diegosouzapw
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use serde_json::{Map, Value};

use super::{
    provider_pool_current_unix_secs, provider_pool_json_bool,
    provider_pool_quota_window_has_observation, provider_pool_quota_window_is_exhausted,
    provider_pool_reset_deadline_elapsed, provider_pool_timestamp_unix_secs,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuotaFamily {
    Argon,
    Gemini,
    ThirdParty,
}

fn contains_ascii(value: &str, needle: &str) -> bool {
    value
        .as_bytes()
        .windows(needle.len())
        .any(|part| part.eq_ignore_ascii_case(needle.as_bytes()))
}

fn quota_family(value: &str) -> Option<QuotaFamily> {
    // Argon 必须先于 Gemini 识别；仅解释额度归属，不生成模型或改写请求模型。
    if contains_ascii(value, "argon")
        || ["gemini-4", "gemini_4", "gemini 4"]
            .iter()
            .any(|name| contains_ascii(value, name))
    {
        Some(QuotaFamily::Argon)
    } else if contains_ascii(value, "gemini") {
        Some(QuotaFamily::Gemini)
    } else if ["claude", "cloud-", "anthropic", "gpt", "3p"]
        .iter()
        .any(|name| contains_ascii(value, name))
    {
        Some(QuotaFamily::ThirdParty)
    } else {
        None
    }
}

fn group_family(group: &Map<String, Value>) -> Option<QuotaFamily> {
    let labels = ["display_name", "group_id"]
        .into_iter()
        .filter_map(|field| group.get(field).and_then(Value::as_str));
    let bucket_labels = group
        .get("buckets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|bucket| {
            ["bucket_id", "display_name"]
                .into_iter()
                .filter_map(move |field| bucket.get(field).and_then(Value::as_str))
        });
    let mut family = None;
    for label in labels.chain(bucket_labels) {
        match quota_family(label) {
            Some(QuotaFamily::Argon) => return Some(QuotaFamily::Argon),
            Some(candidate) if family.is_none() => family = Some(candidate),
            _ => {}
        }
    }
    family
}

pub(super) fn model_quota_exhausted(key: &StoredProviderCatalogKey, model: &str) -> bool {
    let Some(mut family) = quota_family(model.trim().rsplit('/').next().unwrap_or(model)) else {
        return false;
    };
    let Some(bucket) = key
        .upstream_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("antigravity"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    let Some(groups) = bucket.get("quota_groups").and_then(Value::as_array) else {
        return false;
    };
    // 复刻参考实现的缺组回退：独立 Argon 组存在时永远选它，耗尽不能借 Gemini 额度。
    if family == QuotaFamily::Argon
        && !groups
            .iter()
            .filter_map(Value::as_object)
            .any(|group| group_family(group) == Some(QuotaFamily::Argon))
    {
        family = QuotaFamily::Gemini;
    }
    let observed_at = provider_pool_timestamp_unix_secs(bucket.get("quota_groups_updated_at"))
        .or_else(|| provider_pool_timestamp_unix_secs(bucket.get("updated_at")));
    let now = provider_pool_current_unix_secs();
    groups
        .iter()
        .filter_map(Value::as_object)
        .filter(|group| group_family(group) == Some(family))
        .filter_map(|group| group.get("buckets").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_object)
        .any(|window| {
            // 双窗口保留独立重置期限，不能用五小时恢复覆盖仍耗尽的周限制。
            provider_pool_json_bool(window.get("disabled")) != Some(true)
                && provider_pool_json_bool(window.get("is_included")) != Some(false)
                && provider_pool_quota_window_has_observation(window)
                && provider_pool_quota_window_is_exhausted(window)
                && !now.is_some_and(|now| {
                    provider_pool_reset_deadline_elapsed(window, observed_at, now)
                })
        })
}
