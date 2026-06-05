//! `LendingAction` reducers.
//! One file per action; one file per venue's math.

// `build_price_tables`, `threshold_to_bp`) are wired in this commit but
// only consumed by later commits (borrow / repay / liquidate). Keep the
// allow until those land.
#![allow(dead_code)]

mod borrow;
mod buy_collateral;
mod delegate_borrow;
mod liquidate;
mod periphery_operation;
mod repay;
mod set_authorization;
mod set_collateral;
mod set_emode;
mod supply;
mod swap_rate_mode;
mod withdraw;

// Venue-specific math:
mod aave_v2;
mod aave_v3;
mod compound_v2;
mod compound_v3;
mod fluid;
mod morpho_blue;
mod morpho_optimizer;
mod shared;
mod spark;

use policy_state::position::{LendingAccount, PositionId};
use policy_state::primitives::{ChainId, U256};
use policy_state::token::{RateMode, TokenRef};
use policy_state::{EvalContext, PositionChange, StateDelta, WalletState};

use crate::action::lending::{LendingAction, LendingVenue};
use crate::apply::Reducer;
use crate::error::ReducerResult;

impl Reducer for LendingAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Supply(a) => a.apply(state, ctx),
            Self::Withdraw(a) => a.apply(state, ctx),
            Self::Borrow(a) => a.apply(state, ctx),
            Self::BuyCollateral(a) => a.apply(state, ctx),
            Self::Repay(a) => a.apply(state, ctx),
            Self::SwapRateMode(a) => a.apply(state, ctx),
            Self::SetEMode(a) => a.apply(state, ctx),
            Self::EnableCollateral(a) => set_collateral::apply(a, state, ctx, true),
            Self::DisableCollateral(a) => set_collateral::apply(a, state, ctx, false),
            Self::DelegateBorrow(a) => a.apply(state, ctx),
            Self::Liquidate(a) => a.apply(state, ctx),
            Self::SetAuthorization(a) => a.apply(state, ctx),
            Self::PeripheryOperation(a) => a.apply(state, ctx),
        }
    }
}

// ===========================================================================
// Shared helpers used across the per-action reducers.
// ===========================================================================

/// Deterministic position-id derivation for lending positions.
pub(super) mod position_id {
    use policy_state::position::PositionId;

    use crate::action::lending::LendingVenue;

    /// Stable id derived from `(venue, chain, address)`. All supplies /
    /// borrows targeting the same `(venue, chain, pool/comet/vault)` tuple
    /// merge into the same `LendingAccount` position.
    pub(super) fn for_venue(venue: &LendingVenue) -> PositionId {
        match venue {
            LendingVenue::AaveV3 {
                chain,
                pool,
                market_id,
            } => format!(
                "aave_v3:{}:{pool:?}:{}",
                chain.as_str(),
                market_id.unwrap_or(0)
            ),
            LendingVenue::AaveV2 { chain, pool } => {
                format!("aave_v2:{}:{pool:?}", chain.as_str())
            }
            LendingVenue::Spark { chain, pool } => {
                format!("spark:{}:{pool:?}", chain.as_str())
            }
            LendingVenue::CompoundV3 { chain, comet, .. } => {
                format!("compound_v3:{}:{comet:?}", chain.as_str())
            }
            LendingVenue::CompoundV2 { chain, comptroller } => {
                format!("compound_v2:{}:{comptroller:?}", chain.as_str())
            }
            LendingVenue::MorphoBlue { chain, market_id } => {
                format!("morpho_blue:{}:{market_id}", chain.as_str())
            }
            LendingVenue::MorphoOptimizer { chain, vault } => {
                format!("morpho_optimizer:{}:{vault:?}", chain.as_str())
            }
            LendingVenue::Fluid { chain, vault } => {
                format!("fluid:{}:{vault:?}", chain.as_str())
            }
            LendingVenue::MetaMorpho { chain, vault } => {
                format!("metamorpho:{}:{vault:?}", chain.as_str())
            }
            LendingVenue::CrvUsd {
                chain, controller, ..
            } => {
                format!("crv_usd:{}:{controller:?}", chain.as_str())
            }
            LendingVenue::LlamaLend {
                chain, controller, ..
            } => {
                format!("llama_lend:{}:{controller:?}", chain.as_str())
            }
            LendingVenue::AaveV3Periphery { chain, adapter } => {
                format!("aave_v3_periphery:{}:{adapter:?}", chain.as_str())
            }
        }
    }
}

