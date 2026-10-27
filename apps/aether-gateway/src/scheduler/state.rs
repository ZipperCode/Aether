use std::time::Duration;

use aether_data_contracts::repository::candidates::StoredRequestCandidate;
use aether_data_contracts::repository::provider_catalog::{
    StoredProviderCatalogKey, StoredProviderCatalogProvider,
};
use aether_data_contracts::repository::quota::StoredProviderQuotaSnapshot;
use aether_scheduler_core::SchedulerAffinityTarget;
use async_trait::async_trait;

use crate::orchestration::AntigravityModelQuotaCandidate;
use crate::GatewayError;

/// Domain outcome for one candidate's scoped runtime model quota state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchedulerModelQuotaState {
    /// No active scoped runtime model quota fact.
    Available,
    /// Active exact-model runtime quota fact for this candidate credential.
    Blocked,
    /// This candidate's credential/config could not be read; its state is
    /// unknown and must not be reported as exhausted.
    StateUnavailable,
}

#[async_trait]
pub(crate) trait SchedulerRuntimeState {
    async fn read_provider_quota_snapshot(
        &self,
        provider_id: &str,
    ) -> Result<Option<StoredProviderQuotaSnapshot>, GatewayError>;

    async fn read_provider_catalog_providers_by_ids(
        &self,
        provider_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogProvider>, GatewayError>;

    async fn read_provider_catalog_keys_by_ids(
        &self,
        key_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, GatewayError>;

    /// Reads credential-bearing Key records without the local catalog cache.
    /// Scheduling must observe a quota block written by another gateway
    /// instance before it admits a new candidate.
    async fn read_provider_catalog_keys_by_ids_strong(
        &self,
        key_ids: &[String],
    ) -> Result<Vec<StoredProviderCatalogKey>, GatewayError>;

    /// Reads the shared, key/model-scoped Antigravity runtime quota facts in one
    /// batched backend round trip. Results keep the descriptor order. A single
    /// candidate whose credential/config cannot be read reports
    /// `StateUnavailable` without erasing valid siblings. A returned error means
    /// the shared backend failed for the whole batch and must never be reported
    /// as an exhausted model quota.
    async fn read_antigravity_model_quota_blocked(
        &self,
        candidates: &[AntigravityModelQuotaCandidate<'_>],
        now_unix_secs: u64,
    ) -> Result<Vec<SchedulerModelQuotaState>, GatewayError>;

    async fn read_recent_request_candidates(
        &self,
        limit: usize,
    ) -> Result<Vec<StoredRequestCandidate>, GatewayError>;

    fn provider_key_rpm_reset_at(&self, key_id: &str, now_unix_secs: u64) -> Option<u64>;

    fn read_cached_scheduler_affinity_target(
        &self,
        cache_key: &str,
        ttl: Duration,
    ) -> Option<SchedulerAffinityTarget>;

    fn scheduler_affinity_epoch(&self) -> u64;

    fn remember_scheduler_affinity_target(
        &self,
        cache_key: &str,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
    );

    fn remember_scheduler_affinity_target_for_epoch(
        &self,
        cache_key: &str,
        target: SchedulerAffinityTarget,
        ttl: Duration,
        max_entries: usize,
        expected_epoch: Option<u64>,
    ) -> bool;
}
