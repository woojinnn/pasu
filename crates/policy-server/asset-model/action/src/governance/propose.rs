//! `GovernanceProposeAction` — create a DAO proposal.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, U256};

use super::GovernanceVenue;

/// Create a governance proposal.
///
/// Payload targets are preserved as addresses when the ABI exposes them. The
/// opaque proposal metadata (for Aave V3 this is IPFS-like bytes) is kept as a
/// raw hex string so policies can treat proposal creation as high-risk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct GovernanceProposeAction {
    /// Governance venue.
    pub venue: GovernanceVenue,
    /// Payload target contracts touched by the proposal.
    #[tsify(type = "string[]")]
    pub payload_targets: Vec<Address>,
    /// Number of payloads/actions, retained even when target extraction is not
    /// available from the manifest.
    #[tsify(type = "string")]
    pub payload_count: U256,
    /// Raw proposal metadata bytes/hash.
    #[tsify(type = "string")]
    pub metadata: String,
}
