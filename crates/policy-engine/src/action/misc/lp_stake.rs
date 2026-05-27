//! LP token staking (Aerodrome Gauge.deposit / Curve Gauge.deposit / Convex stake).
//! Distinct from `StakeAction` (liquid staking) — no receipt token issued.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRefWithAmountConstraint};

/// Stake an LP token in a gauge / staking contract (Aerodrome Gauge.deposit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LpStakeAction {
    /// Gauge / staking contract address.
    pub gauge: Address,
    /// LP token being staked with its stake amount.
    pub lp_token: AssetRefWithAmountConstraint,
    /// Recipient of stake credit (default = tx sender).
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, amount, assert_json_roundtrip, erc20};
    use serde_json::json;

    #[test]
    fn test_lp_stake_serde_roundtrip() {
        assert_json_roundtrip::<LpStakeAction>(json!({
            "gauge": address(0x90),
            "lpToken": {
                "asset": erc20("LP"),
                "amount": amount("exact", "1000")
            },
            "recipient": address(0x30)
        }));
    }
}
