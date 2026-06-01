//! `LendingAccount` — the *account-level aggregate* for a single lending market.
//!
//! The per-asset supply/borrow ERC20 balances (e.g. aUSDC, variableDebtUSDC)
//! live in `tokens`; this struct only holds the aggregate metadata layered on
//! top of them (HF, LTV, e-mode, isolation). See spec §5.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, U256};
use crate::token::{RateMode, TokenRef};

/// Aave efficiency-mode (e-mode) category id, a single byte.
pub type EModeCategory = u8;

/// Account-level aggregate of a single lending-market position, holding the
/// collateral/debt breakdown plus the risk metadata (HF, LTV, e-mode, isolation).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LendingAccount {
    /// Lending market this account belongs to.
    pub market: MarketRef,
    /// Collateral assets supplied, as `(token, underlying amount)` pairs.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collaterals: Vec<(TokenRef, U256)>,
    /// Borrowed assets, as `(token, underlying amount, rate mode)` tuples.
    #[tsify(type = "Array<[TokenRef, string, RateMode]>")]
    pub debts: Vec<(TokenRef, U256, RateMode)>,
    /// Active efficiency-mode category, if the account has e-mode enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub emode: Option<EModeCategory>,
    /// Whether the account is in isolation mode.
    pub is_isolated: bool,

    /// Account health factor; below 1.0 the position is liquidatable.
    pub health_factor: LiveField<Decimal>,
    /// Current loan-to-value ratio of the account.
    pub ltv: LiveField<Decimal>,
    /// LTV threshold at which the account becomes eligible for liquidation.
    pub liquidation_threshold: LiveField<Decimal>,
}
