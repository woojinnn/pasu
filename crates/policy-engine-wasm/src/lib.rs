//! WASM bridge for the policy engine.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers:
//! `install_policies_json`, `route_request_json`, `plan_policy_rpc_json`,
//! `evaluate_policy_rpc_json`, and `evaluate_with_envelopes_json`.

mod action_eval_exports;
mod declarative_exports;
mod dto;
mod exports;
mod policy_request_exports;
mod trigger_exports;

/// Part 5 — Curve real-transaction coverage verification harness (test-only).
///
/// Disabled in registry v2 cutover: this harness depends on
/// `registry/manifests/curve/**` which is out of scope until Phase C.
/// Re-enable after Curve manifest migration. The `curve-realtx` feature is
/// declared in `Cargo.toml` but never selected by default; the module body
/// is gated to keep the registry v2 build tree free of Curve dependencies.
#[cfg(all(test, feature = "curve-realtx"))]
mod curve_realtx_tests;

use wasm_bindgen::prelude::*;

/// Module init: forward Rust panics to the JS console.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

mod sim_types;

pub use action_eval_exports::{evaluate_action_v2_json, plan_action_rpc_v2_json};
pub use declarative_exports::{
    declarative_install_v3_json, declarative_route_request_v3_json,
    declarative_route_typed_data_v3_json,
};
pub use exports::{
    evaluate_policy_rpc_json, evaluate_with_envelopes_json, get_alias_table_json,
    install_policies_json, plan_policy_rpc_json, preview_custom_schema_json,
    preview_installed_schema_json, preview_schema_json, route_request_json,
};
pub use policy_request_exports::evaluate_policy_request_json;
pub use trigger_exports::evaluate_triggers_json;
