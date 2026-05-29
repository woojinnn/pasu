//! Launchpad-domain lowering.
//!
//! STUB — the fan-out fills in one leaf module per [`LaunchpadAction`] variant
//! and replaces this dispatch's catch-all. Until then every variant returns
//! `Unsupported` (fail-closed — never a silent pass).

use simulation_reducer::action::launchpad::LaunchpadAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Dispatch a [`LaunchpadAction`] to its per-action lowering.
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for every launchpad action (stub).
pub(crate) fn lower(
    action: &LaunchpadAction,
    _ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    Err(LowerError::Unsupported(format!(
        "launchpad/{}",
        action.action_tag()
    )))
}
