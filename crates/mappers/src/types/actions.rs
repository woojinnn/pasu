//! Per-action field types — one struct per `schema_demo/schema/actions/*.json`.
//!
//! Each struct carries `_kind` via serde tag in the `ActionFields` enum
//! (see `envelope.rs`). Fields not extractable from calldata are `Option`
//! and emitted as `null` for the host to fill (oracle, registry).

use serde::{Deserialize, Serialize};

use super::common::{Address, AmountConstraint, AssetRef, UsdValuation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapMode {
    ExactIn,
    ExactOut,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapAction {
    pub mode: SwapMode,
    pub token_in: AssetRef,
    pub token_out: AssetRef,
    pub amount_in: AmountConstraint,
    pub amount_out: AmountConstraint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_seconds_from_now: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_usd: Option<UsdValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value_out_usd: Option<UsdValuation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value_out_usd: Option<UsdValuation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapAction {
    pub asset_in: AssetRef,
    pub asset_out: AssetRef,
    pub amount: AmountConstraint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnwrapAction {
    pub asset_in: AssetRef,
    pub asset_out: AssetRef,
    pub amount: AmountConstraint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveAction {
    pub token: AssetRef,
    pub spender: Address,
    pub amount: AmountConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLiquidityAction {
    pub assets: Vec<AssetRef>,
    pub amounts: Vec<AmountConstraint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLiquidityAction {
    pub assets: Vec<AssetRef>,
    pub amounts_min: Vec<AmountConstraint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
}
