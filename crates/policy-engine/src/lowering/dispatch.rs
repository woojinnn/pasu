//! Dispatch from normalized action envelopes to per-action policy requests.

use crate::action::{Action, ActionEnvelope, Address, DecimalString};
use crate::policy::PolicyRequest;
use serde_json::Value;

use super::common::cedar::entities;
use super::error::LoweringError;

#[allow(dead_code)]
pub(crate) struct LoweringCtx<'a> {
    pub(crate) from: &'a Address,
    pub(crate) to: &'a Address,
    pub(crate) value_wei: &'a DecimalString,
    pub(crate) chain_id: u64,
    pub(crate) block_timestamp: u64,
}

impl LoweringCtx<'_> {
    /// Assemble the standard `Wallet`/`Action`/`Protocol` triple for an action.
    ///
    /// `action_kind` flows into `Action::"<kind>"`. The `Protocol` resource
    /// uid is the transaction target (`self.to`) so policies can match the
    /// contract being interacted with — e.g.
    /// `resource == Protocol::"0xUniswapV3Router"`.
    pub(crate) fn request(&self, action_kind: &str, context: Value) -> PolicyRequest {
        PolicyRequest::new(
            format!(r#"Wallet::"{}""#, self.from),
            format!(r#"Action::"{action_kind}""#),
            format!(r#"Protocol::"{}""#, self.to),
            entities(self.from, self.to),
            context,
        )
    }
}

/// Per-action contract: build a Cedar `PolicyRequest` from a single action
/// payload plus the lowering context. Implemented once per action variant in
/// the matching `lowering/<category>/<action>.rs` file. The dispatcher in this
/// module matches on [`Action`] and calls [`Lower::build`] on the wrapped
/// payload, which keeps every per-action implementation honest about its
/// signature.
pub(crate) trait Lower {
    fn build(&self, ctx: &LoweringCtx<'_>) -> PolicyRequest;
}

/// Build a Cedar policy request from a normalized action envelope.
///
/// # Errors
///
/// Returns [`LoweringError::UnsupportedAction`] when the action variant has no
/// per-action lowering implementation yet. Callers must convert this into a
/// fails-closed engine error rather than silently letting the transaction
/// continue — that is the whole reason this returns `Result` instead of
/// `Option`.
pub fn policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Result<PolicyRequest, LoweringError> {
    let ctx = LoweringCtx {
        from,
        to,
        value_wei,
        chain_id,
        block_timestamp,
    };

    // Every variant of [`Action`] is listed explicitly — no `_ =>` catch-all —
    // so adding a new variant to the enum forces an `unreachable_patterns` /
    // `non_exhaustive_patterns` compile-time decision on whether to wire a new
    // lowering or to extend the `UnsupportedAction` branch below.
    match &envelope.action {
        Action::Swap(action) => Ok(action.build(&ctx)),
        Action::AddLiquidity(action) => Ok(action.build(&ctx)),
        Action::RemoveLiquidity(action) => Ok(action.build(&ctx)),
        Action::MintLiquidityNft(action) => Ok(action.build(&ctx)),
        Action::BurnLiquidityNft(action) => Ok(action.build(&ctx)),
        Action::IncreaseLiquidity(action) => Ok(action.build(&ctx)),
        Action::DecreaseLiquidity(action) => Ok(action.build(&ctx)),
        Action::Supply(_)
        | Action::Withdraw(_)
        | Action::Borrow(_)
        | Action::Repay(_)
        | Action::Liquidate(_)
        | Action::FlashLoan(_)
        | Action::SetAuthorization(_)
        | Action::SignAuthorization(_)
        | Action::Revoke(_)
        | Action::Wrap(_)
        | Action::Unwrap(_)
        | Action::Approve(_)
        | Action::SetApprovalForAll(_)
        | Action::Transfer(_)
        | Action::Permit(_)
        | Action::ClaimRewards(_)
        | Action::SignMessage(_)
        | Action::Delegate(_)
        | Action::Vote(_)
        | Action::Stake(_)
        | Action::RequestUnstake(_)
        | Action::ClaimUnstake(_)
        | Action::Restake(_)
        | Action::RequestRestakeWithdrawal(_)
        | Action::ClaimRestakeWithdrawal(_) => Err(LoweringError::UnsupportedAction {
            kind: envelope.action.kind().to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{policy_request_from_envelope, LoweringError};
    use crate::action::misc::ClaimRewardsAction;
    use crate::action::{Action, ActionEnvelope, Address, Category, DecimalString};
    use std::str::FromStr as _;

    #[test]
    fn unsupported_action_returns_unsupported_action_error() {
        let envelope = ActionEnvelope {
            category: Category::Misc,
            action: Action::ClaimRewards(ClaimRewardsAction {
                source: None,
                nft: None,
                token_id: None,
                from: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
                recipient: Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
                reward_tokens: None,
                max_amounts: None,
            }),
        };

        let result = policy_request_from_envelope(
            &envelope,
            &Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
            &Address::from_str("0x2222222222222222222222222222222222222222").unwrap(),
            &DecimalString::from_str("0").unwrap(),
            1,
            1_700_000_000,
        );

        assert_eq!(
            result.unwrap_err(),
            LoweringError::UnsupportedAction {
                kind: "claim_rewards".to_owned(),
            }
        );
    }
}
