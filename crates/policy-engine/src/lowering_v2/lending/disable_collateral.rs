//! `Lending::DisableCollateral` lowering → `Lending::DisableCollateralContext`.

use simulation_reducer::action::lending::SetCollateralAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_set_collateral_context;

/// Lower a `Lending::DisableCollateral` action into the
/// `Lending::DisableCollateralContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SetCollateralAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let context = lower_set_collateral_context(action, ctx);
    Ok(ctx.lowered(r#"Lending::Action::"DisableCollateral""#, context))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use simulation_reducer::action::lending::{
        LendingAction, SetCollateralAction, SetCollateralLiveInputs,
    };
    use simulation_reducer::action::ActionBody;

    use super::super::test_support::{live, onchain_meta, reserve_state, user_state, venue, weth};

    /// A representative disable-collateral on WETH.
    #[test]
    fn disable_collateral_lowering_conforms_to_schema() {
        let action = LendingAction::DisableCollateral(SetCollateralAction {
            venue: venue(),
            asset: weth(),
            on_behalf_of: None,
            live_inputs: SetCollateralLiveInputs {
                reserve_state: live(reserve_state()),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("disable_collateral", &body, &meta);
    }
}
