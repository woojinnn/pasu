//! Lowering stages: `ActionEnvelope` -> `PolicyRequest`.

pub use dispatch::policy_request_from_envelope;

mod common;
pub mod decimal;
mod dex;
mod dispatch;

pub(crate) use decimal::add_decimal_strings;
