//! `LiquidStaking::Unwrap` lowering → `LiquidStaking::UnwrapContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::UnwrapAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::Unwrap` action.
///
/// `expectedSteth` is the host-populated live field — the stETH the unwrap
/// returns (`getStETHByWstETH(amount)`), shown so the user sees the output.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &UnwrapAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    m.insert(
        "expectedSteth".into(),
        Value::String(u256_hex(action.live_inputs.expected_steth.value)),
    );

    Ok(ctx.lowered(r#"LiquidStaking::Action::"Unwrap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{
        LiquidStakingAction, UnwrapAction, UnwrapLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, live_u256, onchain_meta};

    #[test]
    fn unwrap_conforms() {
        let body = ActionBody::LiquidStaking(LiquidStakingAction::Unwrap(UnwrapAction {
            venue: lido_venue(),
            amount: U256::from(250_000_000_000_000_000u64),
            live_inputs: UnwrapLiveInputs {
                expected_steth: live_u256(),
            },
        }));
        super::super::test_support::assert_conforms("unwrap", &body, &onchain_meta());
    }
}
