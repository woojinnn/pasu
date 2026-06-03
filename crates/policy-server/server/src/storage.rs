//! Runtime storage backend wiring.
//! The policy server now has one durable backend: `PostgreSQL`. State remains
//! primitive-first in `wallet_states.state_json`; mutable wallet metadata and
//! sync cursors live in adjacent tables under the same user namespace.

use std::sync::Arc;
use std::time::Duration;

use policy_db::stores::postgres::connect_pool;
use policy_db::{GlobalDb, MultiUserStore};
use policy_state::WalletStore;

use crate::config::ServerConfig;

/// Storage handles selected at process startup.
#[derive(Clone, Debug)]
pub struct StorageBackend {
    global_db: GlobalDb,
    multi_user: MultiUserStore,
}

/// Capped exponential backoff for the Nth (1-based) retry attempt.
fn backoff_delay(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(16));
    base.saturating_mul(factor).min(cap)
}

impl StorageBackend {
    /// Connect to `PostgreSQL` and apply the schema migrations.
    pub async fn open(config: &ServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_with_options(config, config.run_migrations_on_startup).await
    }

    /// Connect to `PostgreSQL`, optionally applying schema migrations.
    pub async fn open_with_options(
        config: &ServerConfig,
        migrate: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let database_url = config.database_url.as_deref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "DATABASE_URL is required; local-file storage has been removed",
            )
        })?;
        if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "DATABASE_URL must use postgres:// or postgresql://",
            )
            .into());
        }

        let pool = {
            let mut attempt: u32 = 0;
            loop {
                match connect_pool(
                    database_url,
                    config.db_max_connections,
                    Duration::from_secs(config.db_acquire_timeout_secs),
                )
                .await
                {
                    Ok(pool) => break pool,
                    Err(e) => {
                        attempt += 1;
                        if attempt > config.db_connect_max_retries {
                            return Err(e.into());
                        }
                        let delay = backoff_delay(
                            attempt,
                            Duration::from_secs(config.db_connect_backoff_secs),
                            Duration::from_secs(30),
                        );
                        tracing::warn!(attempt, ?delay, error = %e, "db connect failed; retrying");
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        };
        let global_db = GlobalDb::new(pool.clone());
        if migrate {
            global_db.migrate().await?;
        }
        let multi_user = MultiUserStore::new(pool);
        Ok(Self {
            global_db,
            multi_user,
        })
    }

    /// Cross-user identity DB handle.
    #[must_use]
    pub fn global_db(&self) -> GlobalDb {
        self.global_db.clone()
    }

    /// Per-user wallet store router.
    #[must_use]
    pub fn multi_user(&self) -> MultiUserStore {
        self.multi_user.clone()
    }

    /// User ids visible to background workers.
    pub async fn list_user_ids(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(self
            .global_db
            .list_users()
            .await?
            .into_iter()
            .map(|user| user.user_id)
            .collect())
    }

    /// Open the wallet store for one authenticated user's namespace.
    pub fn wallet_store_for_user(
        &self,
        user_id: &str,
    ) -> Result<Arc<dyn WalletStore>, Box<dyn std::error::Error>> {
        Ok(self.multi_user.for_user(user_id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::backoff_delay;
    use std::time::Duration;

    #[test]
    fn backoff_grows_then_caps() {
        let base = Duration::from_secs(5);
        let cap = Duration::from_secs(30);
        assert_eq!(backoff_delay(1, base, cap), Duration::from_secs(5));
        assert_eq!(backoff_delay(2, base, cap), Duration::from_secs(10));
        assert_eq!(backoff_delay(3, base, cap), Duration::from_secs(20));
        assert_eq!(backoff_delay(4, base, cap), cap);
        assert_eq!(backoff_delay(99, base, cap), cap);
    }
}
