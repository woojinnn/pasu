//! `WalletStore` implementations backed by this crate's SQLite layer.
//!
//! The trait itself lives in `simulation-state`; this module just provides
//! the concrete SQLite-backed type the server / scheduler / CLI wire up.
//!
//! Phase 4 adds two siblings:
//! - [`global`]: the cross-user identity DB (email ↔ user_id).
//! - [`multi_user`]: per-user `SqliteWalletStore` cache keyed by user_id.

pub mod global;
pub mod multi_user;
pub mod sqlite;

pub use global::{GlobalDb, User};
pub use multi_user::MultiUserStore;
pub use sqlite::SqliteWalletStore;
