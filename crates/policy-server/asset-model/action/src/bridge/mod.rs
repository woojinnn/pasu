//! `BridgeAction` — cross-chain bridge (the source-chain leg the user signs).
//!
//! ScopeBall only ever sees the source-chain `send`/`deposit` the wallet signs;
//! the destination fill is executed by a relayer/sequencer, never the user. So
//! every protectable signal (destination recipient, destination chain, output
//! token/amount) must be carried here, decoded from the source calldata.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod send;

pub use self::send::*;

/// Cross-chain bridge actions (source-chain leg).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BridgeAction {
    /// Outbound bridge: escrow/burn on the source chain, deliver on the destination.
    Send(BridgeSendAction),
}

impl BridgeAction {
    /// The action's `serde` action tag.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Send(_) => "send",
        }
    }

    /// The venue name of the wrapped action.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Send(a) => Some(a.venue.name()),
        }
    }
}

/// Bridge venue identifier (which bridge the user is calling).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum BridgeVenue {
    /// Across Protocol `SpokePool` (intent / relayer-filled liquidity bridge).
    AcrossSpokePool,
}

impl BridgeVenue {
    /// The venue's `serde` name tag.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::AcrossSpokePool => "across_spoke_pool",
        }
    }
}
