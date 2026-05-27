//! Per-action lowering for misc actions (permit / transfer / wrap / unwrap).
//!
//! Mirrors the structure of [`crate::lowering::dex`]: one submodule per
//! action with an `impl Lower for <Action>`. The dispatcher in
//! [`crate::lowering::dispatch`] calls `action.build(&ctx)` for each
//! supported variant.

use crate::action::AssetRefWithAmountConstraint;
use crate::lowering::common::asset::asset_ref_with_amount_json;
use crate::lowering::LoweringError;
use serde_json::Value;

pub(crate) mod approve;
pub(crate) mod claim_rewards;
pub(crate) mod gauge_vote;
pub(crate) mod lock_create;
pub(crate) mod lock_increase;
pub(crate) mod lock_manage;
pub(crate) mod lock_withdraw;
pub(crate) mod lp_stake;
pub(crate) mod lp_unstake;
pub(crate) mod permit;
pub(crate) mod set_approval_for_all;
pub(crate) mod transfer;
pub(crate) mod unwrap;
pub(crate) mod vote;
pub(crate) mod wrap;

pub(crate) fn asset_with_amount_json(
    asset_with_amount: &AssetRefWithAmountConstraint,
) -> Result<Value, LoweringError> {
    asset_ref_with_amount_json(&asset_with_amount.asset, &asset_with_amount.amount)
}
