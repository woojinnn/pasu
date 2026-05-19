//! Gauge vote action (Aerodrome / Velodrome / Solidly fork emission gauge weighted vote).

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, DecimalString, Validity};

/// Cast a Solidly-style emission gauge vote (Aerodrome `Voter.vote(tokenId, pools, weights)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GaugeVoteAction {
    /// Voter contract address.
    pub voter: Address,
    /// veNFT (Aerodrome veAERO / Velodrome veVELO) token ID.
    pub token_id: DecimalString,
    /// Gauge pool addresses receiving emission weights. Empty array = reset.
    pub pools: Vec<Address>,
    /// Vote weights (parallel to `pools`).
    pub weights: Vec<DecimalString>,
    /// Subkind: "vote" (default) / "reset" (pools empty) / "poke" (refresh).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GaugeVoteKind>,
    /// Signature validity, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Discriminator for the three Solidly-style emission gauge operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaugeVoteKind {
    /// Cast a weighted emission vote across `pools` (default).
    Vote,
    /// Clear an existing vote (empty `pools` and `weights`).
    Reset,
    /// Recompute an existing vote without changing weights.
    Poke,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::misc::test_support::{address, assert_json_roundtrip, validity};
    use serde_json::json;

    #[test]
    fn test_gauge_vote_serde_roundtrip_minimal() {
        assert_json_roundtrip::<GaugeVoteAction>(json!({
            "voter": address(0x90),
            "tokenId": "1",
            "pools": [address(0x01), address(0x02)],
            "weights": ["50", "50"]
        }));
    }

    #[test]
    fn test_gauge_vote_serde_roundtrip_full() {
        assert_json_roundtrip::<GaugeVoteAction>(json!({
            "voter": address(0x90),
            "tokenId": "42",
            "pools": [address(0x01)],
            "weights": ["100"],
            "kind": "vote",
            "validity": validity("signature-deadline")
        }));
    }

    #[test]
    fn test_gauge_vote_serde_roundtrip_reset() {
        assert_json_roundtrip::<GaugeVoteAction>(json!({
            "voter": address(0x90),
            "tokenId": "1",
            "pools": [],
            "weights": [],
            "kind": "reset"
        }));
    }
}
