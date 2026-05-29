//! `Balance` enum ↔ (form, amount) 2 컬럼.
//!
//! * `Fungible { amount }` → `("fungible", Some("123456"))`
//! * `Owned`             → `("owned", None)`

use alloy_primitives::U256;

use simulation_state::token::Balance;

use crate::error::{DbError, DbResult};

/// SQL row 의 balance 2 컬럼.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceColumns {
    pub form: &'static str,
    /// U256 decimal string. `form = "owned"` 면 None.
    pub amount: Option<String>,
}

#[must_use]
pub fn encode_balance(b: &Balance) -> BalanceColumns {
    match b {
        Balance::Fungible { amount } => BalanceColumns {
            form: "fungible",
            amount: Some(amount.to_string()),
        },
        Balance::Owned => BalanceColumns {
            form: "owned",
            amount: None,
        },
    }
}

pub fn decode_balance(c: &BalanceColumns) -> DbResult<Balance> {
    match c.form {
        "fungible" => {
            let s = c
                .amount
                .as_deref()
                .ok_or_else(|| DbError::Invariant("fungible without amount".into()))?;
            let amt = U256::from_str_radix(s, 10)
                .map_err(|e| DbError::Invariant(format!("bad balance amount: {e}")))?;
            Ok(Balance::Fungible { amount: amt })
        }
        "owned" => Ok(Balance::Owned),
        other => Err(DbError::Invariant(format!("unknown balance form: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_fungible() {
        let b = Balance::Fungible {
            amount: U256::from(1_000_000u64),
        };
        let c = encode_balance(&b);
        assert_eq!(c.form, "fungible");
        assert_eq!(c.amount.as_deref(), Some("1000000"));
        let back = decode_balance(&c).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn round_trip_owned() {
        let b = Balance::Owned;
        let c = encode_balance(&b);
        assert_eq!(c.form, "owned");
        assert!(c.amount.is_none());
        let back = decode_balance(&c).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn round_trip_huge_amount() {
        let b = Balance::Fungible { amount: U256::MAX };
        let c = encode_balance(&b);
        let back = decode_balance(&c).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn fungible_without_amount_errors() {
        let c = BalanceColumns {
            form: "fungible",
            amount: None,
        };
        let err = decode_balance(&c).unwrap_err();
        assert!(format!("{err}").contains("fungible without amount"));
    }

    #[test]
    fn unknown_form_errors() {
        let c = BalanceColumns {
            form: "weird",
            amount: None,
        };
        let err = decode_balance(&c).unwrap_err();
        assert!(format!("{err}").contains("unknown balance form"));
    }
}
