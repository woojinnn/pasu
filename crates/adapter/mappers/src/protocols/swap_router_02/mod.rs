//! Mappers for Uniswap SwapRouter02.
//!
//! Mirror the legacy mappers under `crates/mappers/src/swap_router_02/` in
//! the backup branch, ported onto the new `Mapper` trait. SR02 calls have NO
//! `deadline` parameter — deadline is enforced via an outer `Multicall.multicall`
//! wrapper. We surface `validity = None` here; callers / policy authors can
//! still gate on Multicall's deadline via a separate mapper.

mod common;
pub mod exact_input;
pub mod exact_input_single;
pub mod exact_output;
pub mod exact_output_single;

pub use exact_input::Sr02ExactInputMapper;
pub use exact_input_single::Sr02ExactInputSingleMapper;
pub use exact_output::Sr02ExactOutputMapper;
pub use exact_output_single::Sr02ExactOutputSingleMapper;
