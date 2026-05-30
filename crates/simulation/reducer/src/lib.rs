//! `simulation-reducer` — pure function that applies an `Action` to a `WalletState`.
//!
//! No external IO (no DB, no RPC, no clock). Inputs: `state` + `action` + `eval`.
//! Output: `(newState, StateDelta)`. wasm-buildable.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
// Phase 2 skeleton: every reducer / helper returns `Result` but the concrete
// error contract is not yet decided (bodies are `todo!()`). Lift this allow
// as implementations land and proper `# Errors` sections can be written.
#![allow(clippy::missing_errors_doc)]

pub mod action;
pub mod apply;
pub mod effect;
pub mod error;
pub mod helpers;

pub use action::{
    Action, ActionBody, ActionMeta, ActionNature, AirdropAction, AmmAction, Bytes, Eip712Domain,
    LaunchpadAction, LendingAction, PerpAction, TokenAction,
};
pub use apply::{apply, Reducer};
