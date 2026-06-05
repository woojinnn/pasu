//! Postgres-backed wallet state store.
//!
//! The first cloud schema intentionally stores [`WalletState`] snapshots as
//! JSONB. That keeps primitive state authoritative without prematurely
//! normalizing aggregate read models before their product contract settles.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sqlx_core::error::Error as SqlxError;
use sqlx_core::migrate::Migrator;
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_postgres::{PgPool, PgPoolOptions, PgRow};

use policy_state::primitives::ChainId;
use policy_state::store::{StoreError, WalletStore};
use policy_state::{WalletId, WalletState};

use crate::error::{DbError, DbResult};

/// Location of the versioned `PostgreSQL` schema migrations.
/// Keep migrations as runtime files instead of using `sqlx::migrate!()`.
/// The macro pulls in `SQLx`'s macro crate, which currently exposes optional
/// `MySQL` dependencies to `cargo audit` even though this server is Postgres-only.
fn migrations_dir(override_dir: Option<PathBuf>) -> PathBuf {
    override_dir.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"))
}

fn postgres_migrations_path() -> PathBuf {
    migrations_dir(std::env::var_os("POLICY_DB_MIGRATIONS_DIR").map(PathBuf::from))
}

/// A row from the `users` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresUser {
    /// Stable server-side user id.
    pub user_id: String,
    /// Lowercased login email.
    pub email: String,
    /// OAuth provider name.
    pub provider: String,
    /// Creation timestamp as Unix seconds.
    pub created_at: i64,
    /// Last login timestamp as Unix seconds.
    pub last_login_at: i64,
}

/// Display metadata stored beside a wallet's authoritative state snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresWalletMetadata {
    /// Wallet address as a hex string.
    pub address: String,
    /// Chains tracked for this wallet.
    pub chains: Vec<ChainId>,
    /// Optional display label.
    pub label: Option<String>,
    /// Whether the user marked this wallet as owned.
    pub owned: bool,
    /// Whether the wallet is hidden from active views.
    pub archived: bool,
}

/// Cross-user identity store backed by `PostgreSQL`.
#[derive(Clone, Debug)]
pub struct PostgresGlobalDb {
    pool: PgPool,
}

/// Per-user wallet store factory backed by `PostgreSQL`.
#[derive(Clone, Debug)]
pub struct PostgresMultiUserStore {
    pool: PgPool,
}

/// Constructor input for [`PostgresMultiUserStore`].
#[derive(Clone, Debug)]
pub enum PostgresMultiUserStoreSource {
    /// Use an already-created `PostgreSQL` pool.
    Pool(PgPool),
    /// Compatibility path for old integration tests. The filesystem path is
    /// ignored; the store still uses `PostgreSQL` via `TEST_DATABASE_URL`.
    LegacyTestPath(PathBuf),
}

impl From<PgPool> for PostgresMultiUserStoreSource {
    fn from(pool: PgPool) -> Self {
        Self::Pool(pool)
    }
}

impl From<PathBuf> for PostgresMultiUserStoreSource {
    fn from(path: PathBuf) -> Self {
        Self::LegacyTestPath(path)
    }
}

/// Per-user wallet state store backed by `PostgreSQL`.
#[derive(Clone, Debug)]
pub struct PostgresWalletStore {
    pool: PgPool,
    user_id: String,
}

impl PostgresGlobalDb {
    /// Connect to `PostgreSQL`, apply migrations, and return the global store.
    ///
    /// # Errors
    ///
    /// Returns the underlying `SQLx` error if the connection or migration fails.
    pub async fn connect(database_url: &str) -> Result<Self, SqlxError> {
        let pool = connect_pool(database_url, 10, Duration::from_secs(10)).await?;
        let db = Self::new(pool);
        db.migrate().await?;
        Ok(db)
    }

