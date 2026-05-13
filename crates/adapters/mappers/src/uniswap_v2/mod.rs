//! Uniswap V2 Router02 mappers — 9 swap functions.
//!
//! Address (mainnet): `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D`
//! All functions take `path: address[]` so `tokenIn = path[0]`,
//! `tokenOut = path[last]`. `feeBps` is hardcoded to 30 (V2 has a fixed
//! 0.30 % fee). Variants with `ETH` in the name imply `tokenIn` or
//! `tokenOut` is `AssetKind::Native`. `SupportingFeeOnTransferTokens` is
//! the FoT variant — same shape, only differs in router execution path.

pub mod common;
pub mod swap_eth_for_exact_tokens;
pub mod swap_exact_eth_for_tokens;
pub mod swap_exact_eth_for_tokens_supporting_fee_on_transfer_tokens;
pub mod swap_exact_tokens_for_eth;
pub mod swap_exact_tokens_for_eth_supporting_fee_on_transfer_tokens;
pub mod swap_exact_tokens_for_tokens;
pub mod swap_exact_tokens_for_tokens_supporting_fee_on_transfer_tokens;
pub mod swap_tokens_for_exact_eth;
pub mod swap_tokens_for_exact_tokens;
