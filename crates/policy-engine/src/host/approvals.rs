//! Approval capability — current allowance lookup for `(owner, token, spender)`.
//!
//! The lookup shape is explicitly `(owner, token, spender)` to match ERC-20
//! approval semantics and to keep policy decisions aligned to a single caller
//! authority.

use crate::core::{Address, AmountSpec, Token};
use alloy_primitives::U256;
use std::collections::HashMap;
use thiserror::Error;

/// Approval lookup failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApprovalsError {
    /// No allowance was recorded for the owner/token/spender tuple.
    #[error("no allowance record for owner {owner} on token {token} to {spender}")]
    NoRecord {
        /// Owner address.
        owner: String,
        /// Chain-qualified token key.
        token: String,
        /// Spender address.
        spender: String,
    },
}

/// Host approvals capability.
pub trait Approvals: Send + Sync {
    /// Return the current allowance for the owner/token/spender tuple.
    ///
    /// # Errors
    ///
    /// Returns an error when no allowance record is available.
    fn allowance(
        &self,
        owner: &Address,
        token: &Token,
        spender: &Address,
    ) -> Result<AmountSpec, ApprovalsError>;
}

/// In-memory approvals implementation for tests and demos.
#[derive(Debug, Clone, Default)]
pub struct MockApprovals {
    allowances: HashMap<String, AmountSpec>,
}

impl MockApprovals {
    /// Construct an empty mock approvals provider.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key_for_approvals(owner: &Address, token: &Token, spender: &Address) -> String {
        format!("{}/{}/{}", owner.as_str(), token.key(), spender.as_str())
    }

    /// Insert an allowance and return the updated mock.
    #[must_use]
    pub fn with_allowance(
        mut self,
        owner: &Address,
        token: &Token,
        spender: &Address,
        raw: U256,
    ) -> Self {
        let amount = AmountSpec::from_raw(token.clone(), raw);
        self.allowances
            .insert(Self::key_for_approvals(owner, token, spender), amount);
        self
    }
}

impl Approvals for MockApprovals {
    fn allowance(
        &self,
        owner: &Address,
        token: &Token,
        spender: &Address,
    ) -> Result<AmountSpec, ApprovalsError> {
        let key = Self::key_for_approvals(owner, token, spender);
        self.allowances
            .get(&key)
            .cloned()
            .ok_or_else(|| ApprovalsError::NoRecord {
                owner: owner.as_str().to_string(),
                token: token.key(),
                spender: spender.as_str().to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> Address {
        Address::new("0x1111111111111111111111111111111111111111").unwrap()
    }

    fn spender() -> Address {
        Address::new("0x2222222222222222222222222222222222222222").unwrap()
    }

    fn usdt() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        }
    }

    fn usdc() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        }
    }

    #[test]
    fn mock_returns_recorded_allowance() {
        let a =
            MockApprovals::new().with_allowance(&owner(), &usdt(), &spender(), U256::from(7u64));
        let got = a.allowance(&owner(), &usdt(), &spender()).unwrap();
        assert_eq!(got.raw, "7");
        assert_eq!(got.token, usdt());
    }

    #[test]
    fn mock_missing_allowance_errors() {
        let a =
            MockApprovals::new().with_allowance(&owner(), &usdt(), &spender(), U256::from(7u64));
        let err = a.allowance(&owner(), &usdc(), &spender()).unwrap_err();
        assert_eq!(
            err,
            ApprovalsError::NoRecord {
                owner: owner().as_str().to_string(),
                token: usdc().key(),
                spender: spender().as_str().to_string(),
            }
        );
    }

    #[test]
    fn mock_keys_are_chain_and_spender_qualified() {
        let usdt_other_chain = Token {
            chain_id: 137,
            address: usdt().address,
            symbol: usdt().symbol,
            decimals: usdt().decimals,
            is_native: false,
        };
        let a =
            MockApprovals::new().with_allowance(&owner(), &usdt(), &spender(), U256::from(1u64));
        assert!(a.allowance(&owner(), &usdt(), &spender()).is_ok());
        assert!(a
            .allowance(&owner(), &usdt_other_chain, &spender())
            .is_err());
    }
}
