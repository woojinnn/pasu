//! LP token unstaking (Aerodrome Gauge.withdraw / mirror of LpStake).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef};

/// Unstake an LP token from a gauge (Aerodrome Gauge.withdraw).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LpUnstakeAction {
    /// Gauge / staking contract address.
    pub gauge: Address,
    /// LP token being unstaked.
    pub lp_token: AssetRef,
    /// Unstake amount.
    pub amount: AmountConstraint,
    /// Recipient of the unstaked LP tokens (default = tx sender).
    pub recipient: Address,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, amount, assert_json_roundtrip, erc20};
    use serde_json::json;

    #[test]
    fn test_lp_unstake_serde_roundtrip() {
        assert_json_roundtrip::<LpUnstakeAction>(json!({
            "gauge": address(0x90),
            "lpToken": erc20("LP"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }));
    }
}
