//! Canonical `decoder_id` strings and 4-byte selectors used by the new-pipeline
//! `Mapper` registry as join keys.
//!
//! With the option-A architecture, no per-function `Decoder` structs exist
//! anymore — the Sourcify-backed `Resolver` decodes calldata and the
//! `bridge::convert_legacy_call` step assigns one of these `decoder_id`
//! strings to the result based on its selector. The mapper registry is then
//! keyed by `decoder_id`.

// ── ERC-20 ────────────────────────────────────────────────────────────────────
pub const ERC20_APPROVE_DECODER_ID: &str = "erc20/approve";
pub const ERC20_TRANSFER_DECODER_ID: &str = "erc20/transfer";
pub const ERC20_TRANSFER_FROM_DECODER_ID: &str = "erc20/transferFrom";
pub const SET_APPROVAL_FOR_ALL_DECODER_ID: &str = "erc/setApprovalForAll";

pub const APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];
pub const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
pub const TRANSFER_FROM_SELECTOR: [u8; 4] = [0x23, 0xb8, 0x72, 0xdd];
pub const SET_APPROVAL_FOR_ALL_SELECTOR: [u8; 4] = [0xa2, 0x2c, 0xb4, 0x65];

// ── Uniswap V2 Router02 ──────────────────────────────────────────────────────
pub const UNISWAP_V2_ROUTER_MAINNET: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID: &str = "uniswap-v2/swapExactTokensForTokens";
pub const SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID: &str = "uniswap-v2/swapTokensForExactTokens";
pub const SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID: &str = "uniswap-v2/swapExactETHForTokens";
pub const SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID: &str = "uniswap-v2/swapTokensForExactETH";
pub const SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID: &str = "uniswap-v2/swapExactTokensForETH";
pub const SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID: &str = "uniswap-v2/swapETHForExactTokens";

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR: [u8; 4] = [0x38, 0xed, 0x17, 0x39];
pub const SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR: [u8; 4] = [0x88, 0x03, 0xdb, 0xee];
pub const SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5];
pub const SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR: [u8; 4] = [0x4a, 0x25, 0xd9, 0x4a];
pub const SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5];
pub const SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR: [u8; 4] = [0xfb, 0x3b, 0xdb, 0x41];

// Fee-on-transfer variants (V2 only — exact-IN only; no exact-OUT variants).
pub const SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID: &str =
    "uniswap-v2/swapExactTokensForTokensSupportingFeeOnTransferTokens";
pub const SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID: &str =
    "uniswap-v2/swapExactETHForTokensSupportingFeeOnTransferTokens";
pub const SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID: &str =
    "uniswap-v2/swapExactTokensForETHSupportingFeeOnTransferTokens";

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_SELECTOR: [u8; 4] = [0x5c, 0x11, 0xd7, 0x95];
pub const SWAP_EXACT_ETH_FOR_TOKENS_FOT_SELECTOR: [u8; 4] = [0xb6, 0xf9, 0xde, 0x95];
pub const SWAP_EXACT_TOKENS_FOR_ETH_FOT_SELECTOR: [u8; 4] = [0x79, 0x1a, 0xc9, 0x47];

// ── Uniswap V3 SwapRouter ─────────────────────────────────────────────────────
pub const SWAP_ROUTER_MAINNET: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

/// Shared decoder_id used by `UniswapV3Mapper` for both `exactInputSingle` and
/// `exactInput`. The dedicated *Output variants below carry their own ids.
pub const UNISWAP_V3_DECODER_ID: &str = "uniswap_v3";
pub const EXACT_OUTPUT_SINGLE_DECODER_ID: &str = "uniswap-v3/exactOutputSingle";
pub const EXACT_OUTPUT_DECODER_ID: &str = "uniswap-v3/exactOutput";

pub const EXACT_INPUT_SINGLE_SELECTOR: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];
pub const EXACT_INPUT_SELECTOR: [u8; 4] = [0xc0, 0x4b, 0x8d, 0x59];
pub const EXACT_OUTPUT_SINGLE_SELECTOR: [u8; 4] = [0xdb, 0x3e, 0x21, 0x98];
pub const EXACT_OUTPUT_SELECTOR: [u8; 4] = [0xf2, 0x8c, 0x04, 0x98];

// ── Uniswap SwapRouter02 ─────────────────────────────────────────────────────
pub const SWAP_ROUTER_02_MAINNET: &str = "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45";

pub const SR02_EXACT_INPUT_SINGLE_DECODER_ID: &str = "swap-router-02/exactInputSingle";
pub const SR02_EXACT_INPUT_DECODER_ID: &str = "swap-router-02/exactInput";
pub const SR02_EXACT_OUTPUT_SINGLE_DECODER_ID: &str = "swap-router-02/exactOutputSingle";
pub const SR02_EXACT_OUTPUT_DECODER_ID: &str = "swap-router-02/exactOutput";

pub const SR02_EXACT_INPUT_SINGLE_SELECTOR: [u8; 4] = [0x04, 0xe4, 0x5a, 0xaf];
pub const SR02_EXACT_INPUT_SELECTOR: [u8; 4] = [0xb8, 0x58, 0x18, 0x3f];
pub const SR02_EXACT_OUTPUT_SINGLE_SELECTOR: [u8; 4] = [0x50, 0x23, 0xb4, 0xdf];
pub const SR02_EXACT_OUTPUT_SELECTOR: [u8; 4] = [0x09, 0xb8, 0x13, 0x46];

// ── WETH9 ─────────────────────────────────────────────────────────────────────
pub const WETH_DEPOSIT_DECODER_ID: &str = "weth/deposit";
pub const WETH_WITHDRAW_DECODER_ID: &str = "weth/withdraw";

pub const WETH_DEPOSIT_SELECTOR: [u8; 4] = [0xd0, 0xe3, 0x0d, 0xb0];
pub const WETH_WITHDRAW_SELECTOR: [u8; 4] = [0x2e, 0x1a, 0x7d, 0x4d];
