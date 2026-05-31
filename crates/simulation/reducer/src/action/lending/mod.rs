//! `LendingAction` — `Supply` / `Withdraw` / `Borrow` / `Repay`, etc. See spec §6.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId, Decimal, U256};
use simulation_state::token::TokenRef;

pub mod borrow;
pub mod buy_collateral;
pub mod delegate_borrow;
pub mod liquidate;
pub mod repay;
pub mod set_authorization;
pub mod set_collateral;
pub mod set_emode;
pub mod supply;
pub mod swap_rate_mode;
pub mod withdraw;

pub use self::borrow::*;
pub use self::buy_collateral::*;
pub use self::delegate_borrow::*;
pub use self::liquidate::*;
pub use self::repay::*;
pub use self::set_authorization::*;
pub use self::set_collateral::*;
pub use self::set_emode::*;
pub use self::supply::*;
pub use self::swap_rate_mode::*;
pub use self::withdraw::*;

/// User-level lending actions across supported venues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum LendingAction {
    /// Supply (`deposit`) an asset into a lending market.
    Supply(SupplyAction),
    /// Withdraw a previously supplied asset.
    Withdraw(WithdrawAction),
    /// Borrow an asset against existing collateral.
    Borrow(BorrowAction),
    /// Buy collateral being sold from protocol reserves.
    BuyCollateral(BuyCollateralAction),
    /// Repay an outstanding debt position.
    Repay(RepayAction),
    /// `Aave`-specific — switch between `Variable` and `Stable` borrow rate modes.
    SwapRateMode(SwapRateModeAction),
    /// `Aave V3` e-mode selection.
    SetEMode(SetEModeAction),
    /// Mark an asset as collateral.
    EnableCollateral(SetCollateralAction),
    /// Unmark an asset as collateral.
    DisableCollateral(SetCollateralAction),
    /// `Aave` credit delegation.
    DelegateBorrow(DelegateBorrowAction),
    /// Liquidate an unhealthy position; typically not invoked from a user wallet, included for completeness.
    Liquidate(LiquidateAction),
    /// Grant or revoke an operator's full control over the submitter's positions
    /// (`Morpho Blue` `setAuthorization` / off-chain `Authorization`). Account-wide.
    SetAuthorization(SetAuthorizationAction),
}

