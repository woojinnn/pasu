//! `simulation-db` — PostgreSQL persistence for policy-server wallet state.
//!
//! The cloud server treats PostgreSQL as the only durable database. Wallet
//! state snapshots are stored as JSONB first; aggregate/read-model tables can
//! be added later once their product contract is stable.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
// Phase 1 본문은 동작 우선 — 후속에서 doc 보강.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(missing_docs)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_long_first_doc_paragraph)]
#![allow(clippy::format_push_string)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

pub mod error;
pub mod stores;

pub use error::{DbError, DbResult};
pub use stores::{
    derive_user_id, GlobalDb, MultiUserStore, PostgresWalletMetadata, PostgresWalletStore, User,
};
