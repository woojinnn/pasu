//! Runtime storage backend selection.
//!
//! The API still defaults to the local SQLite layout. This boundary keeps
//! main/server wiring from depending directly on that layout so Postgres can
//! be introduced behind the same shape in the next task.

use simulation_db::{GlobalDb, MultiUserStore};

use crate::config::ServerConfig;

/// Storage backend selected at process startup.
#[derive(Clone, Debug)]
pub enum StorageBackend {
    /// Local development backend: one global identity DB plus per-user SQLite
    /// wallet stores under `users/`.
    Sqlite {
        global_db: GlobalDb,
        multi_user: MultiUserStore,
    },
}

impl StorageBackend {
    /// Open the configured storage backend.
    pub fn open(
        config: &ServerConfig,
        home: &std::path::Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(url) = &config.database_url {
            if url.starts_with("postgres://") || url.starts_with("postgresql://") {
                return Err("Postgres backend is not wired until Task 6".into());
            }
        }

        Ok(Self::Sqlite {
            global_db: GlobalDb::open(home.join("global.db"))?,
            multi_user: MultiUserStore::new(home.join("users")),
        })
    }

    /// Cross-user identity DB handle.
    #[must_use]
    pub fn global_db(&self) -> GlobalDb {
        match self {
            Self::Sqlite { global_db, .. } => global_db.clone(),
        }
    }

    /// Per-user wallet store router.
    #[must_use]
    pub fn multi_user(&self) -> MultiUserStore {
        match self {
            Self::Sqlite { multi_user, .. } => multi_user.clone(),
        }
    }
}
