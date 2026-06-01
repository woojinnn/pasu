//! `Lending::EnableCollateral` lowering → `Lending::EnableCollateralContext`.

use policy_transition::action::lending::SetCollateralAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_set_collateral_context;

/// Lower a `Lending::EnableCollateral` action into the
/// `Lending::EnableCollateralContext` shape.
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
    Ok(ctx.lowered(r#"Lending::Action::"EnableCollateral""#, context))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use policy_transition::action::lending::{
        LendingAction, SetCollateralAction, SetCollateralLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{live, onchain_meta, reserve_state, usdc, user_state, venue};

    /// A representative enable-collateral on USDC.
    #[test]
    fn enable_collateral_lowering_conforms_to_schema() {
        let action = LendingAction::EnableCollateral(SetCollateralAction {
            venue: venue(),
            asset: usdc(),
            on_behalf_of: None,
            live_inputs: SetCollateralLiveInputs {
                reserve_state: live(reserve_state()),
                user_state_before: live(user_state()),
            },
        });
        let body = ActionBody::Lending(action);
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("enable_collateral", &body, &meta);
    }
}
