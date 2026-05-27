//! TokenKind — 의미 분류 (10 variants). spec §4.3.

use serde::{Deserialize, Serialize};

use super::lp::{LpShape, ShareForm};
use super::token_ref::TokenRef;
use crate::primitives::{PoolRef, ProtocolRef, Time, U256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FiatCurrency {
    Usd,
    Eur,
    Krw,
    Jpy,
    Gbp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PegTarget {
    /// 오프체인 법정화폐 (USDC → USD 등).
    Fiat(FiatCurrency),
    /// 온체인 자산 (WETH → ETH).
    Token(TokenRef),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseCategory {
    Stable,
    Volatile,
    /// gas asset 의 ERC20 wrapper (WETH, WSOL).
    NativeWrap,
    /// governance token (UNI, COMP). 모든 governance 매칭은 이 패턴으로.
    Governance { protocol: ProtocolRef },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PegKind {
    HardPeg,
    SoftPeg,
    Rebasing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebaseForm {
    Rebasing,
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateMode {
    Variable,
    Stable,
    Fixed,
}

/// Stake / lock 자산이 언제 풀리는지.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlockSchedule {
    /// 단일 시각에 unlock (esGMX 등).
    Cliff { unlock_at: Time },
    /// 선형 unlock (vesting 비슷).
    Linear { start: Time, end: Time },
    /// 사용자 개시 후 cooldown.
    Cooldown { cooldown_secs: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// 원금 토큰 (Pendle PT).
    Principal,
    /// 이자 토큰 (Pendle YT).
    Yield,
}

/// 토큰의 의미 분류. 정책 패턴 매칭의 핵심.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenKind {
    /// 기초 자산 (USDC, USDT, WBTC, WETH, UNI 등).
    Base {
        category: BaseCategory,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peg_to: Option<PegTarget>,
    },

    /// 진짜 native (ETH, SOL). Gas 전용 정책을 위해 분리.
    NativeGas,

    /// 1:1/유사 페그 wrapper (stETH, wstETH 등). yield + smart-contract 리스크.
    Wrapped {
        underlying: TokenRef,
        peg_kind: PegKind,
    },

    /// LP share — Pooled / Concentrated / Custom.
    LpShare {
        pool: PoolRef,
        underlyings: Vec<TokenRef>,
        share_form: ShareForm,
        shape: LpShape,
    },

    /// 대출 영수증 (aUSDC, cUSDC) — "내가 빌려준 게 있다".
    YieldReceipt {
        protocol: ProtocolRef,
        underlying: TokenRef,
        rebase_form: RebaseForm,
    },

    /// 부채 영수증 (variableDebtUSDC) — "내가 빌린 게 있다".
    DebtReceipt {
        protocol: ProtocolRef,
        underlying: TokenRef,
        rate_mode: RateMode,
    },

    /// 락업/스테이킹 영수증 (stkAAVE, esGMX, ve토큰).
    StakeReceipt {
        protocol: ProtocolRef,
        underlying: TokenRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unlock: Option<UnlockSchedule>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voting_power: Option<U256>,
    },

    /// 에어드랍/포인트 클레임권 (잔고형으로 발급된 경우만).
    PointsToken { source: ProtocolRef },

    /// 만기 자산 (Pendle PT/YT 등).
    MaturityNote {
        protocol: ProtocolRef,
        underlying: TokenRef,
        maturity: Time,
        note_kind: NoteKind,
    },

    /// 처음 보는 토큰 — policy 기본값 "경고".
    Unknown,
}
