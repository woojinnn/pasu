//! Numeric type aliases.
//! - `U256`, `I256` are re-exported as-is from alloy-primitives.
//! - `Decimal` is used wherever fractional values are needed, such as prices,
//!   ratios, health factor, and leverage. It is currently represented as a
//!   string so that both uint256 precision and decimal fractions can be handled
//!   safely; it may later be swapped for something like `rust_decimal`.
//! - `Price` is a semantic alias (the underlying value is a `Decimal`).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub use alloy_primitives::{I256 as SignedI256, U128, U256};

/// Newtype for safely handling decimal notation; internally a decimal string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
pub struct Decimal(pub String);

impl Decimal {
    /// Creates a `Decimal` from any value convertible into a `String`.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Returns the `Decimal` representing zero (`"0"`).
    #[must_use]
    pub fn zero() -> Self {
        Self("0".into())
    }

    /// Borrows the underlying decimal string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Decimal {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for Decimal {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Price — alias for `Decimal`. The denomination must be specified separately.
pub type Price = Decimal;

/// Basis points (1 bp = 0.01%); `u32` is wide enough.
pub type BasisPoints = u32;

/// Unsigned ratio notation, e.g. for fee tiers.
pub type Weight = u32;
