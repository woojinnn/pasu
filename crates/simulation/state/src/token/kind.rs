//! `TokenKind` — semantic classification (10 variants). spec §4.3.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use super::lp::{LpShape, ShareForm};
use super::token_ref::TokenRef;
use crate::primitives::{PoolRef, ProtocolRef, Time, U256};

/// Off-chain fiat currency a token may be pegged to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum FiatCurrency {
    /// United States dollar.
    Usd,
    /// Euro.
    Eur,
    /// South Korean won.
    Krw,
    /// Japanese yen.
    Jpy,
    /// British pound sterling.
    Gbp,
}

/// The reference asset a pegged token tracks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PegTarget {
    /// Off-chain fiat currency (e.g. USDC → USD).
    Fiat(FiatCurrency),
    /// On-chain asset (e.g. WETH → ETH).
    Token(TokenRef),
}

/// Broad category of a base (non-derivative) asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseCategory {
    /// Stablecoin pegged to a low-volatility reference.
    Stable,
    /// Volatile, free-floating asset.
    Volatile,
    /// ERC20 wrapper of a gas asset (WETH, WSOL).
    NativeWrap,
    /// Governance token (UNI, COMP). All governance matching uses this pattern.
    Governance {
        /// Protocol whose governance this token controls.
        protocol: ProtocolRef,
    },
}

/// How tightly a wrapped token tracks its underlying asset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum PegKind {
    /// Strictly redeemable 1:1 peg.
    HardPeg,
    /// Market-maintained peg that may deviate from par.
    SoftPeg,
    /// Balance grows over time via rebasing.
    Rebasing,
}

/// How yield accrual is reflected for a yield-bearing receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum RebaseForm {
    /// Holder balance increases as yield accrues (aToken style).
    Rebasing,
    /// Balance is fixed; value accrues via a rising exchange index (cToken style).
    Index,
}

/// Interest rate model applied to a debt receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum RateMode {
    /// Floating market-driven rate.
    Variable,
    /// Stable rate that may be rebalanced.
    Stable,
    /// Locked fixed rate.
    Fixed,
}

/// When a staked / locked asset becomes withdrawable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlockSchedule {
    /// Unlocks fully at a single point in time (e.g. esGMX).
    Cliff {
        /// Timestamp at which the asset unlocks.
        unlock_at: Time,
    },
    /// Unlocks linearly over a window (vesting-like).
    Linear {
        /// Start of the linear unlock window.
        start: Time,
        /// End of the linear unlock window.
        end: Time,
    },
    /// Unlocks after a cooldown initiated by the user.
    Cooldown {
        /// Cooldown duration in seconds before withdrawal is allowed.
        cooldown_secs: u64,
    },
}

/// Which leg of a maturity instrument a note represents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// Principal token (Pendle PT).
    Principal,
    /// Yield token (Pendle YT).
    Yield,
}

/// Semantic classification of a token; the core of policy pattern matching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TokenKind {
    /// Base asset (USDC, USDT, WBTC, WETH, UNI, etc.).
    Base {
        /// Broad category of the base asset.
        category: BaseCategory,
        /// Optional peg target if the asset tracks a reference.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        peg_to: Option<PegTarget>,
    },

    /// True native gas asset (ETH, SOL). Separated to enable gas-only policies.
    NativeGas,

    /// 1:1 / near-peg wrapper (stETH, wstETH, etc.); carries yield + smart-contract risk.
    Wrapped {
        /// Underlying asset being wrapped.
        underlying: TokenRef,
        /// How tightly the wrapper tracks its underlying.
        peg_kind: PegKind,
    },

    /// LP share — Pooled / Concentrated / Custom.
    LpShare {
        /// Pool the share belongs to.
        pool: PoolRef,
        /// Underlying assets backing the share.
        underlyings: Vec<TokenRef>,
        /// Form of the LP share representation.
        share_form: ShareForm,
        /// Liquidity shape (e.g. pooled vs concentrated range).
        shape: LpShape,
    },

    /// Lending receipt (aUSDC, cUSDC) — "I have lent something out".
    YieldReceipt {
        /// Lending protocol that issued the receipt.
        protocol: ProtocolRef,
        /// Underlying asset that was supplied.
        underlying: TokenRef,
        /// How yield accrual is reflected.
        rebase_form: RebaseForm,
    },

    /// Debt receipt (variableDebtUSDC) — "I have borrowed something".
    DebtReceipt {
        /// Lending protocol that issued the debt token.
        protocol: ProtocolRef,
        /// Underlying asset that was borrowed.
        underlying: TokenRef,
        /// Interest rate model applied to the debt.
        rate_mode: RateMode,
    },

    /// Lockup / staking receipt (stkAAVE, esGMX, ve-tokens).
    StakeReceipt {
        /// Protocol the asset is staked / locked in.
        protocol: ProtocolRef,
        /// Underlying asset that was staked.
        underlying: TokenRef,
        /// Optional schedule describing when the asset unlocks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unlock: Option<UnlockSchedule>,
        /// Optional voting power conferred by the stake (raw U256).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional, type = "string")]
        voting_power: Option<U256>,
    },

    /// Airdrop / points claim entitlement (only when issued as a balance).
    PointsToken {
        /// Protocol that is the source of the points.
        source: ProtocolRef,
    },

    /// Maturity-dated asset (Pendle PT/YT, etc.).
    MaturityNote {
        /// Protocol that issued the maturity note.
        protocol: ProtocolRef,
        /// Underlying asset of the note.
        underlying: TokenRef,
        /// Maturity timestamp.
        maturity: Time,
        /// Which leg (principal or yield) the note represents.
        note_kind: NoteKind,
    },

    /// Unrecognized token — policy default is "warn".
    Unknown,
}
