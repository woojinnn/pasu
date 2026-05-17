//! WASM bridge for the policy engine.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers:
//! `install_policies_json`, `route_request_json`, `plan_policy_rpc_json`, and
//! `evaluate_policy_rpc_json`.

mod dto;
mod exports;
mod helpers;

use wasm_bindgen::prelude::*;

/// Module init: forward Rust panics to the JS console.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

pub use exports::{
    evaluate_policy_rpc_json, install_policies_json, plan_policy_rpc_json,
    preview_installed_schema_json, preview_schema_json, route_request_json,
};
pub use helpers::{decode_abi_standard_json, parse_sign_request_json};
