//! 숫자 타입 alias.
//!
//! - `U256`, `I256` 는 alloy-primitives 그대로.
//! - `Decimal` 은 가격/비율/HF/leverage 등 소수가 필요한 곳에 사용. 현재는 문자열로
//!   표현 (uint256 의 정밀도와 소수 양쪽을 안전하게 다루기 위해). 후속에서
//!   `rust_decimal` 등으로 교체 가능.
//! - `Price` 는 의미적 alias (값 자체는 Decimal).

use serde::{Deserialize, Serialize};

pub use alloy_primitives::{I256 as SignedI256, U128, U256};

/// 소수 표기를 안전하게 다루기 위한 newtype. 내부는 decimal-문자열.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Decimal(pub String);

impl Decimal {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn zero() -> Self {
        Self("0".into())
    }

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

/// 가격 — Decimal alias. denom 은 별도로 명시 필요.
pub type Price = Decimal;

/// basis point (1bp = 0.01%). u32 면 충분.
pub type BasisPoints = u32;

/// fee tier 등 부호 없는 비율 표기.
pub type Weight = u32;
