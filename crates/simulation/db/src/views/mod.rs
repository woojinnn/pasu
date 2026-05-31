//! Aggregate read/write helpers that span multiple `repositories` tables.
//!
//! Where `repositories::*` exposes per-table CRUD, `views::*` assembles a
//! whole domain object (currently just [`wallet_state`]) by composing those
//! primitives inside one transaction.

pub mod wallet_state;

pub use wallet_state::{load_wallet_state, save_wallet_state};
