//! Dispatch from normalized action envelopes to per-action policy requests.

use crate::action::{Action, ActionEnvelope, Address, DecimalString};
use crate::policy::PolicyRequest;

#[allow(dead_code)]
pub(crate) struct LoweringCtx<'a> {
    pub(crate) from: &'a Address,
    pub(crate) to: &'a Address,
    pub(crate) value_wei: &'a DecimalString,
    pub(crate) chain_id: u64,
    pub(crate) block_timestamp: u64,
}

/// Build a Cedar policy request from a normalized action envelope.
#[must_use]
pub fn policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Option<PolicyRequest> {
    let ctx = LoweringCtx {
        from,
        to,
        value_wei,
        chain_id,
        block_timestamp,
    };

    match &envelope.action {
        Action::Swap(action) => Some(super::actions::swap::build(action, &ctx)),
        Action::AddLiquidity(action) => Some(super::actions::add_liquidity::build(action, &ctx)),
        Action::RemoveLiquidity(action) => {
            Some(super::actions::remove_liquidity::build(action, &ctx))
        }
        Action::MintLiquidityNft(action) => {
            Some(super::actions::mint_liquidity_nft::build(action, &ctx))
        }
        Action::BurnLiquidityNft(action) => {
            Some(super::actions::burn_liquidity_nft::build(action, &ctx))
        }
        Action::IncreaseLiquidity(action) => {
            Some(super::actions::increase_liquidity::build(action, &ctx))
        }
        Action::DecreaseLiquidity(action) => {
            Some(super::actions::decrease_liquidity::build(action, &ctx))
        }
        _ => None,
    }
}
