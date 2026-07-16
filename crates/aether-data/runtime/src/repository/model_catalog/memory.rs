use async_trait::async_trait;

use super::{ModelCatalogReadRepository, StoredModelCatalogEntry};
use crate::DataLayerError;

#[derive(Debug, Clone, Default)]
pub struct InMemoryModelCatalogReadRepository {
    entries: Vec<StoredModelCatalogEntry>,
}

impl InMemoryModelCatalogReadRepository {
    pub fn new(entries: Vec<StoredModelCatalogEntry>) -> Self {
        Self { entries }
    }
}

#[async_trait]
impl ModelCatalogReadRepository for InMemoryModelCatalogReadRepository {
    async fn list_model_catalog(&self) -> Result<Vec<StoredModelCatalogEntry>, DataLayerError> {
        Ok(self.entries.clone())
    }

    async fn read_model_catalog_detail(
        &self,
        global_model_name: &str,
    ) -> Result<Vec<StoredModelCatalogEntry>, DataLayerError> {
        Ok(self
            .entries
            .iter()
            .filter(|entry| entry.global_model_name == global_model_name)
            .take(256)
            .cloned()
            .collect())
    }
}
