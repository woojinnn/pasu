//! Oracle abstraction + valuation source contract for lowering.
//!  
//! Lowering asks the oracle for USD valuation before policy checks run, then
//! threads that USD shape through `AmountSpec.usd`.
//!  
//! `AmountSpec.usd.value` is embedded as a decimal extension, and the
//! associated `stale_sec` is preserved for policy-level freshness checks.
//!  
//! Implementations should return valuation data with a clear staleness policy;
//! callers can test for freshness via policy `context has \"usd\"` checks or
//! compare the staleness value directly when needed.
//!  
//! v0.1 keeps this synchronous and mockable; in-process tests and playgrounds
//! use `MockOracle`, while real integrations are expected to provide their own
//! transport-backed implementations.

use crate::core::{Token, UsdValuation};
use std::collections::HashMap;
use thiserror::Error;

/// Oracle lookup failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OracleError {
    /// No valuation was available for the token.
    #[error("no price data for token {0}")]
    NoPrice(String),
    /// Valuation existed but exceeded the configured freshness bound.
    #[error("price for {token} is stale ({stale_sec}s old, max {max_sec})")]
    Stale {
        /// Token key that was stale.
        token: String,
        /// Observed staleness in seconds.
        stale_sec: u64,
        /// Maximum accepted staleness in seconds.
        max_sec: u64,
    },
}

/// Host oracle capability.
pub trait Oracle: Send + Sync {
    /// Returns USD valuation for one unit (1 whole token, decimals applied) of
    /// the given token, or an error if not available.
    ///
    /// # Errors
    ///
    /// Returns an error when valuation data is unavailable or stale.
    fn price(&self, token: &Token) -> Result<UsdValuation, OracleError>;
}

/// Test/playground oracle. Constructed by `MockOracle::new()` and populated
/// via `with_price`.
#[derive(Debug, Clone, Default)]
pub struct MockOracle {
    prices: HashMap<String, UsdValuation>,
}

impl MockOracle {
    /// Construct an empty mock oracle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key_for_oracle(token: &Token) -> String {
        token.key()
    }

    /// Insert a price for a token. The key is the token's chain-qualified key.
    #[must_use]
    pub fn with_price(mut self, token: &Token, valuation: UsdValuation) -> Self {
        self.prices.insert(Self::key_for_oracle(token), valuation);
        self
    }

    /// Convenience: insert a fixed-source `chainlink` price with given staleness.
    #[must_use]
    pub fn with_simple_price(self, token: &Token, usd: &str, stale_sec: u64) -> Self {
        let v = UsdValuation {
            value: usd.into(),
            as_of_ts: 0,
            sources: vec!["mock-chainlink".into()],
            stale_sec,
        };
        self.with_price(token, v)
    }
}

impl Oracle for MockOracle {
    fn price(&self, token: &Token) -> Result<UsdValuation, OracleError> {
        self.prices
            .get(&Self::key_for_oracle(token))
            .cloned()
            .ok_or_else(|| OracleError::NoPrice(Self::key_for_oracle(token)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Address;

    fn usdt() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        }
    }

    fn weth() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        }
    }

    #[test]
    fn mock_returns_recorded_price() {
        let oracle = MockOracle::new().with_simple_price(&usdt(), "1.00", 5);
        let v = oracle.price(&usdt()).unwrap();
        assert_eq!(v.value, "1.00");
        assert_eq!(v.stale_sec, 5);
        assert_eq!(v.sources, vec!["mock-chainlink".to_string()]);
    }

    #[test]
    fn mock_errors_on_unknown_token() {
        let oracle = MockOracle::new();
        let err = oracle.price(&usdt()).unwrap_err();
        assert!(matches!(err, OracleError::NoPrice(_)));
    }

    #[test]
    fn mock_independent_per_token() {
        let oracle = MockOracle::new()
            .with_simple_price(&usdt(), "1.00", 5)
            .with_simple_price(&weth(), "3000.00", 10);

        assert_eq!(oracle.price(&usdt()).unwrap().value, "1.00");
        assert_eq!(oracle.price(&weth()).unwrap().value, "3000.00");
    }

    #[test]
    fn mock_keys_by_chain_id() {
        let mut other_chain = usdt();
        other_chain.chain_id = 137;
        let oracle = MockOracle::new().with_simple_price(&usdt(), "1.00", 5);
        // mainnet USDT has price; polygon-keyed USDT does not
        assert!(oracle.price(&usdt()).is_ok());
        assert!(oracle.price(&other_chain).is_err());
    }
}
