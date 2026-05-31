//! `LiquidStaking::Wrap` lowering → `LiquidStaking::WrapContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::liquid_staking::WrapAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_staking_venue;

/// Lower a `LiquidStaking::Wrap` action. No live inputs.
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

    Ok(ctx.lowered(r#"LiquidStaking::Action::"Wrap""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use simulation_reducer::action::liquid_staking::{LiquidStakingAction, WrapAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::U256;

    use super::super::test_support::{lido_venue, onchain_meta};

    #[test]
    fn wrap_conforms() {
        let body = ActionBody::LiquidStaking(LiquidStakingAction::Wrap(WrapAction {
            venue: lido_venue(),
            amount: U256::from(500_000_000_000_000_000u64),
        }));
        super::super::test_support::assert_conforms("wrap", &body, &onchain_meta());
    }
}
