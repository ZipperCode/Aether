use super::domain::{
    AttemptResult, ItemStatus, OfficialQuotaItem, PersistedSnapshot, StableErrorClass,
};
use super::persistence::{failure_refresh_state, latest_refresh_state, latest_snapshot};
use aether_data_contracts::repository::provider_catalog::StoredProviderCatalogKey;
use serde_json::{json, Map, Value};

pub(super) fn management_response(items: Vec<OfficialQuotaItem>) -> Value {
    let mut success = 0u64;
    let mut failed = 0u64;
    let mut skipped = 0u64;
    for item in &items {
        match item.status {
            ItemStatus::Success => success += 1,
            ItemStatus::Error => failed += 1,
            ItemStatus::Backoff => skipped += 1,
        }
    }
    let total = success + failed + skipped;
    let results = items.into_iter().map(item_payload).collect::<Vec<_>>();
    json!({
        "total": total,
        "success": success,
        "failed": failed,
        "skipped": skipped,
        "results": results,
    })
}

pub(super) fn persisted_item(
    key: &StoredProviderCatalogKey,
    attempt: &AttemptResult,
    persisted: PersistedSnapshot,
) -> OfficialQuotaItem {
    let (status, error_class, message) = match attempt.failure_class() {
        Some(class) => (
            ItemStatus::Error,
            Some(class),
            attempt.failure_message().map(str::to_owned),
        ),
        None => (ItemStatus::Success, None, None),
    };
    OfficialQuotaItem {
        key_id: key.id.clone(),
        key_name: key.name.clone(),
        status,
        status_code: attempt.status_code(),
        error_class,
        message,
        quota_snapshot: Some(persisted.snapshot),
        refresh_state: persisted.refresh_state,
    }
}

pub(super) fn backoff_item(key: &StoredProviderCatalogKey) -> OfficialQuotaItem {
    OfficialQuotaItem {
        key_id: key.id.clone(),
        key_name: key.name.clone(),
        status: ItemStatus::Backoff,
        status_code: None,
        error_class: None,
        message: None,
        quota_snapshot: latest_snapshot(key),
        refresh_state: latest_refresh_state(key),
    }
}

pub(super) fn rejected_item(
    key: &StoredProviderCatalogKey,
    class: StableErrorClass,
    now_unix_secs: u64,
) -> OfficialQuotaItem {
    OfficialQuotaItem {
        key_id: key.id.clone(),
        key_name: key.name.clone(),
        status: ItemStatus::Error,
        status_code: None,
        error_class: Some(class),
        message: Some(class.message().to_owned()),
        quota_snapshot: latest_snapshot(key),
        refresh_state: failure_refresh_state(key, class, now_unix_secs),
    }
}

fn item_payload(item: OfficialQuotaItem) -> Value {
    let mut payload = Map::from_iter([
        ("key_id".into(), Value::String(item.key_id)),
        ("key_name".into(), Value::String(item.key_name)),
        ("status".into(), Value::String(item.status.as_str().into())),
        (
            "quota_snapshot".into(),
            item.quota_snapshot.unwrap_or(Value::Null),
        ),
        (
            "refresh_state".into(),
            serde_json::to_value(item.refresh_state).unwrap_or(Value::Null),
        ),
    ]);
    if let Some(status_code) = item.status_code {
        payload.insert("status_code".into(), json!(status_code));
    }
    if let Some(error_class) = item.error_class {
        payload.insert(
            "error_class".into(),
            Value::String(error_class.code().into()),
        );
    }
    if let Some(message) = item.message {
        payload.insert("message".into(), Value::String(message));
    }
    Value::Object(payload)
}
