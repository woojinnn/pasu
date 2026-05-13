//! Uniswap V3 SwapRouter mappers — 4 functions.
//!
//! Address (mainnet): `0xE592427A0AEce92De3Edee1F18E0157C05861564`
//! V3 uses an encoded `bytes path` (token0, fee, token1, fee, token2, …) for
//! multi-hop variants. `feeBps` per hop = `fee_tier / 100` (V3 stores fee as
//! ppm, so 3000 ppm = 30 bps). Single-hop variants take `fee` directly.
//!
//! `SwapRouter02` shares the same selectors with `deadline` field removed —
//! same mapping logic, slight signature diff (handled via shared helper).

pub mod common;
pub mod exact_input;
pub mod exact_input_single;
pub mod exact_output;
pub mod exact_output_single;