    /// Build from an existing Postgres connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Compatibility constructor for integration tests that still pass a
    /// filesystem path. The path is ignored; it creates a lazy `PostgreSQL` pool
    /// from `TEST_DATABASE_URL`.
    ///
    /// # Errors
    ///
    /// Returns the underlying `SQLx` error if the lazy pool cannot be created.
    pub fn open(_path: impl AsRef<Path>) -> Result<Self, SqlxError> {
        Ok(Self::new(lazy_test_pool()?))
    }

    /// Apply the initial Postgres schema.
    ///
    /// # Errors
    ///
    /// Returns the underlying `SQLx` error if migration loading or execution fails.
    pub async fn migrate(&self) -> Result<(), SqlxError> {
        Migrator::new(postgres_migrations_path())
            .await
            .map_err(|e| SqlxError::Protocol(e.to_string()))?
            .run(&self.pool)
            .await
            .map_err(|e| SqlxError::Protocol(e.to_string()))?;
        Ok(())
    }

    /// Insert or refresh an OAuth user, returning the deterministic user id.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the upsert query fails.
    pub async fn upsert_user(&self, email: &str, provider: &str) -> DbResult<String> {
        let email = email.to_lowercase();
        let user_id = derive_user_id(&email);
        let now = unix_now_or_default();
        // `users` has two unique constraints — users_pkey (user_id) and
        // users_email_key (email) — and user_id is derived 1:1 from email, so a
        // same-email insert collides on both at once. `ON CONFLICT ... DO UPDATE`
        // names only one arbiter, leaving the other unguarded: concurrent
        // first-time logins then raced to insert the non-arbiter index and failed
        // with a duplicate-key 500. `ON CONFLICT DO NOTHING` (no target) makes
        // every unique index an arbiter, so the insert never hard-errors; the
        // follow-up UPDATE refreshes last_login_at on the now-committed row
        // (created_at and provider are intentionally left untouched).
        query(
            "INSERT INTO users (user_id, email, provider, created_at, last_login_at)
             VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(&user_id)
        .bind(&email)
        .bind(provider)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
        query("UPDATE users SET last_login_at = $1 WHERE user_id = $2")
            .bind(now)
            .bind(&user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Invariant(e.to_string()))?;
        Ok(user_id)
    }

    /// Look up a user by email.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the lookup query fails.
    pub async fn get_user_by_email(&self, email: &str) -> DbResult<Option<PostgresUser>> {
        let email = email.to_lowercase();
        query(
            "SELECT user_id, email, provider, created_at, last_login_at
             FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.as_ref().map(row_to_required_user))
        .map_err(|e| DbError::Invariant(e.to_string()))
    }

    /// Look up a user by stable user id.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the lookup query fails.
    pub async fn get_user_by_id(&self, user_id: &str) -> DbResult<Option<PostgresUser>> {
        query(
            "SELECT user_id, email, provider, created_at, last_login_at
             FROM users WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.as_ref().map(row_to_required_user))
        .map_err(|e| DbError::Invariant(e.to_string()))
    }

    /// Return every known OAuth user in deterministic order.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the list query fails.
    pub async fn list_users(&self) -> DbResult<Vec<PostgresUser>> {
        query(
            "SELECT user_id, email, provider, created_at, last_login_at
             FROM users ORDER BY email",
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.iter().map(row_to_required_user).collect())
        .map_err(|e| DbError::Invariant(e.to_string()))
    }

    /// Verify the underlying `PostgreSQL` pool can execute a trivial query.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the ping query fails.
    pub async fn ping(&self) -> DbResult<()> {
        query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| DbError::Invariant(e.to_string()))
    }

