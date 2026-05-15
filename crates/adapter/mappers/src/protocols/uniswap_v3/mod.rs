//! Mappers for Uniswap V3 SwapRouter (exactInput*/exactOutput*).
//!
//! Note: `exactInputSingle` and `exactInput` share the same upstream
//! `decoder_id` (`UNISWAP_V3_DECODER_ID`), so the registered mapper for that
//! decoder must dispatch on `function_signature`. That umbrella implementation
//! lives in `exact_input_single` (as `UniswapV3Mapper`). The `exactOutput*`
//! family each has its own decoder_id and per-function mapper.

mod common;
pub mod exact_input_single;
pub mod exact_output;
pub mod exact_output_single;

pub use exact_input_single::{UniswapV3Mapper, UNISWAP_V3_MAPPER_ID};
pub use exact_output::{UniswapV3ExactOutputMapper, EXACT_OUTPUT_MAPPER_ID};
pub use exact_output_single::{UniswapV3ExactOutputSingleMapper, EXACT_OUTPUT_SINGLE_MAPPER_ID};

#[cfg(test)]
mod tests;
