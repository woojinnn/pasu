//! `Staking::GaugeDeposit` lowering → `Staking::GaugeDepositContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::GaugeDepositAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::GaugeDeposit` action. No live inputs.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &GaugeDepositAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"GaugeDeposit""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{GaugeDepositAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{assert_conforms, gauge_venue, onchain_meta, other};

    fn body(on_behalf: bool) -> ActionBody {
        ActionBody::Staking(StakingAction::GaugeDeposit(GaugeDepositAction {
            venue: gauge_venue(),
            amount: U256::from(1_000_000_000_000_000_000u64),
            on_behalf_of: if on_behalf { Some(other()) } else { None },
        }))
    }

    #[test]
    fn gauge_deposit_self_conforms() {
        assert_conforms("gauge_deposit", &body(false), &onchain_meta());
    }

    #[test]
    fn gauge_deposit_for_conforms() {
        assert_conforms("gauge_deposit", &body(true), &onchain_meta());
    }
}