    /// Borrow the underlying pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Latest synced USD price + decimals for `(chain, address)` across EVERY
    /// wallet's holdings — a market-global fact that does not depend on the
    /// requesting wallet being registered.
    ///
    /// The price of a `(chain, contract)` token is identical across wallets, so
    /// this reuses the most-recently-synced holding that carries a price. Lets
    /// `oracle.usd_value` value a swap even when the *specific* wallet has never
    /// been synced, as long as the token's price has been synced anywhere.
    /// `address` is matched case-insensitively; returns `None` when no synced
    /// holding carries a price for that token yet.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the lookup query fails.
    pub async fn latest_token_price(
        &self,
        chain: &str,
        address: &str,
    ) -> DbResult<Option<TokenPriceFact>> {
        let address_lc = address.to_lowercase();
        // `tokens` is a JSONB array of `[key, holding]` pairs; `t->1` is the
        // holding. Native tokens carry no `key.address`, so the address filter
        // also excludes them. Most-recently-synced wallet wins on ties.
        let row = query(
            "SELECT (t->1->>'decimals')::int AS decimals, \
                    (t->1->'price_usd'->>'value') AS price \
             FROM wallet_states w, jsonb_array_elements(w.state_json->'tokens') AS t \
             WHERE lower(t->1->'key'->>'address') = $1 \
               AND (t->1->'key'->>'chain') = $2 \
               AND (t->1->'price_usd'->>'value') IS NOT NULL \
             ORDER BY w.updated_at DESC \
             LIMIT 1",
        )
        .bind(&address_lc)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

        let Some(row) = row else { return Ok(None) };
        let decimals: i32 = row
            .try_get("decimals")
            .map_err(|e| DbError::Invariant(e.to_string()))?;
        let price_usd: String = row
            .try_get("price")
            .map_err(|e| DbError::Invariant(e.to_string()))?;
        let decimals = u8::try_from(decimals)
            .map_err(|_| DbError::Invariant(format!("token decimals out of range: {decimals}")))?;
        Ok(Some(TokenPriceFact {
            price_usd,
            decimals,
        }))
    }

    /// Latest synced `decimals` for `(chain, address)` across EVERY wallet's
    /// holdings — a token-global fact independent of price. Lets
    /// `token.normalize_to_nano` rescale a swap amount with the token's REAL
    /// decimals instead of a hard-coded literal, so a token-amount cap works for
    /// any token (not just 6-decimals USDC). `address` is matched
    /// case-insensitively; returns `None` when the token has never been synced.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the lookup query fails.
    pub async fn latest_token_decimals(&self, chain: &str, address: &str) -> DbResult<Option<u8>> {
        let address_lc = address.to_lowercase();
        let row = query(
            "SELECT (t->1->>'decimals')::int AS decimals \
             FROM wallet_states w, jsonb_array_elements(w.state_json->'tokens') AS t \
             WHERE lower(t->1->'key'->>'address') = $1 \
               AND (t->1->'key'->>'chain') = $2 \
               AND (t->1->>'decimals') IS NOT NULL \
             ORDER BY w.updated_at DESC \
             LIMIT 1",
        )
        .bind(&address_lc)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

        let Some(row) = row else { return Ok(None) };
        let decimals: i32 = row
            .try_get("decimals")
            .map_err(|e| DbError::Invariant(e.to_string()))?;
        u8::try_from(decimals)
            .map(Some)
            .map_err(|_| DbError::Invariant(format!("token decimals out of range: {decimals}")))
    }
}

/// A market-global price fact: USD price (decimal string) + token decimals.
/// Sourced from synced holdings via [`PostgresGlobalDb::latest_token_price`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenPriceFact {
    /// USD price as a decimal string (e.g. `"0.99959644"`).
    pub price_usd: String,
    /// Token decimals (e.g. `6` for USDC).
    pub decimals: u8,
}

impl PostgresMultiUserStore {
    /// Build a per-user store factory from an existing `PostgreSQL` pool.
    ///
    /// # Panics
    ///
    /// Panics only for the legacy test-path variant if `TEST_DATABASE_URL` is not
    /// a valid lazy `PostgreSQL` URL.
    #[must_use]
    pub fn new(source: impl Into<PostgresMultiUserStoreSource>) -> Self {
        match source.into() {
            PostgresMultiUserStoreSource::Pool(pool) => Self { pool },
            PostgresMultiUserStoreSource::LegacyTestPath(_path) => Self {
                pool: lazy_test_pool().expect("valid TEST_DATABASE_URL for PostgreSQL test pool"),
            },
        }
    }