/// Short tag matching the venue's spec name; used for diagnostics and
/// `ProtocolRef.name` fields.
pub(super) const fn venue_tag(venue: &LendingVenue) -> &'static str {
    match venue {
        LendingVenue::AaveV3 { .. } => "aave_v3",
        LendingVenue::AaveV2 { .. } => "aave_v2",
        LendingVenue::Spark { .. } => "spark",
        LendingVenue::CompoundV3 { .. } => "compound_v3",
        LendingVenue::CompoundV2 { .. } => "compound_v2",
        LendingVenue::MorphoBlue { .. } => "morpho_blue",
        LendingVenue::MorphoOptimizer { .. } => "morpho_optimizer",
        LendingVenue::Fluid { .. } => "fluid",
        LendingVenue::MetaMorpho { .. } => "metamorpho",
        LendingVenue::CrvUsd { .. } => "crv_usd",
        LendingVenue::LlamaLend { .. } => "llama_lend",
        LendingVenue::AaveV3Periphery { .. } => "aave_v3_periphery",
    }
}

/// Extract the venue's chain. Every variant currently has one; the helper
/// is exposed as a single function so future venues whose `chain` is
/// inferred at sync time can switch to `Option<ChainId>` without sweeping
/// call sites (in which case `position_chain` can stop calling `.clone()`).
pub(super) fn venue_chain(venue: &LendingVenue) -> ChainId {
    match venue {
        LendingVenue::AaveV3 { chain, .. }
        | LendingVenue::AaveV2 { chain, .. }
        | LendingVenue::Spark { chain, .. }
        | LendingVenue::CompoundV3 { chain, .. }
        | LendingVenue::CompoundV2 { chain, .. }
        | LendingVenue::MorphoBlue { chain, .. }
        | LendingVenue::MorphoOptimizer { chain, .. }
        | LendingVenue::Fluid { chain, .. }
        | LendingVenue::MetaMorpho { chain, .. }
        | LendingVenue::CrvUsd { chain, .. }
        | LendingVenue::LlamaLend { chain, .. }
        | LendingVenue::AaveV3Periphery { chain, .. } => chain.clone(),
    }
}

/// Returns `true` if a `LendingAccount` position with `pid` is part of the
/// effective state — checks both committed `state.positions` and pending
/// `delta.position_changes`. A queued `Close` removes the position from
/// effective state.
pub(super) fn position_exists(state: &WalletState, delta: &StateDelta, pid: &PositionId) -> bool {
    let in_state = state.positions.iter().any(|p| &p.id == pid);
    let mut exists = in_state;
    for change in &delta.position_changes {
        match change {
            PositionChange::Open { position } if &position.id == pid => exists = true,
            PositionChange::Close { id } if id == pid => exists = false,
            _ => {}
        }
    }
    exists
}

/// Merge an asset deposit into a `LendingAccount`'s `collaterals` list —
/// adds to the existing tuple if the asset is already present, otherwise
/// pushes a fresh entry.
pub(super) fn merge_collateral(account: &mut LendingAccount, asset: &TokenRef, amount: U256) {
    if let Some(entry) = account.collaterals.iter_mut().find(|(t, _)| t == asset) {
        entry.1 = entry.1.saturating_add(amount);
    } else {
        account.collaterals.push((asset.clone(), amount));
    }
}

/// Reduce an asset withdrawal from a `LendingAccount`'s `collaterals` list.
/// Returns the post-reduction amount; `0` evicts the entry. Returns
/// `Invariant` when the asset is missing or the withdrawal would underflow.
pub(super) fn reduce_collateral(
    account: &mut LendingAccount,
    asset: &TokenRef,
    amount: U256,
) -> ReducerResult<U256> {
    use crate::error::ReducerError;
    let idx = account
        .collaterals
        .iter()
        .position(|(t, _)| t == asset)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "withdraw: asset {:?} not present in collaterals",
                asset.key
            ))
        })?;
    let current = account.collaterals[idx].1;
    if current < amount {
        return Err(ReducerError::Invariant(format!(
            "withdraw: amount {amount} > collateral {current}"
        )));
    }
    let new_amount = current - amount;
    if new_amount == U256::ZERO {
        account.collaterals.remove(idx);
    } else {
        account.collaterals[idx].1 = new_amount;
    }
    Ok(new_amount)
}

