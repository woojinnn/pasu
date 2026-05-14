//! Delegate action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AssetRef, DecimalString, Validity};

use super::PowerType;

/// Delegate governance voting power.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateAction {
    /// Governance token whose power is delegated.
    pub token: AssetRef,
    /// Delegate receiving voting power.
    pub delegatee: Address,
    /// Human-readable delegate label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegatee_label: Option<String>,
    /// Current delegate before this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_delegate: Option<Address>,
    /// Voting power affected by this action, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voting_power: Option<DecimalString>,
    /// Power type for split-power governance tokens, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_type: Option<PowerType>,
    /// Signature validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{
        address, assert_json_roundtrip, erc20, validity,
    };
    use serde_json::json;

    #[test]
    fn test_delegate_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<DelegateAction>(json!({
            "token": erc20("GOV"),
            "delegatee": address(0x80)
        }));
    }

    #[test]
    fn test_delegate_action_serde_roundtrip_full() {
        assert_json_roundtrip::<DelegateAction>(json!({
            "token": erc20("GOV"),
            "delegatee": address(0x80),
            "delegateeLabel": "Known Delegate",
            "currentDelegate": address(0x81),
            "votingPower": "1000000",
            "powerType": "proposition",
            "validity": validity("signature-deadline")
        }));
    }
}
