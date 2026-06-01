//! `policy-transition` applies typed actions to wallet state and returns the
//! predicted state delta.
//!
//! Transition rules are pure: no database, RPC, or clock access. Callers supply
//! the current [`policy_state::WalletState`], an action, and evaluation context;
//! reducers return the updated state plus a typed [`policy_state::StateDelta`].

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

/// Top-level transition entry points.
pub mod apply;
/// Domain-specific action reducers.
pub mod effect;
/// Reducer error type.
pub mod error;
/// Shared reducer helpers.
pub mod helpers;

/// Action type tree re-exported from the sibling `policy-action` crate.
///
/// Keeping this module preserves the existing `policy_transition::action::*`
/// API while making `asset-model/action` the canonical home for action shapes.
pub mod action {
    pub use policy_action::*;
}

pub use action::{
    Action, ActionBody, ActionMeta, ActionNature, AirdropAction, AmmAction, Bytes, Eip712Domain,
    LaunchpadAction, LendingAction, PerpAction, TokenAction,
};
pub use apply::{apply, Reducer};
