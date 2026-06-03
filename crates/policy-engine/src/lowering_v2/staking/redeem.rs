//! `Staking::Redeem` lowering → `Staking::RedeemContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::RedeemAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Redeem` action (Aave safety-module `redeem(to, amount)`).
/// No live inputs. `recipient` is omitted ⇒ submitter.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &RedeemAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(recipient) = &action.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"Redeem""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{RedeemAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        aave_safety_module_venue, assert_conforms, onchain_meta, other,
    };

    #[test]
    fn redeem_to_recipient_conforms() {
        let body = ActionBody::Staking(StakingAction::Redeem(RedeemAction {
            venue: aave_safety_module_venue(),
            amount: U256::from(2_000_000_000_000_000_000u64),
            recipient: Some(other()),
        }));
        assert_conforms("redeem", &body, &onchain_meta());
    }
}
