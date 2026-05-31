//! SQLite-backed [`WalletStore`] implementation.
//!
//! Wraps a [`Pool`] and delegates per-call to [`views::wallet_state`]. The
//! `WalletStore` trait is async (the server runs on tokio), so blocking SQL
//! is dispatched onto `tokio::task::spawn_blocking`; this keeps the runtime
//! responsive while a long query is in flight and avoids the "block the
//! reactor with rusqlite" footgun.
//!
//! Typical wiring:
//! ```ignore
//! let store = SqliteWalletStore::open("~/.scopeball/users/anon/scopeball.db")?;
//! let state: Arc<dyn WalletStore> = Arc::new(store);
//! ```

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use simulation_state::store::{StoreError, WalletStore};
use simulation_state::{WalletId, WalletState};

use crate::error::DbResult;
use crate::pool::Pool;
use crate::views;

/// SQLite-backed [`WalletStore`]. Cheaply cloneable (the inner [`Pool`] is
/// an `Arc` of a single shared connection — see [`Pool`] for the rationale).
#[derive(Clone, Debug)]
pub struct SqliteWalletStore {
    pool: Pool,
}

impl SqliteWalletStore {
    /// Opens (or creates) the DB file at `path` and runs all pending
    /// migrations so the store is immediately ready for `load` / `save`.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let pool = Pool::open(path)?;
        crate::run_migrations(&pool)?;
        Ok(Self { pool })
    }

    /// In-memory store with migrations applied. Test / scratchpad use.
    #[must_use]
    pub fn open_in_memory() -> Self {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).expect("run_migrations on in-memory pool");
        Self { pool }
    }

    /// Construct from an existing pool (callers that already opened one).
    #[must_use]
    pub const fn from_pool(pool: Pool) -> Self {
        Self { pool }
    }

    /// Borrow the underlying pool — handy for callers that want to run
    /// extra per-table queries without a second `Pool::open`.
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }
}

#[async_trait]
impl WalletStore for SqliteWalletStore {
    async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
        let pool = self.pool.clone();
        run_blocking(move || pool.with_tx(views::wallet_state::list_wallets)).await
    }

    async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
        let pool = self.pool.clone();
        let id = id.clone();
        run_blocking(move || pool.with_tx(|tx| views::load_wallet_state(tx, &id))).await
    }

    async fn save(&self, state: &WalletState) -> Result<(), StoreError> {
        let pool = self.pool.clone();
        let state = state.clone();
        run_blocking(move || pool.with_tx(|tx| views::save_wallet_state(tx, &state))).await
    }
}

/// Run a blocking `DbResult<T>` closure on tokio's blocking pool and map
/// the result into [`StoreError`].
async fn run_blocking<T, F>(f: F) -> Result<T, StoreError>
where
    T: Send + 'static,
    F: FnOnce() -> DbResult<T> + Send + 'static,
{
    let joined = tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| StoreError::Backend(format!("join error: {e}")))?;
    joined.map_err(|e| StoreError::Backend(e.to_string()))
}

// `Arc<SqliteWalletStore>` is a common usage shape; keep an inherent helper
// so callers can write `SqliteWalletStore::open(...).map(Arc::new)?` and
// pass the Arc directly where `Arc<dyn WalletStore>` is expected.
impl SqliteWalletStore {
    /// Open and wrap in an `Arc<dyn WalletStore>` — the shape `AppState`
    /// holds.
    pub fn open_as_arc(path: impl AsRef<Path>) -> DbResult<Arc<dyn WalletStore>> {
        Self::open(path).map(|s| Arc::new(s) as Arc<dyn WalletStore>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use simulation_state::primitives::{Address, BlockHeight, ChainId};

    fn sample_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [ChainId::ethereum_mainnet()],
        )
    }

    #[tokio::test]
    async fn unseen_wallet_loads_empty_state() {
        let store = SqliteWalletStore::open_in_memory();
        let id = sample_id();
        let state = store.load(&id).await.unwrap();
        assert_eq!(state, WalletState::new(id));
    }

    #[tokio::test]
    async fn save_then_load_round_trip() {
        let store = SqliteWalletStore::open_in_memory();
        let id = sample_id();
        let mut seed = WalletState::new(id.clone());
        seed.block_heights.insert(
            ChainId::ethereum_mainnet(),
            BlockHeight {
                number: 19_000_000,
                time: 1_700_000_000,
            },
        );

        store.save(&seed).await.unwrap();
        let back = store.load(&id).await.unwrap();
        assert_eq!(back, seed);
    }

    #[tokio::test]
    async fn list_wallets_returns_saved() {
        let store = SqliteWalletStore::open_in_memory();
        let id = sample_id();
        store.save(&WalletState::new(id.clone())).await.unwrap();
        let listed = store.list_wallets().await.unwrap();
        assert_eq!(listed, vec![id]);
    }
}
