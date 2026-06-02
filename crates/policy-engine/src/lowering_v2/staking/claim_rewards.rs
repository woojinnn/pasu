//! `Staking::ClaimRewards` lowering → `Staking::ClaimRewardsContext`.

use serde_json::{Map, Value};

use policy_transition::action::staking::ClaimRewardsAction;

use super::super::common::cedar::addr;
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_stake_venue;

/// Lower a `Staking::ClaimRewards` action. No live inputs. `gauges` is a
/// `Set<String>` of gauge addresses (one for `mint`/`mint_for`, up to 8 for
/// `mint_many`).
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action `lower`
/// contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &ClaimRewardsAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_stake_venue(&action.venue));
    if let Some(reward_token) = &action.reward_token {
        m.insert("rewardToken".into(), lower_token_ref(reward_token));
    }
    m.insert(
        "gauges".into(),
        Value::Array(
            action
                .gauges
                .iter()
                .map(|g| Value::String(addr(g)))
                .collect(),
        ),
    );
    if let Some(on_behalf_of) = &action.on_behalf_of {
        m.insert("onBehalfOf".into(), Value::String(addr(on_behalf_of)));
    }
    if let Some(recipient) = &action.recipient {
        m.insert("recipient".into(), Value::String(addr(recipient)));
    }

    Ok(ctx.lowered(r#"Staking::Action::"ClaimRewards""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::Address;
    use policy_transition::action::staking::{ClaimRewardsAction, StakingAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, crv, gauge_venue, minter_venue, onchain_meta, other,
    };

    fn gauge() -> Address {
        Address::from_str("0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a").unwrap()
    }

    #[test]
    fn mint_single_gauge_conforms() {
        let body = ActionBody::Staking(StakingAction::ClaimRewards(ClaimRewardsAction {
            venue: minter_venue(),
            reward_token: Some(crv()),
            gauges: vec![gauge()],
            on_behalf_of: None,
            recipient: None,
        }));
        assert_conforms("claim_rewards", &body, &onchain_meta());
    }

    #[test]
    fn mint_for_many_gauges_conforms() {
        let body = ActionBody::Staking(StakingAction::ClaimRewards(ClaimRewardsAction {
            venue: minter_venue(),
            reward_token: Some(crv()),
            gauges: vec![gauge(), gauge()],
            on_behalf_of: Some(other()),
            recipient: None,
        }));
        assert_conforms("claim_rewards", &body, &onchain_meta());
    }

    #[test]
    fn gauge_claim_rewards_to_recipient_conforms() {
        // A gauge's own claim_rewards: no reward_token (multi-reward set), no
        // gauges (the gauge IS the venue), explicit recipient.
        let body = ActionBody::Staking(StakingAction::ClaimRewards(ClaimRewardsAction {
            venue: gauge_venue(),
            reward_token: None,
            gauges: vec![],
            on_behalf_of: Some(other()),
            recipient: Some(gauge()),
        }));
        assert_conforms("claim_rewards", &body, &onchain_meta());
    }
}
