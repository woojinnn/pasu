//! Runtime storage backend selection.
//!
//! The API still defaults to the local SQLite layout. This boundary keeps
//! main/server wiring from depending directly on that layout so Postgres can
//! be introduced behind the same shape in the next task.

use std::sync::Arc;

use simulation_db::{GlobalDb, MultiUserStore};
use simulation_state::WalletStore;

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

    /// User ids visible to background workers.
    pub fn list_user_ids(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        match self {
            Self::Sqlite { global_db, .. } => Ok(global_db
                .list_users()?
                .into_iter()
                .map(|user| user.user_id)
                .collect()),
        }
    }

    /// Open the wallet store for one authenticated user's namespace.
    pub fn wallet_store_for_user(
        &self,
        user_id: &str,
    ) -> Result<Arc<dyn WalletStore>, Box<dyn std::error::Error>> {
        match self {
            Self::Sqlite { multi_user, .. } => Ok(multi_user.for_user(user_id)?),
        }
    }
}