impl LendingAction {
    /// The action's `serde` `action` tag (e.g. `"borrow"`, `"set_e_mode"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Supply(_) => "supply",
            Self::Withdraw(_) => "withdraw",
            Self::Borrow(_) => "borrow",
            Self::BuyCollateral(_) => "buy_collateral",
            Self::Repay(_) => "repay",
            Self::SwapRateMode(_) => "swap_rate_mode",
            Self::SetEMode(_) => "set_e_mode",
            Self::EnableCollateral(_) => "enable_collateral",
            Self::DisableCollateral(_) => "disable_collateral",
            Self::DelegateBorrow(_) => "delegate_borrow",
            Self::Liquidate(_) => "liquidate",
            Self::SetAuthorization(_) => "set_authorization",
        }
    }

    /// The venue `name` of the wrapped action. Every lending action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Supply(a) => Some(a.venue.name()),
            Self::Withdraw(a) => Some(a.venue.name()),
            Self::Borrow(a) => Some(a.venue.name()),
            Self::BuyCollateral(a) => Some(a.venue.name()),
            Self::Repay(a) => Some(a.venue.name()),
            Self::SwapRateMode(a) => Some(a.venue.name()),
            Self::SetEMode(a) => Some(a.venue.name()),
            Self::EnableCollateral(a) | Self::DisableCollateral(a) => Some(a.venue.name()),
            Self::DelegateBorrow(a) => Some(a.venue.name()),
            Self::Liquidate(a) => Some(a.venue.name()),
            // Account-wide grant — no market venue.
            Self::SetAuthorization(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Lending venue identifier with venue-specific addressing fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum LendingVenue {
    /// `Aave V3` deployment on a given chain.
    AaveV3 {
        /// Chain hosting the pool.
        chain: ChainId,
        /// `Pool` contract address.
        #[tsify(type = "string")]
        pool: Address,
        /// Optional sub-market identifier (used by some `Aave V3` forks).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[tsify(optional)]
        market_id: Option<u8>,
    },
    /// `Aave V2` deployment on a given chain.
    AaveV2 {
        /// Chain hosting the pool.
        chain: ChainId,
        /// `LendingPool` contract address.
        #[tsify(type = "string")]
        pool: Address,
    },
    /// `Compound V3` (`Comet`) deployment.
    CompoundV3 {
        /// Chain hosting the `Comet` market.
        chain: ChainId,
        /// `Comet` contract address.
        #[tsify(type = "string")]
        comet: Address,
        /// Base asset of this `Comet` market.
        base_asset: TokenRef,
    },
    /// `Compound V2` deployment.
    CompoundV2 {
        /// Chain hosting the comptroller.
        chain: ChainId,
        /// `Comptroller` contract address.
        #[tsify(type = "string")]
        comptroller: Address,
    },
    /// `Morpho Blue` market — `market_id = keccak((loan, collat, oracle, irm, lltv))`.
    MorphoBlue {
        /// Chain hosting `Morpho Blue`.
        chain: ChainId,
        /// `Market` id as a hex string.
        market_id: String,
    },
    /// `Morpho Optimizer` vault on top of `Aave` / `Compound`.
    MorphoOptimizer {
        /// Chain hosting the vault.
        chain: ChainId,
        /// Vault contract address.
        #[tsify(type = "string")]
        vault: Address,
    },
    /// `Spark` lending pool (`Aave V3` fork).
    Spark {
        /// Chain hosting the pool.
        chain: ChainId,
        /// `Pool` contract address.
        #[tsify(type = "string")]
        pool: Address,
    },
    /// `Fluid` lending vault.
    Fluid {
        /// Chain hosting the vault.
        chain: ChainId,
        /// Vault contract address.
        #[tsify(type = "string")]
        vault: Address,
    },
    /// `Curve` crvUSD lending market. One `Controller` per collateral token;
    /// the debt asset is always crvUSD. (`create_loan`/`borrow_more` deposit the
    /// market's `collateral` and mint crvUSD debt.)
    CrvUsd {
        /// Chain hosting the controller.
        chain: ChainId,
        /// `Controller` contract address (one per collateral market).
        #[tsify(type = "string")]
        controller: Address,
        /// Collateral token of this market.
        collateral: TokenRef,
    },
    /// `Curve` LlamaLend (`OneWayLendingFactory`) market. Same `Controller`
    /// interface as crvUSD, but the borrowed asset comes from a permissionless
    /// ERC-4626 `Vault` (lender deposits) rather than crvUSD minting — distinct
    /// risk/liquidity profile, so it carries its own venue tag. One `Controller`
    /// per (collateral, borrowed) market; the borrowed asset is named by the
    /// action's `asset` field (the top markets all borrow crvUSD).
    LlamaLend {
        /// Chain hosting the controller.
        chain: ChainId,
        /// `Controller` contract address (one per market).
        #[tsify(type = "string")]
        controller: Address,
        /// Collateral token of this market.
        collateral: TokenRef,
    },
}

impl LendingVenue {
    /// The venue's `serde` `name` tag (e.g. `"aave_v3"`, `"morpho_blue"`).
    ///
    /// These strings match the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminants exactly and are verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::AaveV3 { .. } => "aave_v3",
            Self::AaveV2 { .. } => "aave_v2",
            Self::CompoundV3 { .. } => "compound_v3",
            Self::CompoundV2 { .. } => "compound_v2",
            Self::MorphoBlue { .. } => "morpho_blue",
            Self::MorphoOptimizer { .. } => "morpho_optimizer",
            Self::Spark { .. } => "spark",
            Self::Fluid { .. } => "fluid",
            Self::CrvUsd { .. } => "crv_usd",
            Self::LlamaLend { .. } => "llama_lend",
        }
    }
}

// ---------------------------------------------------------------------------
// Shared sub-types
// ---------------------------------------------------------------------------

/// Reserve-level metadata — supply/borrow caps, `LTV`, etc.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ReserveState {
    /// Total supplied amount in the reserve (asset units).
    #[tsify(type = "string")]
    pub total_supply: U256,
    /// Total borrowed amount from the reserve (asset units).
    #[tsify(type = "string")]
    pub total_borrow: U256,
    /// Current utilization in basis points.
    pub utilization_bp: u32,
    /// Optional supply cap (asset units).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub supply_cap: Option<U256>,
    /// Optional borrow cap (asset units).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub borrow_cap: Option<U256>,
    /// Loan-to-value in basis points.
    pub ltv_bp: u32,
    /// Liquidation threshold in basis points.
    pub liquidation_threshold_bp: u32,
    /// Liquidation bonus in basis points.
    pub liquidation_bonus_bp: u32,
    /// Reserve factor in basis points.
    pub reserve_factor_bp: u32,
    /// Whether the reserve is frozen (no new positions).
    pub is_frozen: bool,
    /// Whether the reserve is paused (no interactions).
    pub is_paused: bool,
}

/// Aggregated lending account state for one user — mirrors `Aave`'s `getUserAccountData`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct UserLendingState {
    /// Current health factor.
    pub health_factor: Decimal,
    /// Total collateral value in USD (scaled).
    #[tsify(type = "string")]
    pub total_collat_usd: U256,
    /// Total debt value in USD (scaled).
    #[tsify(type = "string")]
    pub total_debt_usd: U256,
    /// Remaining borrowing power in USD (scaled).
    #[tsify(type = "string")]
    pub available_borrow_usd: U256,
}
