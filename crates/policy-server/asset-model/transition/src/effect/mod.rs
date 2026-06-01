//! Per-action `Reducer` trait implementations.
//!
//! Cross-cutting / low-fanout domains live as single files:
//!   - [`token`]     — `Erc20Approve` / `Permit` / `Transfer` / NFT ops
//!   - [`airdrop`]   — `Claim` / `Delegate`
//!   - [`launchpad`] — `Commit` / `ClaimAllocation` / `ClaimVested` / ...
//!
//! Protocol-rich domains use a subdirectory with one file per action and one
//! file per venue's math. Every variant of `AmmVenue` / `LendingVenue` /
//! `PerpVenue` (except the catch-all `PerpVenue::Generic`) has a corresponding
//! stub file:
//!   - [`amm`]     — swap / add+remove liquidity / ... + per-protocol math
//!   - [`lending`] — supply / borrow / repay / ... + per-protocol math
//!   - [`perp`]    — open / close / `place_order` / ... + per-protocol math

pub mod airdrop;
pub mod amm;
pub mod hyperliquid_core;
pub mod launchpad;
pub mod lending;
pub mod liquid_staking;
pub mod permission;
pub mod perp;
pub mod restaking;
pub mod staking;
pub mod token;
pub mod yield_;
