//! Per-user [`SqliteWalletStore`] cache.
//!
//! Production wiring: one DB file per user under
//! `~/.scopeball/users/<user_id>/scopeball.db`. Every request resolves
//! the right store via [`MultiUserStore::for_user`], which opens the file
//! (running migrations the first time) and caches the `Arc` for the
//! process lifetime.
//!
//! Why an in-process cache?
//! - `SqliteWalletStore` is `Arc<Mutex<Connection>>` under the hood, so a
//!   second `open` would be a second connection — wasteful for hot users.
//! - `WalletStore` is shared via `Arc<dyn WalletStore>`, so handing back
//!   the same Arc is exactly what callers expect.
//!
//! Memory pressure is bounded by the active user count (one user ≈ one
//! Connection ≈ a few KB of buffers); no LRU is needed at typical scales.
//! If that ever changes, swap the `HashMap` for an `lru` crate.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use std::collections::HashMap;

use crate::error::DbResult;
use crate::stores::SqliteWalletStore;

/// Builds and caches one [`SqliteWalletStore`] per `user_id`.
///
/// Cloneable: clones share the same cache (the inner state is `Arc`-wrapped).
#[derive(Clone, Debug)]
pub struct MultiUserStore {
    home: PathBuf,
    cache: Arc<RwLock<HashMap<String, Arc<SqliteWalletStore>>>>,
}

impl MultiUserStore {
    /// `home` is the directory that contains per-user subdirectories. The
    /// canonical value is `~/.scopeball/users`. The directory is created
    /// lazily on first user open.
    #[must_use]
    pub fn new(home: impl AsRef<Path>) -> Self {
        Self {
            home: home.as_ref().to_path_buf(),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolve the store for `user_id`, opening (and migrating) the file
    /// on the first call. Subsequent calls return the cached `Arc`.
    // The write-lock guard's lifetime is intentionally kept until after the
    // cache.entry() call returns — clippy's significant-drop heuristic
    // doesn't like that, but releasing earlier would race with another
    // `for_user(user_id)` writer.
    #[allow(clippy::significant_drop_tightening)]
    pub fn for_user(&self, user_id: &str) -> DbResult<Arc<SqliteWalletStore>> {
        // Drop the read lock before opening — opening is slow (file IO +
        // migrations on first call) and blocking the lock would serialise
        // every concurrent caller.
        {
            let guard = self.cache.read().expect("MultiUserStore cache poisoned");
            if let Some(store) = guard.get(user_id) {
                return Ok(store.clone());
            }
        }
        let path = self.path_for(user_id);
        let store = Arc::new(SqliteWalletStore::open(&path)?);
        // Race with another `for_user(user_id)` is fine — `or_insert_with`
        // resolves to whichever one wrote first.
        let mut cache = self.cache.write().expect("MultiUserStore cache poisoned");
        let entry = cache
            .entry(user_id.to_string())
            .or_insert_with(|| store.clone());
        Ok(entry.clone())
    }

    /// Build the on-disk path for `user_id` (does not open / create).
    #[must_use]
    pub fn path_for(&self, user_id: &str) -> PathBuf {
        self.home.join(user_id).join("scopeball.db")
    }

    /// How many users this process has touched. Test/diagnostic use.
    #[must_use]
    pub fn cached_user_count(&self) -> usize {
        self.cache
            .read()
            .expect("MultiUserStore cache poisoned")
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_user_creates_per_user_files() {
        let tmp = tempfile::tempdir().unwrap();
        let mu = MultiUserStore::new(tmp.path());

        let _ = mu.for_user("u_alice").unwrap();
        let _ = mu.for_user("u_bob").unwrap();

        assert!(tmp.path().join("u_alice/scopeball.db").exists());
        assert!(tmp.path().join("u_bob/scopeball.db").exists());
        assert_eq!(mu.cached_user_count(), 2);
    }

    #[test]
    fn for_user_returns_same_arc_on_repeat_calls() {
        let tmp = tempfile::tempdir().unwrap();
        let mu = MultiUserStore::new(tmp.path());

        let a = mu.for_user("u_alice").unwrap();
        let b = mu.for_user("u_alice").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "cached store must be the same Arc");
        assert_eq!(mu.cached_user_count(), 1);
    }

    #[test]
    fn cloned_multi_user_store_shares_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let mu1 = MultiUserStore::new(tmp.path());
        let mu2 = mu1.clone();

        let _ = mu1.for_user("u_alice").unwrap();
        assert_eq!(
            mu2.cached_user_count(),
            1,
            "clone must see the same cache entry"
        );
    }
}
