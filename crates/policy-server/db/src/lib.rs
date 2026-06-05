//! `policy-db` provides `PostgreSQL` persistence for policy-server wallet state.
//!
//! The cloud server treats `PostgreSQL` as the only durable database. Wallet
//! state snapshots are stored as JSONB first; aggregate/read-model tables can
//! be added later once their product contract is stable.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]

/// Database error types.
pub mod error;
/// Store implementations backed by durable databases.
pub mod stores;

pub use error::{DbError, DbResult};
pub use stores::{
    derive_user_id, market, GlobalDb, MultiUserStore, PostgresWalletMetadata, PostgresWalletStore,
    User,
};
