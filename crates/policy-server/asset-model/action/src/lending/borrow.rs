//! `BorrowAction` — borrow an asset against existing collateral.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Decimal, Price, U256};
use policy_state::token::{RateMode, TokenRef};
use policy_state::LiveField;

use super::{LendingVenue, ReserveState, UserLendingState};

/// Borrow an asset against existing collateral.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BorrowAction {
    /// Lending venue.
    pub venue: LendingVenue,
    /// Asset being borrowed.
    pub asset: TokenRef,
    /// Amount to borrow (asset units).
    #[tsify(type = "string")]
    pub amount: U256,
    /// Borrow rate mode (`Variable` or `Stable`).
    pub rate_mode: RateMode,
    /// Borrower of record; defaults to `submitter` when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub on_behalf_of: Option<Address>,
    /// Live inputs fetched at simulation time.
    pub live_inputs: BorrowLiveInputs,
}

/// Live-fetched inputs for a `BorrowAction`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct BorrowLiveInputs {
    /// Reserve state at simulation time.
    pub reserve_state: LiveField<ReserveState>,
    /// User account state before the action.
    pub user_state_before: LiveField<UserLendingState>,
    /// Borrow asset price in USD.
    pub asset_price_usd: LiveField<Price>,
    /// Current borrow rate for the chosen `RateMode`.
    pub current_borrow_rate: LiveField<Decimal>,
    /// Liquidity available in the reserve for borrowing.
    #[tsify(type = "LiveField<string>")]
    pub available_liquidity: LiveField<U256>,
}
