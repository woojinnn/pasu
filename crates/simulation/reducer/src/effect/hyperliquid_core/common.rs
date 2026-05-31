//! Shared helpers for the Hyperliquid CORE reducers.

use std::str::FromStr;

use rust_decimal::Decimal as RustDecimal;

use simulation_state::live_field::DataSource;
use simulation_state::position::{HlAccount, Position, PositionKind};
use simulation_state::primitives::{Decimal, ProtocolRef, Time};
use simulation_state::wallet::WalletState;

use crate::error::{ReducerError, ReducerResult};

/// The single stable position id for a wallet's Hyperliquid account.
pub(super) const HL_ACCOUNT_ID: &str = "hyperliquid/account";

/// `ProtocolRef` for the off-chain Hyperliquid venue (no chain).
pub(super) fn hl_protocol_ref() -> ProtocolRef {
    ProtocolRef::new("hyperliquid")
}

/// Wrap an `HlAccount` in a full `Position` envelope ready for `Open`.
/// `now` comes from `EvalContext::now` (NOT a system clock) — preserves purity.
// removed when task 5 wires it
#[allow(dead_code)]
pub(super) fn hl_position(acct: HlAccount, now: Time) -> Position {
    Position {
        id: HL_ACCOUNT_ID.to_owned(),
        protocol: hl_protocol_ref(),
        chain: None,
        kind: PositionKind::HyperliquidAccount(acct),
        primitives_synced_at: now,
        primitives_source: DataSource::UserSupplied,
    }
}

/// Find the wallet's existing `HlAccount` in base state (clone), if present.
// removed when task 5 wires it
#[allow(dead_code)]
pub(super) fn find_hl_account(state: &WalletState) -> Option<HlAccount> {
    state.positions.iter().find_map(|p| match &p.kind {
        PositionKind::HyperliquidAccount(a) if p.id == HL_ACCOUNT_ID => Some(a.clone()),
        _ => None,
    })
}

/// `lhs - rhs` on two state-side decimals via `rust_decimal`. Errors on parse
/// failure or when the result is negative (an underflow the caller must handle).
// removed when task 7 wires it
#[allow(dead_code)]
pub(super) fn decimal_sub_nonneg(lhs: &Decimal, rhs: &Decimal) -> ReducerResult<Decimal> {
    let l = RustDecimal::from_str(lhs.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("HL decimal parse {:?}: {e}", lhs.as_str()))
    })?;
    let r = RustDecimal::from_str(rhs.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("HL decimal parse {:?}: {e}", rhs.as_str()))
    })?;
    let out = l - r;
    if out.is_sign_negative() {
        return Err(ReducerError::Invariant(format!(
            "HL balance underflow: {lhs} - {rhs} < 0"
        )));
    }
    Ok(Decimal::new(out.normalize().to_string()))
}

/// `lhs + rhs` on two state-side decimals via `rust_decimal`.
/// Errors on parse failure of either operand.
// removed when task 7 wires it
#[allow(dead_code)]
pub(super) fn decimal_add(lhs: &Decimal, rhs: &Decimal) -> ReducerResult<Decimal> {
    let l = RustDecimal::from_str(lhs.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("HL decimal parse {:?}: {e}", lhs.as_str()))
    })?;
    let r = RustDecimal::from_str(rhs.as_str()).map_err(|e| {
        ReducerError::Invariant(format!("HL decimal parse {:?}: {e}", rhs.as_str()))
    })?;
    Ok(Decimal::new((l + r).normalize().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_sub_basic() {
        assert_eq!(
            decimal_sub_nonneg(&Decimal::new("1000.5"), &Decimal::new("0.5")).unwrap(),
            Decimal::new("1000")
        );
    }

    #[test]
    fn decimal_sub_underflow_errs() {
        let err = decimal_sub_nonneg(&Decimal::new("10"), &Decimal::new("11")).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn decimal_add_basic() {
        assert_eq!(
            decimal_add(&Decimal::new("100"), &Decimal::new("0.25")).unwrap(),
            Decimal::new("100.25")
        );
    }
}
