//! Add-liquidity action for fungible LP pools.

use serde::{Deserialize, Serialize};

use crate::action::common::{
    Address, AmountConstraint, AssetRef, AssetRefWithAmountConstraint, Validity,
};

use super::PoolRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Add liquidity to a fungible LP pool.
pub struct AddLiquidityAction {
    /// Target pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolRef>,
    /// Assets deposited into the pool with amount constraints.
    pub inputs: Vec<AssetRefWithAmountConstraint>,
    /// LP token minted by the pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_token: Option<AssetRef>,
    /// LP amount constraint, when represented by the protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_amount: Option<AmountConstraint>,
    /// Recipient of the LP token.
    pub recipient: Address,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        address, amount, assert_roundtrip, asset_amount_pair, erc20, pool, validity,
    };

    #[test]
    fn test_add_liquidity_action_serde_roundtrip_minimal() {
        let action = AddLiquidityAction {
            pool: None,
            inputs: asset_amount_pair(AmountKind::Exact, AmountKind::Exact),
            lp_token: None,
            lp_amount: None,
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_add_liquidity_action_serde_roundtrip_full() {
        let action = AddLiquidityAction {
            pool: Some(pool()),
            inputs: asset_amount_pair(AmountKind::Min, AmountKind::Min),
            lp_token: Some(erc20(
                "0x3333333333333333333333333333333333333333",
                "UNI-V2",
                18,
            )),
            lp_amount: Some(amount(AmountKind::Min, "100000000000000000")),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }
}
