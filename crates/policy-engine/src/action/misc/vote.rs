//! Vote action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, DecimalString, Validity};

use super::VoteSupport;

/// Cast a governance vote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteAction {
    /// Governor contract.
    pub governance: Address,
    /// Human-readable governance label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governance_label: Option<String>,
    /// Proposal identifier.
    pub proposal_id: DecimalString,
    /// Vote direction.
    pub support: VoteSupport,
    /// Free-form vote reason, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Voting power applied to this vote, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voting_power: Option<DecimalString>,
    /// Signature validity window, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, assert_json_roundtrip, validity};
    use serde_json::json;

    #[test]
    fn test_vote_action_serde_roundtrip_minimal() {
        assert_json_roundtrip::<VoteAction>(json!({
            "governance": address(0x90),
            "proposalId": "1",
            "support": "for"
        }));
    }

    #[test]
    fn test_vote_action_serde_roundtrip_full() {
        assert_json_roundtrip::<VoteAction>(json!({
            "governance": address(0x90),
            "governanceLabel": "Example Governor",
            "proposalId": "1",
            "support": "abstain",
            "reason": "reason",
            "votingPower": "1000000",
            "validity": validity("signature-deadline")
        }));
    }
}
