mod in_memory;
mod traits;
mod types;

pub use in_memory::InMemoryStorage;
pub use traits::{AppendLogStore, DocumentStore, KeyValueStore, StorageBackend};
pub use types::{AppendLogEntry, StorageDocument, StorageError, StorageScope, StorageValue};

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStorage;

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;

pub fn crate_name() -> &'static str {
    "codel00p-storage"
}

#[cfg(test)]
mod tests;
