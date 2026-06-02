//! `LendingAccount` stores account-level lending-market metadata.
//!
//! The per-asset supply/borrow ERC20 balances (e.g. aUSDC, variableDebtUSDC)
//! live in `tokens`; this struct only holds the aggregate metadata layered on
//! top of them (HF, LTV, e-mode, isolation).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, U256};
use crate::token::{RateMode, TokenRef};

/// Aave efficiency-mode category id.
pub type EModeCategory = u8;

/// Account aggregate for one lending market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LendingAccount {
    /// Lending market this account belongs to.
    pub market: MarketRef,
    /// Collateral assets as `(token, underlying amount)`.
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collaterals: Vec<(TokenRef, U256)>,
    /// Debt assets as `(token, underlying amount, rate mode)`.
    #[tsify(type = "Array<[TokenRef, string, RateMode]>")]
    pub debts: Vec<(TokenRef, U256, RateMode)>,
    /// Aave eMode category id; `None` when disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub emode: Option<EModeCategory>,
    /// Whether Aave isolation mode is active.
    pub is_isolated: bool,

    /// Account health factor (`collateral_usd * liquidation_threshold / debt_usd`).
    pub health_factor: LiveField<Decimal>,
    /// Account loan-to-value ratio.
    pub ltv: LiveField<Decimal>,
    /// Account liquidation threshold.
    pub liquidation_threshold: LiveField<Decimal>,
}
