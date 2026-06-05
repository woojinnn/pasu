//! `SupplyAction` — supply (`deposit`) an asset into a lending market.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Decimal, Price, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use super::{LendingVenue, ReserveState, UserLendingState};

/// Supply (`deposit`) an asset into a lending market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SupplyAction {
    /// Lending venue (e.g. `AaveV3` on Optimism).
    pub venue: LendingVenue,
    /// Asset being supplied.
    pub asset: TokenRef,
    /// Amount to supply (asset units).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Beneficiary; defaults to `submitter` when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
    /// Live inputs fetched at simulation time.
    pub live_inputs: SupplyLiveInputs,
}

/// Live-fetched inputs for a `SupplyAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct SupplyLiveInputs {
    /// Reserve state at simulation time.
    pub reserve_state: LiveField<ReserveState>,
    /// Current supply APY for the asset.
    pub supply_apy: LiveField<Decimal>,
    /// `aToken` price in USD.
    pub a_token_price_usd: LiveField<Price>,
    /// Whether the supplied asset can be used as collateral.
    pub eligible_as_collat: LiveField<bool>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
}
