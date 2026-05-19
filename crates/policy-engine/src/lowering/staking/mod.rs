//! Per-action lowering for staking actions.
//!
//! Each submodule provides an `impl Lower for <Action>` so the dispatcher in
//! [`crate::lowering::dispatch`] can call `action.build(&ctx)` uniformly.

pub(crate) mod claim_unstake;
pub(crate) mod stake;
