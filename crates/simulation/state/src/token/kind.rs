//! TokenKind — 의미 분류 (10 variants). spec §4.3.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::lp::{LpShape, ShareForm};
use super::token_ref::TokenRef;
use crate::primitives::{PoolRef, ProtocolRef, Time, U256};

/// 가격 매김 / peg 의 기준이 되는 법정화폐.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum FiatCurrency {
    /// 미국 달러.
    Usd,
    /// 유로.
    Eur,
    /// 한국 원.
    Krw,
    /// 일본 엔.
    Jpy,
    /// 영국 파운드.
    Gbp,
}

/// peg 의 대상 — 법정화폐 또는 다른 온체인 자산.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PegTarget {
    /// 오프체인 법정화폐 (USDC → USD 등).
    Fiat(FiatCurrency),
    /// 온체인 자산 (WETH → ETH).
    Token(TokenRef),
}

/// 기초 자산의 sub-category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseCategory {
    /// 스테이블코인 (USDC, USDT, DAI 등).
    Stable,
    /// 변동성 자산 (WBTC, WETH 등 — non-stable).
    Volatile,
    /// gas asset 의 ERC20 wrapper (WETH, WSOL).
    NativeWrap,
    /// governance token (UNI, COMP). 모든 governance 매칭은 이 패턴으로.
    Governance {
        /// governance 가 속한 프로토콜 식별자.
        protocol: ProtocolRef,
    },
}

/// peg 의 안정성 / 메커니즘 분류.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PegKind {
    /// 1:1 hard peg (담보 / 발행 직접 통제).
    HardPeg,
    /// 시장 균형으로 유지되는 soft peg.
    SoftPeg,
    /// rebase 메커니즘으로 동기화 (stETH, aToken 등).
    Rebasing,
}

/// yield 영수증의 잔고 동기화 방식.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum RebaseForm {
    /// 잔고 자체가 rebase (aToken v2 등).
    Rebasing,
    /// 잔고 고정 + index 곱셈으로 가치 표현 (cToken, aToken v3 등).
    Index,
}

/// 부채 영수증의 이자율 모드.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum RateMode {
    /// 가변 이자율.
    Variable,
    /// 안정 (rebalance 가능) 이자율.
    Stable,
    /// 고정 이자율 (만기 fixed).
    Fixed,
}

/// Stake / lock 자산이 언제 풀리는지.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlockSchedule {
    /// 단일 시각에 unlock (esGMX 등).
    Cliff {
        /// 잠금이 해제되는 시각.
        unlock_at: Time,
    },
    /// 선형 unlock (vesting 비슷).
    Linear {
        /// 선형 unlock 시작 시각.
        start: Time,
        /// 선형 unlock 종료 시각.
        end: Time,
    },
    /// 사용자 개시 후 cooldown.
    Cooldown {
        /// 사용자 개시 후 인출 가능까지 대기 초.
        cooldown_secs: u64,
    },
}

/// Pendle 형 만기 토큰의 sub-kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// 원금 토큰 (Pendle PT).
    Principal,
    /// 이자 토큰 (Pendle YT).
    Yield,
}

/// 토큰의 의미 분류. 정책 패턴 매칭의 핵심.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenKind {
    /// 기초 자산 (USDC, USDT, WBTC, WETH, UNI 등).
    Base {
        /// 기초 자산 sub-category.
        category: BaseCategory,
        /// peg 대상 (스테이블 / wrapped 자산 한정). 비-peg 자산은 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peg_to: Option<PegTarget>,
    },

    /// 진짜 native (ETH, SOL). Gas 전용 정책을 위해 분리.
    NativeGas,

    /// 1:1/유사 페그 wrapper (stETH, wstETH 등). yield + smart-contract 리스크.
    Wrapped {
        /// wrapper 의 기초 자산.
        underlying: TokenRef,
        /// peg 의 메커니즘.
        peg_kind: PegKind,
    },

    /// LP share — Pooled / Concentrated / Custom.
    LpShare {
        /// LP 가 속한 pool.
        pool: PoolRef,
        /// pool 의 기초 자산 list.
        underlyings: Vec<TokenRef>,
        /// share 가 fungible 인지 NFT 인지.
        share_form: ShareForm,
        /// share 의 가격 분포 모양.
        shape: LpShape,
    },

    /// 대출 영수증 (aUSDC, cUSDC) — "내가 빌려준 게 있다".
    YieldReceipt {
        /// yield 가 속한 프로토콜.
        protocol: ProtocolRef,
        /// 영수증의 기초 자산.
        underlying: TokenRef,
        /// 잔고가 rebase / index 중 어느 방식으로 동기화되는지.
        rebase_form: RebaseForm,
    },

    /// 부채 영수증 (variableDebtUSDC) — "내가 빌린 게 있다".
    DebtReceipt {
        /// 부채가 속한 프로토콜.
        protocol: ProtocolRef,
        /// 부채의 기초 자산.
        underlying: TokenRef,
        /// 이자율 모드.
        rate_mode: RateMode,
    },

    /// 락업/스테이킹 영수증 (stkAAVE, esGMX, ve토큰).
    StakeReceipt {
        /// stake 가 속한 프로토콜.
        protocol: ProtocolRef,
        /// stake 의 기초 자산.
        underlying: TokenRef,
        /// 잠금 해제 일정. 일정 없는 즉시 인출 가능 자산은 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unlock: Option<UnlockSchedule>,
        /// 본 stake 가 부여하는 voting power (ve 토큰 등). 비투표 자산은 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional, type = "string")]
        voting_power: Option<U256>,
    },

    /// 에어드랍/포인트 클레임권 (잔고형으로 발급된 경우만).
    PointsToken {
        /// 포인트를 발행한 프로토콜.
        source: ProtocolRef,
    },

    /// 만기 자산 (Pendle PT/YT 등).
    MaturityNote {
        /// 만기 토큰이 속한 프로토콜.
        protocol: ProtocolRef,
        /// 만기 토큰의 기초 자산.
        underlying: TokenRef,
        /// 만기 시각.
        maturity: Time,
        /// Principal (PT) / Yield (YT) 구분.
        note_kind: NoteKind,
    },

    /// 처음 보는 토큰 — policy 기본값 "경고".
    Unknown,
}
