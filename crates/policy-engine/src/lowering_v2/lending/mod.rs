//! Lending-domain lowering.
//!
//! STUB — the fan-out fills in one leaf module per [`LendingAction`] variant
//! and replaces this dispatch's catch-all. Until then every variant returns
//! `Unsupported` (fail-closed — never a silent pass).

use simulation_reducer::action::lending::LendingAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Dispatch a [`LendingAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for every lending action (stub).
pub(crate) fn lower(
    action: &LendingAction,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported(format!(
        "lending/{}",
        action.action_tag()
    )))
}
