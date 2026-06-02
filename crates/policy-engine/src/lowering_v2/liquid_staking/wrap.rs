//! `LiquidStaking::Wrap` lowering → `LiquidStaking::WrapContext`.

use serde_json::{Map, Value};

use policy_transition::action::liquid_staking::WrapAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::Wrap` action.
///
/// `expectedWsteth` is the host-populated live field — the wstETH the wrap mints
/// (`getWstETHByStETH(amount)`), shown so the user sees the concrete output.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(action: &WrapAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_staking_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    m.insert(
        "expectedWsteth".into(),
        Value::String(u256_hex(action.live_inputs.expected_wsteth.value)),
    );

    Ok(ctx.lowered(r#"LiquidStaking::Action::"Wrap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::liquid_staking::{
        LiquidStakingAction, WrapAction, WrapLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{lido_venue, live_u256, onchain_meta};

    #[test]
    fn wrap_conforms() {
        let body = ActionBody::LiquidStaking(LiquidStakingAction::Wrap(WrapAction {
            venue: lido_venue(),
            amount: U256::from(500_000_000_000_000_000u64),
            live_inputs: WrapLiveInputs {
                expected_wsteth: live_u256(),
            },
        }));
        super::super::test_support::assert_conforms("wrap", &body, &onchain_meta());
    }
}
