//! `MarketplaceVenue` — NFT-marketplace settlement venue identifier.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, ChainId};

/// NFT-marketplace settlement venue. Currently only Seaport; the `name`
/// discriminator keeps the domain reusable for Blur / LooksRare / X2Y2.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum MarketplaceVenue {
    /// OpenSea Seaport settlement engine.
    Seaport {
        /// Chain the settlement contract lives on.
        chain: ChainId,
        /// Seaport core settlement contract address (the EIP-712 verifyingContract
        /// for signed orders + the fulfill/match/cancel target).
        #[tsify(type = "string")]
        settlement: Address,
    },
}

impl MarketplaceVenue {
    /// The venue's serde `name` tag (e.g. `"seaport"`).
    ///
    /// Matches the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Seaport { .. } => "seaport",
        }
    }
}
