//! `AdjustMarginAction` — add or withdraw collateral from an existing position.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::{PerpSide, PositionId};
use policy_state::primitives::{MarketRef, SignedI256, U256};
use policy_state::LiveField;

use super::{PerpPositionLive, PerpVenue};

/// Add or withdraw collateral from a position. The position is referenced
/// either by `position_id` (on-chain venues) OR by `(market, side)` (off-chain
/// orderbook venues like Hyperliquid, whose `updateIsolatedMargin` names the
/// market + side, not a position id) — at least one form is present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AdjustMarginAction {
    /// Perpetual venue hosting the position.
    pub venue: PerpVenue,
    /// Identifier of the position being adjusted. `None` when the venue
    /// references the position by `(market, side)` instead (Hyperliquid).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub position_id: Option<PositionId>,
    /// Market the adjustment targets — used by venues that reference the
    /// position by market (Hyperliquid). `None` when `position_id` is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub market: Option<MarketRef>,
    /// Position side — used together with `market` (Hyperliquid). `None`
    /// on-chain (the `position_id` identifies the side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub side: Option<PerpSide>,
    /// Positive = deposit, negative = withdraw.
    #[tsify(type = "string")]
    pub delta: SignedI256,
    /// Live position / margin inputs. `None` for Hyperliquid pre-sign (the
    /// `updateIsolatedMargin` intent carries no live state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub live_inputs: Option<AdjustMarginLiveInputs>,
}

/// Live inputs read at execution time for `AdjustMarginAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct AdjustMarginLiveInputs {
    /// Current `PerpPositionLive` state.
    pub position_state: LiveField<PerpPositionLive>,
    /// Free margin remaining after the adjustment is applied.
    #[tsify(type = "LiveField<string>")]
    pub free_margin_after: LiveField<U256>,
}
