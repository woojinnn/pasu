//! Foundational primitive types (addresses, chains, decimals, refs, time) that all other modules depend on.

pub mod address;
/// Chain and block identifiers: CAIP-2 `ChainId` and `BlockHeight`.
pub mod chain;
pub mod decimal;
pub mod refs;
pub mod time;

pub use address::{Address, Spender};
pub use chain::{BlockHeight, ChainId};
pub use decimal::{BasisPoints, Decimal, Price, SignedI256, Weight, U128, U256};
pub use refs::{MarketRef, PoolRef, ProtocolRef, VenueRef};
pub use time::{Duration, Time};
