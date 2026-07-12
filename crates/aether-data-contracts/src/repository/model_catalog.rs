use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredModelCatalogEntry {
    pub global_model_id: String,
    pub global_model_name: String,
    pub global_model_config: Option<serde_json::Value>,
    pub global_model_supported_capabilities: Option<serde_json::Value>,
    pub global_model_is_active: bool,
    pub provider_model_id: String,
    pub provider_model_name: String,
    pub provider_model_mappings: Option<serde_json::Value>,
    pub provider_model_config: Option<serde_json::Value>,
    pub provider_model_is_active: bool,
    pub provider_model_is_available: bool,
    pub provider_id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub provider_is_active: bool,
}

#[async_trait]
pub trait ModelCatalogReadRepository: Send + Sync {
    /// Lists static model catalog entries. Implementations must not derive
    /// catalog visibility from provider endpoints, credentials, or runtime health.
    async fn list_model_catalog(
        &self,
    ) -> Result<Vec<StoredModelCatalogEntry>, crate::DataLayerError>;

    /// Reads the bounded catalog rows for one public model name.
    async fn read_model_catalog_detail(
        &self,
        global_model_name: &str,
    ) -> Result<Vec<StoredModelCatalogEntry>, crate::DataLayerError>;
}
