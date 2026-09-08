mod memory;

pub use aether_data_contracts::repository::model_catalog::{
    ModelCatalogReadRepository, StoredModelCatalogEntry,
};
#[cfg(feature = "postgres")]
pub use aether_data_postgres::PostgresModelCatalogReadRepository;
pub use memory::InMemoryModelCatalogReadRepository;
