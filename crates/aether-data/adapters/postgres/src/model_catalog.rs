use async_trait::async_trait;
use sqlx::{postgres::PgRow, PgPool, Row};

use crate::{error::SqlxResultExt, DataLayerError};
use aether_data_contracts::repository::model_catalog::{
    ModelCatalogReadRepository, StoredModelCatalogEntry,
};

const LIST_MODEL_CATALOG: &str = r#"
SELECT gm.id AS global_model_id, gm.name AS global_model_name,
       gm.config AS global_model_config,
       gm.supported_capabilities AS global_model_supported_capabilities,
       gm.is_active AS global_model_is_active,
       m.id AS provider_model_id, m.provider_model_name,
       m.provider_model_mappings, m.config AS provider_model_config,
       m.is_active AS provider_model_is_active,
       COALESCE(m.is_available, TRUE) AS provider_model_is_available,
       p.id AS provider_id, p.name AS provider_name, p.provider_type,
       p.is_active AS provider_is_active
FROM models m
JOIN global_models gm ON gm.id = m.global_model_id
JOIN providers p ON p.id = m.provider_id
ORDER BY gm.name, p.id, m.id
"#;

const READ_MODEL_CATALOG_DETAIL: &str = r#"
SELECT gm.id AS global_model_id, gm.name AS global_model_name,
       gm.config AS global_model_config, gm.supported_capabilities AS global_model_supported_capabilities,
       gm.is_active AS global_model_is_active, m.id AS provider_model_id, m.provider_model_name,
       m.provider_model_mappings, m.config AS provider_model_config, m.is_active AS provider_model_is_active,
       COALESCE(m.is_available, TRUE) AS provider_model_is_available,
       p.id AS provider_id, p.name AS provider_name, p.provider_type, p.is_active AS provider_is_active
FROM models m JOIN global_models gm ON gm.id = m.global_model_id JOIN providers p ON p.id = m.provider_id
WHERE gm.name = $1 ORDER BY p.id, m.id LIMIT 256
"#;

#[derive(Debug, Clone)]
pub struct PostgresModelCatalogReadRepository {
    pool: PgPool,
}

impl PostgresModelCatalogReadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn decode_rows(rows: Vec<PgRow>) -> Result<Vec<StoredModelCatalogEntry>, DataLayerError> {
    rows.into_iter()
        .map(|row| {
            Ok(StoredModelCatalogEntry {
                global_model_id: row.try_get("global_model_id").map_postgres_err()?,
                global_model_name: row.try_get("global_model_name").map_postgres_err()?,
                global_model_config: row.try_get("global_model_config").map_postgres_err()?,
                global_model_supported_capabilities: row
                    .try_get("global_model_supported_capabilities")
                    .map_postgres_err()?,
                global_model_is_active: row.try_get("global_model_is_active").map_postgres_err()?,
                provider_model_id: row.try_get("provider_model_id").map_postgres_err()?,
                provider_model_name: row.try_get("provider_model_name").map_postgres_err()?,
                provider_model_mappings: row
                    .try_get("provider_model_mappings")
                    .map_postgres_err()?,
                provider_model_config: row.try_get("provider_model_config").map_postgres_err()?,
                provider_model_is_active: row
                    .try_get("provider_model_is_active")
                    .map_postgres_err()?,
                provider_model_is_available: row
                    .try_get("provider_model_is_available")
                    .map_postgres_err()?,
                provider_id: row.try_get("provider_id").map_postgres_err()?,
                provider_name: row.try_get("provider_name").map_postgres_err()?,
                provider_type: row.try_get("provider_type").map_postgres_err()?,
                provider_is_active: row.try_get("provider_is_active").map_postgres_err()?,
            })
        })
        .collect()
}

#[async_trait]
impl ModelCatalogReadRepository for PostgresModelCatalogReadRepository {
    async fn list_model_catalog(&self) -> Result<Vec<StoredModelCatalogEntry>, DataLayerError> {
        let rows = sqlx::query(LIST_MODEL_CATALOG)
            .fetch_all(&self.pool)
            .await
            .map_postgres_err()?;
        decode_rows(rows)
    }

    async fn read_model_catalog_detail(
        &self,
        global_model_name: &str,
    ) -> Result<Vec<StoredModelCatalogEntry>, DataLayerError> {
        let rows = sqlx::query(READ_MODEL_CATALOG_DETAIL)
            .bind(global_model_name)
            .fetch_all(&self.pool)
            .await
            .map_postgres_err()?;
        decode_rows(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{LIST_MODEL_CATALOG, READ_MODEL_CATALOG_DETAIL};

    #[test]
    fn catalog_query_is_independent_of_endpoint_and_key_cardinality() {
        let query = LIST_MODEL_CATALOG.to_ascii_lowercase();
        assert!(!query.contains("provider_endpoints"));
        assert!(!query.contains("provider_api_keys"));
        assert!(!query.contains("candidate_selection"));
    }

    #[test]
    fn detail_query_is_bounded_and_endpoint_independent() {
        let query = READ_MODEL_CATALOG_DETAIL.to_ascii_lowercase();
        assert!(query.contains("where gm.name = $1"));
        assert!(query.contains("limit 256"));
        assert!(!query.contains("provider_endpoints"));
        assert!(!query.contains("provider_api_keys"));
    }
}
