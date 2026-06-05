//! `Staking::Stake` lowering → `Staking::StakeContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::StakeAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::Stake` action (Aave safety-module `stake` /
/// `stakeWithPermit`). No live inputs. `recipient` is omitted ⇒ submitter.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(action: &StakeAction, ctx: &LowerCtx<'_>) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    if let Some(asset) = &action.asset {
        m.insert("asset".into(), lower_token_ref(asset));
    }
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    if let Some(recipient) = &action.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"Stake""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::U256;
    use policy_transition::action::staking::{StakeAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        aave_safety_module_venue, assert_conforms, onchain_meta, other,
    };

    #[test]
    fn stake_to_recipient_conforms() {
        let body = ActionBody::Staking(StakingAction::Stake(StakeAction {
            venue: aave_safety_module_venue(),
            asset: None,
            amount: U256::from(1_000_000_000_000_000_000u64),
            on_behalf_of: None,
            recipient: Some(other()),
        }));
        assert_conforms("stake", &body, &onchain_meta());
    }

    #[test]
    fn stake_with_permit_no_recipient_conforms() {
        let body = ActionBody::Staking(StakingAction::Stake(StakeAction {
            venue: aave_safety_module_venue(),
            asset: None,
            amount: U256::from(5_000_000_000_000_000_000u64),
            on_behalf_of: None,
            recipient: None,
        }));
        assert_conforms("stake", &body, &onchain_meta());
    }
}
