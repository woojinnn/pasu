//! `BuyCollateralAction` — buy discounted collateral from a lending protocol's
//! reserves using its base asset.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, U256};
use simulation_state::token::TokenRef;

use super::LendingVenue;

/// Buy collateral that a lending protocol is selling from reserves.
///
/// Models Compound V3 Comet `buyCollateral(asset,minAmount,baseAmount,recipient)`.
/// It is not a generic AMM swap: the venue is the lending market itself, the
/// input asset is the Comet base asset, and the quoted output comes from
/// protocol reserves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BuyCollateralAction {
    /// Lending venue selling collateral.
    pub venue: LendingVenue,
    /// Collateral asset being bought from reserves.
    pub collateral_asset: TokenRef,
    /// Base asset paid into the protocol.
    pub base_asset: TokenRef,
    /// Minimum acceptable collateral amount.
    #[tsify(type = "string")]
    pub min_collateral_amount: U256,
    /// Amount of base asset the user pays.
    #[tsify(type = "string")]
    pub base_amount: U256,
    /// Recipient of the bought collateral.
    #[tsify(type = "string")]
    pub recipient: Address,
}
