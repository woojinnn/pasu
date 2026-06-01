//! Postgres-backed wallet state store.
//!
//! The first cloud schema intentionally stores [`WalletState`] snapshots as
//! JSONB. That keeps primitive state authoritative without prematurely
//! normalizing aggregate read models before their product contract settles.

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use simulation_state::primitives::Time;
use simulation_state::store::{StoreError, WalletStore};
use simulation_state::{WalletId, WalletState};

use crate::error::{DbError, DbResult};
use crate::stores::global::derive_user_id;

/// Cross-user identity store backed by Postgres.
#[derive(Clone, Debug)]
pub struct PostgresGlobalDb {
    pool: PgPool,
}

/// Per-user wallet state store backed by Postgres.
#[derive(Clone, Debug)]
pub struct PostgresWalletStore {
    pool: PgPool,
    user_id: String,
}

impl PostgresGlobalDb {
    /// Build from an existing Postgres connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Apply the initial Postgres schema.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(include_str!("../postgres_migrations/001_initial.sql"))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Insert or refresh an OAuth user, returning the deterministic user id.
    pub async fn upsert_user(&self, email: &str, provider: &str) -> DbResult<String> {
        let email = email.to_lowercase();
        let user_id = derive_user_id(&email);
        let now = unix_now_or_default();
        sqlx::query(
            "INSERT INTO users (user_id, email, provider, created_at, last_login_at)
             VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT(email) DO UPDATE SET last_login_at = excluded.last_login_at",
        )
        .bind(&user_id)
        .bind(&email)
        .bind(provider)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
        Ok(user_id)
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
}

#[async_trait]
impl WalletStore for PostgresWalletStore {
    async fn list_wallets(&self) -> Result<Vec<WalletId>, StoreError> {
        let rows =
            sqlx::query("SELECT state_json FROM wallet_states WHERE user_id = $1 ORDER BY address")
                .bind(&self.user_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| StoreError::Backend(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let value: serde_json::Value = row.get("state_json");
                serde_json::from_value::<WalletState>(value)
                    .map(|s| s.wallet_id)
                    .map_err(|e| StoreError::Backend(e.to_string()))
            })
            .collect()
    }

    async fn load(&self, id: &WalletId) -> Result<WalletState, StoreError> {
        let address = format!("{:#x}", id.address);
        let row =
            sqlx::query("SELECT state_json FROM wallet_states WHERE user_id = $1 AND address = $2")
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
        sqlx::query(
            "INSERT INTO wallets (user_id, address, chains, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT(user_id, address) DO UPDATE
             SET chains = excluded.chains, updated_at = excluded.updated_at",
        )
        .bind(&self.user_id)
        .bind(&address)
        .bind(&chains)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;

        sqlx::query(
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

    async fn reconcile_reports(&self, id: &WalletId, now: Time) -> Result<usize, StoreError> {
        let address = format!("{:#x}", id.address);
        let result = sqlx::query(
            "UPDATE execution_reports
             SET status = 'reconciled', reconciled_at = $1
             WHERE user_id = $2 AND wallet_address = $3 AND status = 'pending'",
        )
        .bind(i64::try_from(now.as_unix()).unwrap_or(i64::MAX))
        .bind(&self.user_id)
        .bind(address)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.to_string()))?;
        usize::try_from(result.rows_affected()).map_err(|e| StoreError::Backend(e.to_string()))
    }
}

fn unix_now_or_default() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
