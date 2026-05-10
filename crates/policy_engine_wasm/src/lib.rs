//! WASM bridge for the policy engine.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers:
//! `install_policies_json`, `build_action_json`, `tier1_fact_plan_json`,
//! `tier2_window_keys_json`, and `evaluate_json`.

mod dto;
mod exports;
mod state;

use wasm_bindgen::prelude::*;

/// Module init: forward Rust panics to the JS console.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

pub use exports::{
    build_action_json, evaluate_json, install_policies_json, tier1_fact_plan_json,
    tier2_window_keys_json,
};
