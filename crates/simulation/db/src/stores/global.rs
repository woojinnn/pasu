//! Cross-user identity DB (the global `~/.scopeball/global.db`).
//!
//! Holds the canonical `users` table — the only place email and user_id are
//! mapped to each other. Per-user wallet DBs (`~/.scopeball/users/<id>/…`)
//! do not duplicate this; they only know their own `user_id`.
//!
//! The store is intentionally minimal: upsert on OAuth callback, lookup by
//! email or id. No password fields, no email verification — those live in
//! the OAuth provider.

use std::path::Path;

use rusqlite::{params, OptionalExtension};

use crate::error::DbResult;
use crate::pool::Pool;

/// A row from the `users` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub user_id: String,
    pub email: String,
    pub provider: String,
    pub created_at: i64,
    pub last_login_at: i64,
}

/// Pool + global-schema migrations applied.
#[derive(Clone, Debug)]
pub struct GlobalDb {
    pool: Pool,
}

impl GlobalDb {
    /// Open (or create) the file at `path` and apply the global-DB
    /// migrations so the `users` table is ready.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Self> {
        let pool = Pool::open(path)?;
        crate::migrate::run_global(&pool)?;
        Ok(Self { pool })
    }

    /// In-memory variant for tests / scratchpad use.
    #[must_use]
    pub fn open_in_memory() -> Self {
        let pool = Pool::open_in_memory();
        crate::migrate::run_global(&pool).expect("global migrations on in-memory pool");
        Self { pool }
    }

    /// Insert the user if their email is new, otherwise bump
    /// `last_login_at`. Returns the canonical `user_id` either way.
    ///
    /// The id is **deterministic** — same email always yields the same id
    /// — so callers don't need to read it back from the DB for routing.
    pub fn upsert_user(&self, email: &str, provider: &str) -> DbResult<String> {
        let email = email.to_lowercase();
        let user_id = derive_user_id(&email);
        let now = unix_now_or_default();
        self.pool.with_tx(|tx| {
            tx.execute(
                "INSERT INTO users (user_id, email, provider, created_at, last_login_at) \
                 VALUES (?1, ?2, ?3, ?4, ?4) \
                 ON CONFLICT(email) DO UPDATE SET last_login_at = excluded.last_login_at",
                params![user_id, email, provider, now],
            )?;
            Ok(())
        })?;
        Ok(user_id)
    }

    /// Look up a user by email (lower-cased internally).
    pub fn get_user_by_email(&self, email: &str) -> DbResult<Option<User>> {
        let email = email.to_lowercase();
        self.pool.with_conn(|c| {
            c.query_row(
                "SELECT user_id, email, provider, created_at, last_login_at \
                 FROM users WHERE email = ?1",
                params![email],
                row_to_user,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    /// Look up a user by their stable `user_id`.
    pub fn get_user_by_id(&self, user_id: &str) -> DbResult<Option<User>> {
        self.pool.with_conn(|c| {
            c.query_row(
                "SELECT user_id, email, provider, created_at, last_login_at \
                 FROM users WHERE user_id = ?1",
                params![user_id],
                row_to_user,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    /// Return every known OAuth user in deterministic order.
    pub fn list_users(&self) -> DbResult<Vec<User>> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT user_id, email, provider, created_at, last_login_at \
                 FROM users ORDER BY email",
            )?;
            let rows = stmt.query_map([], row_to_user)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// Borrow the underlying pool (for tests / diagnostics).
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }
}

fn row_to_user(r: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        user_id: r.get(0)?,
        email: r.get(1)?,
        provider: r.get(2)?,
        created_at: r.get(3)?,
        last_login_at: r.get(4)?,
    })
}

/// Deterministic short id from a (already lower-cased) email.
///
/// `u_` prefix + first 12 hex chars of blake3(email). Collisions inside
/// 12 hex chars (48 bits) are astronomically unlikely for the scale we
/// expect; if we ever need more headroom, widen the slice.
#[must_use]
pub fn derive_user_id(email_lower: &str) -> String {
    let h = blake3::hash(email_lower.as_bytes());
    let hex = hex::encode(h.as_bytes());
    format!("u_{}", &hex[..12])
}

fn unix_now_or_default() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    )
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_user_id_is_deterministic_and_canonical() {
        let a = derive_user_id("alice@example.com");
        let b = derive_user_id("alice@example.com");
        assert_eq!(a, b);
        assert!(a.starts_with("u_"));
        assert_eq!(a.len(), 14); // "u_" + 12 hex
    }

    #[test]
    fn upsert_creates_then_updates_last_login() {
        let db = GlobalDb::open_in_memory();
        let id1 = db.upsert_user("alice@example.com", "google").unwrap();
        let id2 = db.upsert_user("ALICE@example.com", "google").unwrap();
        assert_eq!(id1, id2, "email is case-insensitive");

        let user = db.get_user_by_id(&id1).unwrap().unwrap();
        assert_eq!(user.email, "alice@example.com");
        assert_eq!(user.provider, "google");
    }

    #[test]
    fn get_by_email_and_id_round_trip() {
        let db = GlobalDb::open_in_memory();
        let id = db.upsert_user("bob@example.com", "github").unwrap();

        let by_email = db.get_user_by_email("bob@example.com").unwrap().unwrap();
        let by_id = db.get_user_by_id(&id).unwrap().unwrap();
        assert_eq!(by_email, by_id);
    }

    #[test]
    fn unknown_user_returns_none() {
        let db = GlobalDb::open_in_memory();
        assert!(db.get_user_by_email("nobody@x.com").unwrap().is_none());
        assert!(db.get_user_by_id("u_doesnotexist").unwrap().is_none());
    }

    #[test]
    fn different_emails_yield_different_ids() {
        assert_ne!(
            derive_user_id("alice@example.com"),
            derive_user_id("bob@example.com")
        );
    }

    #[test]
    fn list_users_returns_all_users() {
        let db = GlobalDb::open_in_memory();
        let alice = db.upsert_user("alice@example.com", "google").unwrap();
        let bob = db.upsert_user("bob@example.com", "google").unwrap();

        let users = db.list_users().unwrap();
        let ids: Vec<_> = users.into_iter().map(|u| u.user_id).collect();

        assert_eq!(ids, vec![alice, bob]);
    }
}
