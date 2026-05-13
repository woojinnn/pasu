//! Uniswap V2 Router02 swap adapters for `policy-engine`.
//!
//! Module layout mirrors the V3 crate: one file per Solidity swap function,
//! with shared helpers in `common.rs`. Each function module ships its `sol!`
//! declaration, encode/decode pair, `Adapter_` impl, and unit tests.
//!
//! ```text
//!   src/
//!     ├── lib.rs                              ← module index + re-exports
//!     ├── common.rs                           ← shared: ROUTER, TokenLookup,
//!     │                                         shift_decimals, NATIVE_TOKEN
//!     ├── swap_exact_tokens_for_tokens.rs
//!     ├── swap_tokens_for_exact_tokens.rs
//!     ├── swap_exact_eth_for_tokens.rs        (payable)
//!     ├── swap_eth_for_exact_tokens.rs        (payable)
//!     ├── swap_exact_tokens_for_eth.rs
//!     └── swap_tokens_for_exact_eth.rs
//! ```


pub mod common;
pub mod swap_eth_for_exact_tokens;
pub mod swap_exact_eth_for_tokens;
pub mod swap_exact_tokens_for_eth;
pub mod swap_exact_tokens_for_tokens;
pub mod swap_tokens_for_exact_eth;
pub mod swap_tokens_for_exact_tokens;

pub use common::{
    native_eth, shift_decimals, DecodeError, TokenLookup, NATIVE_ETH_SENTINEL,
    UNISWAP_V2_ROUTER_MAINNET,
};

// ---- Per-function re-exports ----------------------------------------------

pub use swap_exact_tokens_for_tokens::Adapter_ as UniswapV2SwapExactTokensForTokensAdapter;
pub use swap_exact_tokens_for_tokens::{
    decode as decode_swap_exact_tokens_for_tokens, encode as encode_swap_exact_tokens_for_tokens,
    Params as SwapExactTokensForTokensParams, SELECTOR as SELECTOR_SWAP_EXACT_TOKENS_FOR_TOKENS,
};

pub use swap_tokens_for_exact_tokens::Adapter_ as UniswapV2SwapTokensForExactTokensAdapter;
pub use swap_tokens_for_exact_tokens::{
    decode as decode_swap_tokens_for_exact_tokens, encode as encode_swap_tokens_for_exact_tokens,
    Params as SwapTokensForExactTokensParams, SELECTOR as SELECTOR_SWAP_TOKENS_FOR_EXACT_TOKENS,
};

pub use swap_exact_eth_for_tokens::Adapter_ as UniswapV2SwapExactETHForTokensAdapter;
pub use swap_exact_eth_for_tokens::{
    decode as decode_swap_exact_eth_for_tokens, encode as encode_swap_exact_eth_for_tokens,
    Params as SwapExactETHForTokensParams, SELECTOR as SELECTOR_SWAP_EXACT_ETH_FOR_TOKENS,
};

pub use swap_eth_for_exact_tokens::Adapter_ as UniswapV2SwapETHForExactTokensAdapter;
pub use swap_eth_for_exact_tokens::{
    decode as decode_swap_eth_for_exact_tokens, encode as encode_swap_eth_for_exact_tokens,
    Params as SwapETHForExactTokensParams, SELECTOR as SELECTOR_SWAP_ETH_FOR_EXACT_TOKENS,
};

pub use swap_exact_tokens_for_eth::Adapter_ as UniswapV2SwapExactTokensForETHAdapter;
pub use swap_exact_tokens_for_eth::{
    decode as decode_swap_exact_tokens_for_eth, encode as encode_swap_exact_tokens_for_eth,
    Params as SwapExactTokensForETHParams, SELECTOR as SELECTOR_SWAP_EXACT_TOKENS_FOR_ETH,
};

pub use swap_tokens_for_exact_eth::Adapter_ as UniswapV2SwapTokensForExactETHAdapter;
pub use swap_tokens_for_exact_eth::{
    decode as decode_swap_tokens_for_exact_eth, encode as encode_swap_tokens_for_exact_eth,
    Params as SwapTokensForExactETHParams, SELECTOR as SELECTOR_SWAP_TOKENS_FOR_EXACT_ETH,
};
