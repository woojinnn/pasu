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
//! This crate currently provides the service **DTO contract** ([`dto`]) — the
//! request/response shapes the extension and backend agree on, matching +
//! extending the legacy Node.js `scopeball.evaluate_v3` contract. The axum
//! server, the wallet-store boundary, and the orchestration handler land in
//! subsequent tasks.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

pub mod dto;
