//! Web3 wallet transaction policy engine — runtime crate.
//!
//! This crate hosts the pieces an end-to-end pipeline needs at runtime:
//!
//! - **`action`**: normalized [`ActionEnvelope`] schema types (the new pipeline input).
//! - **`core`**: shared domain types (`Address`, `Token`, `AmountSpec`,
//!   `UsdValuation`, `TransactionRequest`, `SignatureRequest`) consumed by
//!   adapters and Policy RPC integrations.
//! - **`policy`**: `PolicyEngine` (Cedar wrapper) and the `PolicyRequest`
//!   shape that lowering produces and the engine consumes.
//! - **`lowering`**: [`policy_request_from_envelope`] — the bridge from
//!   `ActionEnvelope` to `PolicyRequest`.
//! - **`prelude`**: the curated import surface
//!   (`use policy_engine::prelude::*;`).
//! - **`root`**: top-level [`RootRequest`] envelope describing the transport.
//! - **`schema`**: bundled Cedar schema composition.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
// `unreachable_pub` already catches over-broad visibility; `pub(crate)` in a
// private module is the more honest spelling for crate-internal helpers.
#![allow(clippy::redundant_pub_crate)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

pub mod action;
pub mod cedar_json;
pub mod context_keys;
pub mod core;
pub mod lowering;
pub mod policy;
pub mod policy_rpc;
pub mod prelude;
pub mod root;
pub mod schema;

pub use action::{
    Action, ActionEnvelope, Address as ActionAddress, AmountConstraint, AmountKind, AssetKind,
    AssetRef, AssetRefWithAmountConstraint, Category, DecimalString, Hex, Validity, ValiditySource,
};
pub use core::{Address, AmountSpec, SignatureRequest, Token, TransactionRequest, UsdValuation};
pub use lowering::{policy_request_from_envelope, LoweringError};
pub use policy::{
    MatchedPolicy, PolicyEngine, PolicyEngineBuilder, PolicyError, PolicyRequest,
    PolicyRequestOrigin, Severity, Verdict,
};
pub use root::{ProtocolRef, RequestKind, RootRequest};
pub use schema::PolicySchemaComposer;
