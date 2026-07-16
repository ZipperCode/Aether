mod memory;

pub use aether_data_contracts::repository::model_catalog::{
    ModelCatalogReadRepository, StoredModelCatalogEntry,
};
#[cfg(feature = "mysql")]
pub use aether_data_mysql::MysqlModelCatalogReadRepository;
#[cfg(feature = "postgres")]
pub use aether_data_postgres::PostgresModelCatalogReadRepository;
#[cfg(feature = "sqlite")]
pub use aether_data_sqlite::SqliteModelCatalogReadRepository;
pub use memory::InMemoryModelCatalogReadRepository;