    /// Resolve a wallet store for one user namespace.
    ///
    /// # Errors
    ///
    /// This implementation currently cannot fail, but returns [`DbResult`] to
    /// keep the trait-compatible factory boundary.
    pub fn for_user(&self, user_id: &str) -> DbResult<Arc<PostgresWalletStore>> {
        Ok(Arc::new(PostgresWalletStore::new(
            self.pool.clone(),
            user_id.to_owned(),
        )))
    }

    /// Borrow the underlying pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl PostgresWalletStore {
    /// Build a per-user wallet store from an existing Postgres pool.
    #[must_use]
    pub fn new(pool: PgPool, user_id: impl Into<String>) -> Self {
        Self {
            pool,
            user_id: user_id.into(),
        }
    }

    /// Borrow the underlying pool.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// User namespace this store writes to.
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Return active wallet metadata for dashboard/list views.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the metadata query or chain decoding fails.
    pub async fn list_wallet_metadata(&self) -> Result<Vec<PostgresWalletMetadata>, StoreError> {
        let rows = query(
            "SELECT address, chains, label, owned, archived
             FROM wallets
             WHERE user_id = $1 AND archived = FALSE
             ORDER BY address",
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;

        rows.iter()
            .map(row_to_wallet_metadata)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Update mutable wallet metadata. Returns `false` when the wallet is absent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the metadata read or update query fails.
    pub async fn update_wallet_metadata(
        &self,
        address: &str,
        label: Option<Option<String>>,
        owned: Option<bool>,
    ) -> Result<bool, StoreError> {
        let existing = query(
            "SELECT label, owned FROM wallets
             WHERE user_id = $1 AND address = $2 AND archived = FALSE",
        )
        .bind(&self.user_id)
        .bind(address)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;
        let Some(row) = existing else {
            return Ok(false);
        };
        let current_label: Option<String> = row.get("label");
        let current_owned: bool = row.get("owned");
        let next_label = label.unwrap_or(current_label);
        let next_owned = owned.unwrap_or(current_owned);
        let now = unix_now_or_default();

        query(
            "UPDATE wallets
             SET label = $1, owned = $2, updated_at = $3
             WHERE user_id = $4 AND address = $5 AND archived = FALSE",
        )
        .bind(next_label)
        .bind(next_owned)
        .bind(now)
        .bind(&self.user_id)
        .bind(address)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;
        Ok(true)
    }

    /// Soft-delete a wallet from active views. The state row is kept for audit/recovery.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the archive query fails.
    pub async fn archive_wallet(&self, address: &str, now: i64) -> Result<bool, StoreError> {
        let result = query(
            "UPDATE wallets
             SET archived = TRUE, updated_at = $1
             WHERE user_id = $2 AND address = $3 AND archived = FALSE",
        )
        .bind(now)
        .bind(&self.user_id)
        .bind(address)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl WalletStore for PostgresWalletStore {
    async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
        let rows = query(
            "SELECT address, chains FROM wallets
             WHERE user_id = $1 AND archived = FALSE
             ORDER BY address",
        )
        .bind(&self.user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let address: String = row.get("address");
                let chains_value: serde_json::Value = row.get("chains");
                let chains = serde_json::from_value::<Vec<ChainId>>(chains_value)
                    .map_err(|e| StoreError::Backend(e.to_string()))?;
                let address = address
                    .parse()
                    .map_err(|e| StoreError::Backend(format!("wallet address: {e}")))?;
                Ok(WalletId::new(address, chains))
            })
            .collect()
    }

    async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
        let address = format!("{:#x}", id.address);
        let row = query("SELECT state_json FROM wallet_states WHERE user_id = $1 AND address = $2")
            .bind(&self.user_id)
            .bind(address)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.to_string()))?;

