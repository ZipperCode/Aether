use std::time::Duration;

use aether_cache::ExpiringMap;

const ENDPOINT_CAPABILITY_QUARANTINE_TTL_ENV: &str =
    "AETHER_GATEWAY_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS";
const DEFAULT_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS: u64 = 10 * 60;
const MIN_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS: u64 = 30;
const MAX_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS: u64 = 60 * 60;
const ENDPOINT_CAPABILITY_QUARANTINE_MAX_ENTRIES: usize = 50_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct EndpointCapabilityQuarantineKey {
    model_id: String,
    endpoint_id: String,
    key_id: String,
    client_api_format: String,
    request_mode: &'static str,
    request_operation: Option<String>,
}

impl EndpointCapabilityQuarantineKey {
    pub(crate) fn new(
        model_id: &str,
        endpoint_id: &str,
        key_id: &str,
        client_api_format: &str,
        stream: bool,
        request_operation: Option<&str>,
    ) -> Option<Self> {
        let model_id = model_id.trim();
        let endpoint_id = endpoint_id.trim();
        let key_id = key_id.trim();
        let client_api_format = client_api_format.trim().to_ascii_lowercase();
        if model_id.is_empty()
            || endpoint_id.is_empty()
            || key_id.is_empty()
            || client_api_format.is_empty()
        {
            return None;
        }
        Some(Self {
            model_id: model_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            key_id: key_id.to_string(),
            client_api_format,
            request_mode: if stream { "stream" } else { "sync" },
            request_operation: request_operation
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase()),
        })
    }

    pub(crate) fn model_id(&self) -> &str {
        &self.model_id
    }

    pub(crate) fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }

    pub(crate) fn key_id(&self) -> &str {
        &self.key_id
    }

    pub(crate) fn client_api_format(&self) -> &str {
        &self.client_api_format
    }

    pub(crate) fn request_mode(&self) -> &str {
        self.request_mode
    }

    pub(crate) fn request_operation(&self) -> Option<&str> {
        self.request_operation.as_deref()
    }
}

#[derive(Debug, Default)]
pub(crate) struct EndpointCapabilityQuarantineCache {
    entries: ExpiringMap<EndpointCapabilityQuarantineKey, ()>,
}

impl EndpointCapabilityQuarantineCache {
    pub(crate) fn contains(&self, key: &EndpointCapabilityQuarantineKey) -> bool {
        self.entries.contains_fresh(key, quarantine_ttl())
    }

    pub(crate) fn mark(&self, key: EndpointCapabilityQuarantineKey) {
        self.entries.insert(
            key,
            (),
            quarantine_ttl(),
            ENDPOINT_CAPABILITY_QUARANTINE_MAX_ENTRIES,
        );
    }

    pub(crate) fn clear_for_success(
        &self,
        model_id: &str,
        endpoint_id: &str,
        key_id: &str,
        client_api_format: &str,
        stream: bool,
        request_operation: Option<&str>,
    ) {
        let Some(key) = EndpointCapabilityQuarantineKey::new(
            model_id,
            endpoint_id,
            key_id,
            client_api_format,
            stream,
            request_operation,
        ) else {
            return;
        };
        self.entries.remove(&key);
    }

    pub(crate) fn clear(&self) {
        self.entries.clear();
    }

    pub(crate) fn snapshot(&self) -> Vec<EndpointCapabilityQuarantineKey> {
        self.entries
            .snapshot_fresh(quarantine_ttl())
            .into_iter()
            .map(|entry| entry.key)
            .collect()
    }
}

fn quarantine_ttl() -> Duration {
    let seconds = std::env::var(ENDPOINT_CAPABILITY_QUARANTINE_TTL_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS)
        .clamp(
            MIN_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS,
            MAX_ENDPOINT_CAPABILITY_QUARANTINE_TTL_SECS,
        );
    Duration::from_secs(seconds)
}

#[cfg(test)]
mod tests {
    use super::{EndpointCapabilityQuarantineCache, EndpointCapabilityQuarantineKey};

    #[test]
    fn quarantine_is_scoped_by_model_endpoint_key_format_mode_and_operation() {
        let cache = EndpointCapabilityQuarantineCache::default();
        let stream = EndpointCapabilityQuarantineKey::new(
            "model-1",
            "endpoint-1",
            "key-1",
            "openai:responses",
            true,
            Some("compact"),
        )
        .expect("valid quarantine key");
        cache.mark(stream.clone());

        assert!(cache.contains(&stream));
        assert!(!cache.contains(
            &EndpointCapabilityQuarantineKey::new(
                "model-1",
                "endpoint-1",
                "key-1",
                "openai:responses",
                false,
                Some("compact"),
            )
            .expect("valid sync quarantine key")
        ));
        assert!(!cache.contains(
            &EndpointCapabilityQuarantineKey::new(
                "model-1",
                "endpoint-1",
                "key-1",
                "claude:messages",
                true,
                Some("compact"),
            )
            .expect("valid format quarantine key")
        ));
        assert!(!cache.contains(
            &EndpointCapabilityQuarantineKey::new(
                "model-1",
                "endpoint-1",
                "key-2",
                "openai:responses",
                true,
                Some("compact"),
            )
            .expect("valid credential quarantine key")
        ));

        assert!(!cache.contains(
            &EndpointCapabilityQuarantineKey::new(
                "model-1",
                "endpoint-1",
                "key-1",
                "openai:responses",
                true,
                None,
            )
            .expect("valid operation quarantine key")
        ));

        cache.clear_for_success(
            "model-1",
            "endpoint-1",
            "key-1",
            "openai:responses",
            true,
            Some("compact"),
        );
        assert!(!cache.contains(&stream));
    }
}
