//! `Staking::VoteForGauge` lowering → `Staking::VoteForGaugeContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::staking::VoteForGaugeAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::VoteForGauge` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &VoteForGaugeAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("gauge".into(), Value::String(addr(&action.gauge)));
    m.insert("weightBp".into(), Value::String(u256_hex(action.weight_bp)));

    Ok(ctx.lowered(r#"Staking::Action::"VoteForGauge""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::staking::{StakingAction, VoteForGaugeAction};
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Address, U256};

    use super::super::test_support::{assert_conforms, gauge_controller_venue, onchain_meta};

    #[test]
    fn vote_for_gauge_conforms() {
        let body = ActionBody::Staking(StakingAction::VoteForGauge(VoteForGaugeAction {
            venue: gauge_controller_venue(),
            gauge: Address::from_str("0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a").unwrap(),
            weight_bp: U256::from(10_000u64),
        }));
        assert_conforms("vote_for_gauge", &body, &onchain_meta());
    }
}
