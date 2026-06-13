//! `policy-server` — the HTTP backend for wallet state and synchronization.
//! # Vision
//! The browser extension decodes calldata/signature into an `Action`, reads the
//! policy manifest to decide which enrichment calls are needed (the *planning*
//! step), and POSTs `{wallet, Action(s), eval_context, call-specs}` here. This
//! backend **executes** those calls (via `policy-sync`) and **simulates**
//! the action(s) over the wallet's state (via `policy-transition`, persisted
//! through `policy-db`), returning the resulting **state / statediff /
//! enriched results**. Cedar policy evaluation stays in the extension (WASM),
//! so this crate has **no** `cedar` / `policy-engine` dependency.
//! # Status
//! This crate provides the service **DTO contract** ([`dto`]) — the
//! request/response shapes the extension and backend agree on, matching +
//! extending the legacy Node.js `dambi.evaluate_v3` contract — plus the
//! axum [`app`] (router + shared state), the [`handler`] that simulates action
//! envelopes over canonical wallet state (load → reduce → predicted response),
//! and the in-memory test store boundary ([`store`]). The server stores
//! primitive wallet state; policies, verdicts, and audit history stay in the
//! browser extension.

#![forbid(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Error/panic doc sections add noise on internal handler fns; public docs live
// on the DTO contract instead.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
// Several long handler functions (dashboard summary, simulate_sequence,
// seed_holdings) exceed the 100-line clippy default; splitting them would
// just create one-shot helpers that obscure the linear request flow.
#![allow(clippy::too_many_lines)]

pub mod app;
pub mod auth;
pub mod config;
pub mod coordination;
pub mod dashboard_handlers;
pub mod docs;
pub mod dto;
pub mod events;
pub mod handler;
pub mod logging;
pub mod market_dto;
pub mod market_handlers;
pub(crate) mod methods;
pub mod read_handlers;
pub mod readiness;
pub mod storage;
pub mod store;
pub mod write_handlers;
