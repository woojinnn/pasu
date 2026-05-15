//! Lowering stages: `ActionEnvelope` -> `PolicyRequest`.

pub use dispatch::policy_request_from_envelope;
pub use error::LoweringError;

mod common;
mod dex;
mod dispatch;
mod error;
mod lending;
mod restaking;
mod staking;
