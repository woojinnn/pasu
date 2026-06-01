//! 모든 다른 모듈이 의존하는 기본 타입들.

/// EVM 주소 alias + 정규화 helper.
pub mod address;
/// 체인 식별자 (CAIP-2) 와 블록 높이.
pub mod chain;
/// 숫자 타입 (`Decimal`, `Price`, `U256` 등).
pub mod decimal;
/// 가벼운 식별자 (`ProtocolRef`, `PoolRef`, `VenueRef`, `MarketRef`).
pub mod refs;
/// 시각 / 기간 (`Time`, `Duration`).
pub mod time;

pub use address::{Address, Spender};
pub use chain::{BlockHeight, ChainId};
pub use decimal::{BasisPoints, Decimal, Price, SignedI256, Weight, U128, U256};
pub use refs::{MarketRef, PoolRef, ProtocolRef, VenueRef};
pub use time::{Duration, Time};