        row.map_or_else(
            || Ok(WalletState::new(id.clone())),
            |row| {
                let value: serde_json::Value = row.get("state_json");
                serde_json::from_value(value).map_err(|e| StoreError::Backend(e.to_string()))
            },
        )
    }

    async fn save(&self, state: &WalletState) -> Result<(), StoreError> {
        let address = format!("{:#x}", state.wallet_id.address);
        let chains = serde_json::to_value(&state.wallet_id.chains)
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        let state_json =
            serde_json::to_value(state).map_err(|e| StoreError::Backend(e.to_string()))?;
        let now = unix_now_or_default();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Backend(e.to_string()))?;
        query(
            "INSERT INTO wallets (user_id, address, chains, owned, created_at, updated_at)
             VALUES ($1, $2, $3, TRUE, $4, $4)
             ON CONFLICT(user_id, address) DO UPDATE
             SET chains = excluded.chains,
                 archived = FALSE,
                 updated_at = excluded.updated_at",
        )
        .bind(&self.user_id)
        .bind(&address)
        .bind(&chains)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;

        query(
            "INSERT INTO wallet_states (user_id, address, state_json, updated_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(user_id, address) DO UPDATE
             SET state_json = excluded.state_json, updated_at = excluded.updated_at",
        )
        .bind(&self.user_id)
        .bind(&address)
        .bind(&state_json)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| StoreError::Backend(e.to_string()))
    }
}

fn row_to_wallet_metadata(row: &PgRow) -> Result<PostgresWalletMetadata, StoreError> {
    let chains_value: serde_json::Value = row.get("chains");
    let chains = serde_json::from_value::<Vec<ChainId>>(chains_value)
        .map_err(|e| StoreError::Backend(e.to_string()))?;
    Ok(PostgresWalletMetadata {
        address: row.get("address"),
        chains,
        label: row.get("label"),
        owned: row.get("owned"),
        archived: row.get("archived"),
    })
}

fn unix_now_or_default() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Connect to `PostgreSQL` with the default policy-server pool settings.
///
/// # Errors
///
/// Returns the underlying `SQLx` error if the pool cannot connect.
pub async fn connect_pool(
    database_url: &str,
    max_connections: u32,
    acquire_timeout: Duration,
) -> Result<PgPool, SqlxError> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(acquire_timeout)
        .connect(database_url)
        .await
}

fn lazy_test_pool() -> Result<PgPool, SqlxError> {
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://scopeball:scopeball@127.0.0.1:5432/scopeball_test".to_owned()
    });
    PgPoolOptions::new().max_connections(5).connect_lazy(&url)
}

fn row_to_required_user(row: &PgRow) -> PostgresUser {
    PostgresUser {
        user_id: row.get("user_id"),
        email: row.get("email"),
        provider: row.get("provider"),
        created_at: row.get("created_at"),
        last_login_at: row.get("last_login_at"),
    }
}

/// Deterministic short id from a lower-cased email.
/// `u_` prefix + first 12 hex chars of blake3(email). Collisions inside
/// 12 hex chars (48 bits) are astronomically unlikely for the expected scale.
#[must_use]
pub fn derive_user_id(email_lower: &str) -> String {
    let h = blake3::hash(email_lower.as_bytes());
    let hex = hex::encode(h.as_bytes());
    format!("u_{}", &hex[..12])
}

#[cfg(test)]
mod tests {
    use super::{derive_user_id, migrations_dir, postgres_migrations_path};
    use std::path::PathBuf;

    #[test]
    fn derive_user_id_is_deterministic_and_canonical() {
        let a = derive_user_id("alice@example.com");
        let b = derive_user_id("alice@example.com");
        assert_eq!(a, b);
        assert!(a.starts_with("u_"));
        assert_eq!(a.len(), 14);
    }

    #[test]
    fn postgres_migrations_include_initial_schema() {
        let initial = postgres_migrations_path().join("0001_initial.sql");
        let sql = std::fs::read_to_string(initial).expect("initial migration exists");
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
    }

    #[test]
    fn migrations_dir_uses_override_when_present() {
        assert_eq!(
            migrations_dir(Some(PathBuf::from("/srv/app/migrations"))),
            PathBuf::from("/srv/app/migrations")
        );
    }

    #[test]
    fn migrations_dir_falls_back_to_manifest_dir() {
        let p = migrations_dir(None);
        assert!(p.ends_with("migrations"));
    }
}
