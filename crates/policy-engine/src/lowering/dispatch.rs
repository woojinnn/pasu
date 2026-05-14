//! Dispatch from normalized action envelopes to per-action policy requests.

use crate::action::{Action, ActionEnvelope, Address, DecimalString};
use crate::policy::PolicyRequest;
use serde_json::Value;

use super::common::cedar::entities;

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
        Action::Swap(action) => Some(action.build(&ctx)),
        Action::AddLiquidity(action) => Some(action.build(&ctx)),
        Action::RemoveLiquidity(action) => Some(action.build(&ctx)),
        Action::MintLiquidityNft(action) => Some(action.build(&ctx)),
        Action::BurnLiquidityNft(action) => Some(action.build(&ctx)),
        Action::IncreaseLiquidity(action) => Some(action.build(&ctx)),
        Action::DecreaseLiquidity(action) => Some(action.build(&ctx)),
        _ => None,
    }
}
