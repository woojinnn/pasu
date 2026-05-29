//! Airdrop-domain lowering.
//!
//! STUB — the fan-out fills in one leaf module per [`AirdropAction`] variant
//! and replaces this dispatch's catch-all. Until then every variant returns
//! `Unsupported` (fail-closed — never a silent pass).

use simulation_reducer::action::airdrop::AirdropAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Dispatch an [`AirdropAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for every airdrop action (stub).
pub(crate) fn lower(
    action: &AirdropAction,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported(format!(
        "airdrop/{}",
        action.action_tag()
    )))
}
