//! `LiquidStakingAction` — Lido-style liquid staking: stake (`submit`),
//! wrap/unwrap, withdrawal-queue request/claim, and shares-denominated transfer.
//!
//! New domain (extension-guide axis 1). Mirrors the `lending` layout: a venue
//! enum (`StakingVenue`) + per-action structs + `action_tag()` / `venue_name()`.
//! The exchange-rate conversions (`wrap` / `unwrap` / `transfer_shares`) carry a
//! `LiveField` input each — the host fills the wstETH/stETH/pooled-ETH amount so
//! the user sees the concrete value behind the abstract wrapper/shares unit. The
//! remaining actions (`stake` / `request_withdrawal` / `claim_withdrawal`) are
//! still faithful static decodes; their enrichment is deferred to a later pass.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::ChainId;

pub mod claim_withdrawal;
pub mod request_withdrawal;
pub mod stake;
pub mod transfer_shares;
pub mod unwrap;
pub mod wrap;

pub use self::claim_withdrawal::*;
pub use self::request_withdrawal::*;
pub use self::stake::*;
pub use self::transfer_shares::*;
pub use self::unwrap::*;
pub use self::wrap::*;

/// User-level liquid-staking actions across supported venues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum LiquidStakingAction {
    /// Stake native ETH and receive a liquid-staking token (Lido `submit`).
    Stake(StakeAction),
    /// Wrap the rebasing staking token into its non-rebasing wrapper (`wrap`).
    Wrap(WrapAction),
    /// Unwrap the non-rebasing wrapper back into the rebasing token (`unwrap`).
    Unwrap(UnwrapAction),
    /// Request a withdrawal — enter the withdrawal queue (mints request NFTs).
    RequestWithdrawal(RequestWithdrawalAction),
    /// Claim a finalized withdrawal — redeem queued request(s) for ETH.
    ClaimWithdrawal(ClaimWithdrawalAction),
    /// Transfer the staking token denominated in protocol shares (`transferShares`).
    TransferShares(TransferSharesAction),
}

impl LiquidStakingAction {
    /// The action's `serde` `action` tag (e.g. `"stake"`, `"request_withdrawal"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Stake(_) => "stake",
            Self::Wrap(_) => "wrap",
            Self::Unwrap(_) => "unwrap",
            Self::RequestWithdrawal(_) => "request_withdrawal",
            Self::ClaimWithdrawal(_) => "claim_withdrawal",
            Self::TransferShares(_) => "transfer_shares",
        }
    }

    /// The venue `name` of the wrapped action. Every liquid-staking action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Stake(a) => Some(a.venue.name()),
            Self::Wrap(a) => Some(a.venue.name()),
            Self::Unwrap(a) => Some(a.venue.name()),
            Self::RequestWithdrawal(a) => Some(a.venue.name()),
            Self::ClaimWithdrawal(a) => Some(a.venue.name()),
            Self::TransferShares(a) => Some(a.venue.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Liquid-staking venue identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum StakingVenue {
    /// `Lido` deployment on a given chain (stETH / wstETH / `WithdrawalQueue` set).
    Lido {
        /// Chain hosting the `Lido` deployment.
        chain: ChainId,
    },
}

impl StakingVenue {
    /// The venue's `serde` `name` tag (e.g. `"lido"`).
    ///
    /// Matches the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminant exactly and is verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Lido { .. } => "lido",
        }
    }
}
