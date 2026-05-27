//! LendingAccount — 한 lending market 에서의 *계정 집계*.
//!
//! 개별 supply/borrow 의 ERC20 잔고 (aUSDC, variableDebtUSDC) 는 tokens 안에 살고,
//! 여기서는 그 위의 집계 메타 (HF, LTV, emode, isolation) 만 담는다. spec §5.

use serde::{Deserialize, Serialize};

use crate::live_field::LiveField;
use crate::primitives::{Decimal, MarketRef, U256};
use crate::token::{RateMode, TokenRef};

/// Aave 의 efficiency mode 카테고리 id. 1-bytes.
pub type EModeCategory = u8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LendingAccount {
    pub market: MarketRef,
    /// 담보 자산 목록 (token, underlying amount).
    pub collaterals: Vec<(TokenRef, U256)>,
    /// 부채 자산 목록 (token, underlying amount, rate mode).
    pub debts: Vec<(TokenRef, U256, RateMode)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emode: Option<EModeCategory>,
    pub is_isolated: bool,

    pub health_factor: LiveField<Decimal>,
    pub ltv: LiveField<Decimal>,
    pub liquidation_threshold: LiveField<Decimal>,
}
