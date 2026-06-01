//! `LendingAccount` — 한 lending market 에서의 *계정 집계*.
//!
//! 개별 supply/borrow 의 ERC20 잔고 (aUSDC, variableDebtUSDC) 는 tokens 안에 살고,
//! 여기서는 그 위의 집계 메타 (HF, LTV, emode, isolation) 만 담는다. spec §5.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, U256};
use crate::token::{RateMode, TokenRef};

/// Aave 의 efficiency mode 카테고리 id. 1-bytes.
pub type EModeCategory = u8;

/// 한 lending market 의 *계정 집계* — 담보 / 부채 / HF / LTV / emode / isolation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LendingAccount {
    /// 본 계정이 속한 lending market.
    pub market: MarketRef,
    /// 담보 자산 목록 (token, underlying amount).
    #[tsify(type = "Array<[TokenRef, string]>")]
    pub collaterals: Vec<(TokenRef, U256)>,
    /// 부채 자산 목록 (token, underlying amount, rate mode).
    #[tsify(type = "Array<[TokenRef, string, RateMode]>")]
    pub debts: Vec<(TokenRef, U256, RateMode)>,
    /// Aave eMode 카테고리 id. 비활성 시 `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub emode: Option<EModeCategory>,
    /// Aave isolation mode 활성 여부.
    pub is_isolated: bool,

    /// 본 계정의 health factor (`collateral_usd × liq_threshold / debt_usd`).
    pub health_factor: LiveField<Decimal>,
    /// 본 계정의 loan-to-value 비율.
    pub ltv: LiveField<Decimal>,
    /// 본 계정의 청산 임계값.
    pub liquidation_threshold: LiveField<Decimal>,
}
