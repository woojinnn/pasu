//! `simulation-server` — the HTTP backend service for the simulation engine.
//!
//! # Vision
//!
//! The browser extension decodes calldata/signature into an `Action`, reads the
//! policy manifest to decide which enrichment calls are needed (the *planning*
//! step), and POSTs `{wallet, Action(s), eval_context, call-specs}` here. This
//! backend **executes** those calls (via `simulation-sync`) and **simulates**
//! the action(s) over the wallet's state (via `simulation-reducer`, persisted
//! through `simulation-db`), returning the resulting **state / statediff /
//! enriched results**. Cedar policy evaluation stays in the extension (WASM),
//! so this crate has **no** `cedar` / `policy-engine` dependency.
//!
//! # Status
//!
//! This crate provides the service **DTO contract** ([`dto`]) — the
//! request/response shapes the extension and backend agree on, matching +
//! extending the legacy Node.js `scopeball.evaluate_v3` contract — plus the
//! axum [`app`] (router + shared state), the [`handler`] that simulates action
//! envelopes over canonical wallet state (load → reduce → predicted response),
//! and the store boundaries ([`store`], [`db_store`]). Post-policy execution reports are
//! recorded separately from wallet state so wallet/chain/venue callbacks cannot
//! be mistaken for authoritative state. Live-input refresh, enrichment-call
//! execution, and report reconciliation are marked `TODO(prep)` and land in
//! subsequent tasks.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]
// Phase 5 auth + multi-user code: pedantic lints handled at follow-up cleanup.
#![allow(missing_docs)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::result_large_err)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::must_use_candidate)]

pub mod app;
pub mod auth;
pub mod db_store;
pub mod docs;
pub mod dto;
pub mod events;
pub mod handler;
pub mod read_handlers;
pub mod spenders;
pub mod store;
pub mod verdict_handlers;
pub mod write_handlers;
