//! `StakingAction` — Curve DAO staking & vote-escrow: veCRV vote-locking, CRV
//! reward minting, gauge-weight voting, and (reserved) gauge LP staking.
//!
//! **Distinct from `liquid_staking`** (Lido stETH/wstETH, a *liquid* derivative):
//! this domain models *non-liquid* staking — veCRV locks CRV for a fixed term
//! (non-transferable vote-escrow), the `Minter` mints accrued CRV, and the
//! `GaugeController` allocates vote weight. New domain (extension-guide axis 1).
//!
//! Mirrors the `liquid_staking` layout: a venue enum (`StakeVenue`) + per-action
//! structs + `action_tag()` / `venue_name()`. Actions carry **no** `LiveField`
//! inputs — the `ActionBody` is a faithful static decode of the on-chain intent;
//! APR/boost/lock-state enrichment is deferred to a later pass.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId};

pub mod claim_rewards;
pub mod gauge_deposit;
pub mod gauge_withdraw;
pub mod increase_lock_amount;
pub mod increase_lock_time;
pub mod lock;
pub mod unlock;
pub mod vote_for_gauge;

pub use self::claim_rewards::*;
pub use self::gauge_deposit::*;
pub use self::gauge_withdraw::*;
pub use self::increase_lock_amount::*;
pub use self::increase_lock_time::*;
pub use self::lock::*;
pub use self::unlock::*;
pub use self::vote_for_gauge::*;

/// User-level staking / vote-escrow actions across supported venues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum StakingAction {
    /// Lock a governance token for vote-escrow (Curve veCRV `create_lock`).
    Lock(LockAction),
    /// Add tokens to an existing lock (veCRV `increase_amount` / `deposit_for`).
    IncreaseLockAmount(IncreaseLockAmountAction),
    /// Extend the unlock time of an existing lock (veCRV `increase_unlock_time`).
    IncreaseLockTime(IncreaseLockTimeAction),
    /// Withdraw an expired vote-escrow lock (veCRV `withdraw`).
    Unlock(UnlockAction),
    /// Mint/claim accrued reward tokens (Curve `Minter.mint` / `mint_for` / `mint_many`).
    ClaimRewards(ClaimRewardsAction),
    /// Allocate vote-escrow weight to a gauge (`GaugeController.vote_for_gauge_weights`).
    VoteForGauge(VoteForGaugeAction),
    /// Stake LP into a Curve liquidity gauge (gauge `deposit`).
    GaugeDeposit(GaugeDepositAction),
    /// Unstake LP from a Curve liquidity gauge (gauge `withdraw`).
    GaugeWithdraw(GaugeWithdrawAction),
}

impl StakingAction {
    /// The action's `serde` `action` tag (e.g. `"lock"`, `"claim_rewards"`).
    ///
    /// Matches the `#[serde(tag = "action", rename_all = "snake_case")]`
    /// discriminant exactly; verified against `serde_json` output in tests.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Lock(_) => "lock",
            Self::IncreaseLockAmount(_) => "increase_lock_amount",
            Self::IncreaseLockTime(_) => "increase_lock_time",
            Self::Unlock(_) => "unlock",
            Self::ClaimRewards(_) => "claim_rewards",
            Self::VoteForGauge(_) => "vote_for_gauge",
            Self::GaugeDeposit(_) => "gauge_deposit",
            Self::GaugeWithdraw(_) => "gauge_withdraw",
        }
    }

    /// The venue `name` of the wrapped action. Every staking action carries a venue.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        match self {
            Self::Lock(a) => Some(a.venue.name()),
            Self::IncreaseLockAmount(a) => Some(a.venue.name()),
            Self::IncreaseLockTime(a) => Some(a.venue.name()),
            Self::Unlock(a) => Some(a.venue.name()),
            Self::ClaimRewards(a) => Some(a.venue.name()),
            Self::VoteForGauge(a) => Some(a.venue.name()),
            Self::GaugeDeposit(a) => Some(a.venue.name()),
            Self::GaugeWithdraw(a) => Some(a.venue.name()),
        }
    }
}

// ---------------------------------------------------------------------------
// Venue
// ---------------------------------------------------------------------------

/// Staking / vote-escrow venue identifier. Each variant is one Curve DAO
/// contract; the single contract address is carried per-variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "name", rename_all = "snake_case")]
pub enum StakeVenue {
    /// Curve `VotingEscrow` (veCRV) — locks CRV for vote-escrow.
    CurveVotingEscrow {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// `VotingEscrow` contract address.
        #[tsify(type = "string")]
        escrow: Address,
    },
    /// Curve CRV `Minter` — mints accrued CRV emissions.
    CurveMinter {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// `Minter` contract address.
        #[tsify(type = "string")]
        minter: Address,
    },
    /// Curve `GaugeController` — allocates gauge vote weight.
    CurveGaugeController {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// `GaugeController` contract address.
        #[tsify(type = "string")]
        controller: Address,
    },
    /// Curve liquidity gauge (per-pool LP staking). Reserved for the gauge slice.
    CurveGauge {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// Liquidity gauge contract address.
        #[tsify(type = "string")]
        gauge: Address,
    },
    /// Curve `FeeDistributor` — distributes protocol fees (3CRV / crvUSD) to
    /// veCRV lockers; a user `claim()`s their accrued share.
    CurveFeeDistributor {
        /// Chain hosting the deployment.
        chain: ChainId,
        /// `FeeDistributor` contract address.
        #[tsify(type = "string")]
        distributor: Address,
    },
}

impl StakeVenue {
    /// The venue's `serde` `name` tag (e.g. `"curve_voting_escrow"`).
    ///
    /// Matches the `#[serde(tag = "name", rename_all = "snake_case")]`
    /// discriminant exactly and is verified against `serde_json` output in tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CurveVotingEscrow { .. } => "curve_voting_escrow",
            Self::CurveMinter { .. } => "curve_minter",
            Self::CurveGaugeController { .. } => "curve_gauge_controller",
            Self::CurveGauge { .. } => "curve_gauge",
            Self::CurveFeeDistributor { .. } => "curve_fee_distributor",
        }
    }
}
