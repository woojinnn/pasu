//! Dispatch from normalized action envelopes to per-action policy requests.

use crate::action::{Action, ActionEnvelope, Address, DecimalString};
use crate::policy::PolicyRequest;
use serde_json::Value;

use super::common::cedar::entities;
use super::LoweringError;

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
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError>;
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
    try_policy_request_from_envelope(envelope, from, to, value_wei, chain_id, block_timestamp)
        .ok()
        .flatten()
}

/// Build a Cedar policy request from a normalized action envelope, preserving
/// lowering failures for supported action categories.
#[allow(clippy::missing_errors_doc)]
pub fn try_policy_request_from_envelope(
    envelope: &ActionEnvelope,
    from: &Address,
    to: &Address,
    value_wei: &DecimalString,
    chain_id: u64,
    block_timestamp: u64,
) -> Result<Option<PolicyRequest>, LoweringError> {
    let ctx = LoweringCtx {
        from,
        to,
        value_wei,
        chain_id,
        block_timestamp,
    };

    match &envelope.action {
        Action::Swap(action) => action.build(&ctx).map(Some),
        Action::AddLiquidity(action) => action.build(&ctx).map(Some),
        Action::RemoveLiquidity(action) => action.build(&ctx).map(Some),
        Action::MintLiquidityNft(action) => action.build(&ctx).map(Some),
        Action::BurnLiquidityNft(action) => action.build(&ctx).map(Some),
        Action::IncreaseLiquidity(action) => action.build(&ctx).map(Some),
        Action::DecreaseLiquidity(action) => action.build(&ctx).map(Some),
        Action::Donate(action) => action.build(&ctx).map(Some),
        Action::InitializePool(action) => action.build(&ctx).map(Some),
        // misc — added so PERMIT2_PERMIT / TRANSFER / WRAP_ETH / UNWRAP_WETH
        // envelopes lower into PolicyRequest.
        Action::Permit(action) => action.build(&ctx).map(Some),
        Action::Transfer(action) => action.build(&ctx).map(Some),
        Action::Wrap(action) => action.build(&ctx).map(Some),
        Action::Unwrap(action) => action.build(&ctx).map(Some),
        // misc — Phase 7B added the `approve` + `set_approval_for_all`
        // single_emit builders (Permit2 `approve`, ERC-721/NFPM
        // `setApprovalForAll`). Without these arms a forbid policy on
        // `Action::"approve"` / `Action::"set_approval_for_all"` would never
        // see the matching PolicyRequest and would silently aggregate to
        // `Pass` — the same fail-open class as Curve Phase 12.7 P0-1 and
        // Aerodrome Round 7 P0 #1. The static `protocols/erc20` mappers
        // already emit these variants, so the gap also closed an existing
        // ERC-20 `approve` / `setApprovalForAll` silent-pass on the static
        // path.
        Action::Approve(action) => action.build(&ctx).map(Some),
        Action::SetApprovalForAll(action) => action.build(&ctx).map(Some),
        // lending — Phase 12.5 added crvUSD Controller bundles whose action
        // envelopes (Borrow / Repay / Liquidate) silently fell through to
        // `Ok(None)` before this arm landed, producing an empty verdict list
        // and aggregating to `Pass` (the silent fail-open Phase 12.7 audit
        // P0-1). Phase B / F1 added `Supply` for crvUSD Controller
        // `addCollateral` / `addCollateral-for` — without this arm the 6
        // `crvusd/{wsteth,sfrxeth,wbtc}/addCollateral{,-for}@1.0.0` manifests
        // silently aggregated to `Pass` (same fail-open class).
        Action::Borrow(action) => action.build(&ctx).map(Some),
        Action::Repay(action) => action.build(&ctx).map(Some),
        Action::Liquidate(action) => action.build(&ctx).map(Some),
        Action::Supply(action) => action.build(&ctx).map(Some),
        // staking — Phase 12.6 added veCRV / Gauge bundles whose Stake /
        // ClaimUnstake envelopes also silently fell through. Same fail-open
        // class as lending above.
        Action::Stake(action) => action.build(&ctx).map(Some),
        Action::ClaimUnstake(action) => action.build(&ctx).map(Some),
        // misc — governance + reward claims (GaugeController / Gauge
        // claim_rewards). Without lowering, a forbid policy on
        // `Action::"vote"` or `Action::"claim_rewards"` would never see the
        // matching PolicyRequest and would silently aggregate to `Pass`.
        Action::Vote(action) => action.build(&ctx).map(Some),
        // misc — Aerodrome ve(3,3) variants (gauge vote / LP stake / locks).
        // Phase 8 added 6 new Action variants whose missing dispatch arm
        // would silently aggregate to `Pass` (Phase 8 Round 7 P0 #1 equiv).
        Action::GaugeVote(action) => action.build(&ctx).map(Some),
        Action::LpStake(action) => action.build(&ctx).map(Some),
        Action::LpUnstake(action) => action.build(&ctx).map(Some),
        Action::LockCreate(action) => action.build(&ctx).map(Some),
        Action::LockIncrease(action) => action.build(&ctx).map(Some),
        Action::LockManage(action) => action.build(&ctx).map(Some),
        Action::LockWithdraw(action) => action.build(&ctx).map(Some),
        // misc — reward claims (Curve Gauge claim_rewards + Aerodrome Voter
        // claimFees/Bribes + Slipstream NPM collect). Without this arm
        // ClaimRewards envelopes lower to Ok(None), which fail-opens past
        // the dispatcher (Curve Phase 12.7 P0-1 + Aerodrome Round 7 P0 #1).
        Action::ClaimRewards(action) => action.build(&ctx).map(Some),
        _ => Ok(None),
    }
}
