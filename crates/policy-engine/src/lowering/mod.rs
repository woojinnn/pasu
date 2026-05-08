//! Lowering stages run in the pipeline sequence:
//! 1. one semantic action is built from a transaction,
//! 2. Dex actions are enriched with aggregate host facts,
//! 3. the action is lowered to one Cedar `PolicyRequest`.

pub mod decimal;
pub mod request;
pub mod stamping;

pub(crate) use decimal::add_decimal_strings;
pub use request::{
    request_from_action, request_from_action_with_host, requests_from_action, requests_from_actions,
};
pub use stamping::{
    compute_dex_window_deltas, enrich_dex_action, enrich_dex_action_base, enrich_dex_window_stats,
    enrich_signature_action,
};