/// Merge a borrow into a `LendingAccount`'s `debts` list, keyed by
/// `(token, rate_mode)`.
pub(super) fn merge_debt(
    account: &mut LendingAccount,
    asset: &TokenRef,
    amount: U256,
    rate_mode: &RateMode,
) {
    if let Some(entry) = account
        .debts
        .iter_mut()
        .find(|(t, _, r)| t == asset && r == rate_mode)
    {
        entry.1 = entry.1.saturating_add(amount);
    } else {
        account
            .debts
            .push((asset.clone(), amount, rate_mode.clone()));
    }
}

/// Reduce a repayment from a `LendingAccount`'s `debts` list. Returns the
/// post-reduction amount; `0` evicts the entry.
pub(super) fn reduce_debt(
    account: &mut LendingAccount,
    asset: &TokenRef,
    amount: U256,
    rate_mode: &RateMode,
) -> ReducerResult<U256> {
    use crate::error::ReducerError;
    let idx = account
        .debts
        .iter()
        .position(|(t, _, r)| t == asset && r == rate_mode)
        .ok_or_else(|| {
            ReducerError::Invariant(format!(
                "repay: asset {:?} not present in debts for rate_mode {rate_mode:?}",
                asset.key
            ))
        })?;
    let current = account.debts[idx].1;
    if current < amount {
        return Err(ReducerError::Invariant(format!(
            "repay: amount {amount} > debt {current}"
        )));
    }
    let new_amount = current - amount;
    if new_amount == U256::ZERO {
        account.debts.remove(idx);
    } else {
        account.debts[idx].1 = new_amount;
    }
    Ok(new_amount)
}

/// Parallel-slice tables consumed by `helpers::derived::recompute_*`:
/// `(collateral_prices, debt_prices, liquidation_thresholds_bp)`.
pub(super) type PriceTables = (
    Vec<(TokenRef, policy_state::primitives::Decimal)>,
    Vec<(TokenRef, policy_state::primitives::Decimal)>,
    Vec<(TokenRef, u32)>,
);

/// Build the parallel-slice tables expected by `helpers::derived::recompute_*`
/// from a `LendingAccount` plus a `(token, price)` source. Used by
/// `borrow.rs` / `withdraw.rs` / `liquidate.rs` to call into the HF helper.
pub(super) fn build_price_tables(
    account: &LendingAccount,
    asset_price: &policy_state::primitives::Price,
    referenced_asset: &TokenRef,
) -> PriceTables {
    let mut collat_prices = Vec::new();
    let mut debt_prices = Vec::new();
    let mut lts = Vec::new();
    for (token, _) in &account.collaterals {
        collat_prices.push((token.clone(), asset_price.clone()));
        // PDF §5: HF needs per-asset LT. Absent richer per-asset data we
        // use the account-level liquidation_threshold LiveField as the
        // common value, recovering the LiquidationThreshold field that the
        // derived HF helper expects in basis points (string Decimal).
        lts.push((
            token.clone(),
            threshold_to_bp(&account.liquidation_threshold.value),
        ));
    }
    for (token, _, _) in &account.debts {
        debt_prices.push((token.clone(), asset_price.clone()));
    }
    // Ensure the referenced asset has a price too (BorrowAction.asset may
    // not be in collaterals/debts yet at the point HF is evaluated).
    if !collat_prices.iter().any(|(t, _)| t == referenced_asset) {
        collat_prices.push((referenced_asset.clone(), asset_price.clone()));
        lts.push((
            referenced_asset.clone(),
            threshold_to_bp(&account.liquidation_threshold.value),
        ));
    }
    if !debt_prices.iter().any(|(t, _)| t == referenced_asset) {
        debt_prices.push((referenced_asset.clone(), asset_price.clone()));
    }
    (collat_prices, debt_prices, lts)
}

/// Convert a `Decimal` `LiquidationThreshold` (stored as a 0.x fraction) into
/// basis points (`u32`). Best-effort string parse; returns `0` on failure
/// (safer for HF — under-counts collateral rather than over-counts).
fn threshold_to_bp(value: &policy_state::primitives::Decimal) -> u32 {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(value.as_str())
        .ok()
        .map(|d| (d * rust_decimal::Decimal::from(10_000_u32)).round())
        .and_then(|d| {
            use rust_decimal::prelude::ToPrimitive;
            d.to_u32()
        })
        .unwrap_or(0)
}
