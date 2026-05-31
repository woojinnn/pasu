//! New-model Cedar lowering — `simulation_reducer::action::ActionBody` →
//! [`LoweredAction`] (`Wallet` / `<Namespace>::Action::"…"` / `Protocol` +
//! cedarschema action-context JSON).
//!
//! This is the ADDITIVE counterpart to the legacy [`crate::lowering`] pipeline
//! (which consumes the old `ActionEnvelope`). It targets the new action model
//! directly and produces a context object that conforms to the per-action
//! cedarschema types under `schema/policy-schema/actions/`. The two pipelines
//! run side by side; this module never touches the legacy one.
//!
//! # Layout (mirrors [`crate::lowering`])
//!
//! - [`dispatch`] — the `LoweredAction` / `TxMeta` / `LowerError` contract,
//!   the `LowerCtx`, and `lower_action`, which matches an `ActionBody` on its
//!   **domain** and delegates to that domain's `lower`.
//! - [`common`] — shared sub-lowerings (Cedar primitives, token refs/keys,
//!   action meta / nature / EIP-712 domain).
//! - one module per **domain** (`amm`, `token`, `lending`, `airdrop`,
//!   `launchpad`, `perp`) + the two struct variants (`multicall`, `unknown`).
//!   Each domain owns its directory and per-action leaf modules.
//!
//! # Conventions
//!
//! - `principal` = `Wallet::"<tx.from>"`, `resource` = `Protocol::"<tx.to>"`.
//! - `action_uid` is namespaced + `PascalCase`, e.g. `Amm::Action::"Swap"`.
//!
//! # JSON shape rules
//!
//! Cedar 4.10 has no enums/unions: Rust enums are modelled as discriminated
//! records (`{ kind | name | standard: String, …optional }`) and `LiveField<T>`
//! is inlined as its underlying `T`. The cedarschema uses **camelCase** keys, so
//! every record is hand-built (a blind `serde_json::to_value` of the Rust struct
//! would emit `snake_case` keys and whole `LiveField` objects). Optional fields
//! are **omitted** when absent — never emitted as `null`. `Long` fields are
//! plain JSON numbers; `U256`/`U128` values are lower-hex strings (`{:#x}`).

pub use dispatch::{lower_action, LowerError, LoweredAction, TxMeta};

mod airdrop;
mod amm;
mod common;
mod dispatch;
mod hyperliquid_core;
mod launchpad;
mod lending;
mod multicall;
mod perp;
mod token;
mod unknown;
