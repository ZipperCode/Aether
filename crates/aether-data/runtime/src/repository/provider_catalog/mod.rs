mod memory;

#[allow(unused_imports)]
pub(crate) use aether_data_contracts::repository::provider_catalog::{
    ProviderCatalogKeyAdaptiveState, ProviderCatalogKeyAdaptiveStateUpdate,
    ProviderCatalogKeyAdminCasUpdate, ProviderCatalogKeyCredentialsCasUpdate,
    ProviderCatalogKeyHealthStateUpdate, ProviderCatalogKeyListOrder, ProviderCatalogKeyListQuery,
    ProviderCatalogKeyOAuthCredentialCasDelete, ProviderCatalogKeyOAuthCredentialFence,
    ProviderCatalogKeyOAuthRuntimeStateCasUpdate, ProviderCatalogKeyRuntimeMetadataUpdate,
    ProviderCatalogKeySchedulingStateCasUpdate, ProviderCatalogKeyStatusSnapshotUpdate,
    ProviderCatalogProviderConfigCasUpdate, ProviderCatalogProxyCasUpdate,
    ProviderCatalogReadRepository, ProviderCatalogSnapshot,
    ProviderCatalogUpstreamMetadataNamespaceExpectation,
    ProviderCatalogUpstreamMetadataNamespaceUpdate, ProviderCatalogWriteRepository,
    StoredProviderCatalogAuthMaintenanceCandidate, StoredProviderCatalogEndpoint,
    StoredProviderCatalogKey, StoredProviderCatalogKeyMaintenanceSummary,
    StoredProviderCatalogKeyPage, StoredProviderCatalogKeyStats,
    StoredProviderCatalogModelFetchCandidate, StoredProviderCatalogProvider,
};
#[cfg(feature = "postgres")]
pub use aether_data_postgres::SqlxProviderCatalogReadRepository;
pub use memory::InMemoryProviderCatalogReadRepository;
