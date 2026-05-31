//! WASM bridge for the policy engine.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers. The v2/v3
//! verdict surface lives in `action_eval_exports` (`evaluate_action_v2_json`,
//! `plan_action_rpc_v2_json`), `policy_request_exports`, `trigger_exports`, and
//! the v3 declarative entries (`declarative_install_v3_json`,
//! `declarative_route_request_v3_json`). `install_policies_json` plus the
//! `preview_*` / `get_alias_table_json` schema helpers remain for the dashboard
//! manifest-CRUD surface.

#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::private_intra_doc_links)]
#![allow(rustdoc::redundant_explicit_links)]
#![allow(unknown_lints)]
#![allow(clippy::duration_suboptimal_units)]

mod action_eval_exports;
mod declarative_exports;
mod dto;
mod exports;
mod policy_request_exports;
mod trigger_exports;

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
    get_alias_table_json, install_policies_json, preview_custom_schema_json,
    preview_installed_schema_json, preview_schema_json,
};
pub use policy_request_exports::evaluate_policy_request_json;
pub use trigger_exports::evaluate_triggers_json;
