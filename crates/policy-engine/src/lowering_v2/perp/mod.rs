//! Perp-domain lowering.
//!
//! STUB — the fan-out fills in one leaf module per [`PerpAction`] variant and
//! replaces this dispatch's catch-all. Until then every variant returns
//! `Unsupported` (fail-closed — never a silent pass).

use simulation_reducer::action::perp::PerpAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Dispatch a [`PerpAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for every perp action (stub).
pub(crate) fn lower(
    action: &PerpAction,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported(format!(
        "perp/{}",
        action.action_tag()
    )))
}
