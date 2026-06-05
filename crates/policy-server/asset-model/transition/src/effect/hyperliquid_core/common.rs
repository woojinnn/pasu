//! Shared helpers for the Hyperliquid CORE reducers.

use std::str::FromStr;

use rust_decimal::Decimal as RustDecimal;

use policy_state::live_field::DataSource;
use policy_state::position::{HlAccount, Position, PositionKind};
use policy_state::primitives::{Decimal, ProtocolRef, Time};
use policy_state::wallet::WalletState;

use crate::error::{ReducerError, ReducerResult};

/// The single stable position id for a wallet's Hyperliquid account.
pub(super) const HL_ACCOUNT_ID: &str = "hyperliquid/account";

/// `ProtocolRef` for the off-chain Hyperliquid venue (no chain).
pub(super) fn hl_protocol_ref() -> ProtocolRef {
    ProtocolRef::new("hyperliquid")
}

/// Wrap an `HlAccount` in a full `Position` envelope ready for `Open`.
/// `now` comes from `EvalContext::now` (NOT a system clock) — preserves purity.
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
pub(super) fn find_hl_account(state: &WalletState) -> Option<HlAccount> {
    state.positions.iter().find_map(|p| match &p.kind {
        PositionKind::HyperliquidAccount(a) if p.id == HL_ACCOUNT_ID => Some(a.clone()),
        _ => None,
    })
}

/// `lhs - rhs` on two state-side decimals via `rust_decimal`. Errors on parse
/// failure or when the result is negative (an underflow the caller must handle).
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

/// Parse a state-side decimal and require it be non-negative, returning the
/// normalized value. Errors `Invariant` on parse failure or a negative value.
/// Used to validate outflow amounts (a negative withdrawal/transfer is a fault;
/// the reducer records only clean effects — fail-closed).
pub(super) fn decimal_nonneg(d: &Decimal) -> ReducerResult<Decimal> {
    let v = RustDecimal::from_str(d.as_str())
        .map_err(|e| ReducerError::Invariant(format!("HL decimal parse {:?}: {e}", d.as_str())))?;
    if v.is_sign_negative() {
        return Err(ReducerError::Invariant(format!(
            "HL amount must be non-negative, got {d}"
        )));
    }
    Ok(Decimal::new(v.normalize().to_string()))
}

/// `lhs + rhs` on two state-side decimals via `rust_decimal`.
/// Errors on parse failure of either operand.
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

    #[test]
    fn decimal_nonneg_normalizes_and_rejects_negative() {
        assert_eq!(
            decimal_nonneg(&Decimal::new("100.50")).unwrap(),
            Decimal::new("100.5")
        );
        assert_eq!(
            decimal_nonneg(&Decimal::new("0")).unwrap(),
            Decimal::new("0")
        );
        assert!(matches!(
            decimal_nonneg(&Decimal::new("-5")).unwrap_err(),
            ReducerError::Invariant(_)
        ));
    }
}
