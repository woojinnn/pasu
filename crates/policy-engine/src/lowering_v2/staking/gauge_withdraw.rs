//! `Staking::GaugeWithdraw` lowering → `Staking::GaugeWithdrawContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::GaugeWithdrawAction;

use super::super::common::cedar::u256_hex;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::GaugeWithdraw` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GaugeWithdrawAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));

    Ok(ctx.lowered(r#"Staking::Action::"GaugeWithdraw""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{GaugeWithdrawAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, gauge_venue, onchain_meta};

    #[test]
    fn gauge_withdraw_conforms() {
        let body = ActionBody::Staking(StakingAction::GaugeWithdraw(GaugeWithdrawAction {
            venue: gauge_venue(),
            amount: U256::from(5_000_000_000_000_000_000u64),
        }));
        assert_conforms("gauge_withdraw", &body, &onchain_meta());
    }
}
