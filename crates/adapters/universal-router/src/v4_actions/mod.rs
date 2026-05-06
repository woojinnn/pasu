//! Per-action decoders for the `V4_SWAP` command payload.

use crate::commands::{ActionMeta, RoutedAction};
use crate::common::{currency_to_policy_address, TokenLookup};
use alloy_primitives::aliases::U24;
use alloy_sol_types::sol;
use policy_engine::prelude::*;

pub(crate) mod exact_input;
pub(crate) mod exact_input_single;
pub(crate) mod exact_output;
pub(crate) mod exact_output_single;

sol! {
    struct PoolKey {
        address currency0;
        address currency1;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
    }

    struct PathKey {
        address intermediateCurrency;
        uint24 fee;
        int24 tickSpacing;
        address hooks;
        bytes hookData;
    }

    struct V4ExactInputSingleParams {
        PoolKey poolKey;
        bool zeroForOne;
        uint128 amountIn;
        uint128 amountOutMinimum;
        uint256 minHopPriceX36;
        bytes hookData;
    }

    struct V4ExactInputParams {
        address currencyIn;
        PathKey[] path;
        uint256[] minHopPriceX36;
        uint128 amountIn;
        uint128 amountOutMinimum;
    }

    struct V4ExactOutputSingleParams {
        PoolKey poolKey;
        bool zeroForOne;
        uint128 amountOut;
        uint128 amountInMaximum;
        uint256 minHopPriceX36;
        bytes hookData;
    }

    struct V4ExactOutputParams {
        address currencyOut;
        PathKey[] path;
        uint256[] minHopPriceX36;
        uint128 amountOut;
        uint128 amountInMaximum;
    }
}

pub(crate) const V4_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
pub(crate) const V4_SWAP_EXACT_IN: u8 = 0x07;
pub(crate) const V4_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
pub(crate) const V4_SWAP_EXACT_OUT: u8 = 0x09;
pub(crate) const V4_SETTLE: u8 = 0x0b;
pub(crate) const V4_SETTLE_ALL: u8 = 0x0c;
pub(crate) const V4_SETTLE_PAIR: u8 = 0x0d;
pub(crate) const V4_TAKE: u8 = 0x0e;
pub(crate) const V4_TAKE_ALL: u8 = 0x0f;
pub(crate) const V4_TAKE_PORTION: u8 = 0x10;
pub(crate) const V4_TAKE_PAIR: u8 = 0x11;
pub(crate) const V4_CLOSE_CURRENCY: u8 = 0x12;
pub(crate) const V4_CLEAR_OR_TAKE: u8 = 0x13;
pub(crate) const V4_SWEEP: u8 = 0x14;
pub(crate) const V4_WRAP: u8 = 0x15;
pub(crate) const V4_UNWRAP: u8 = 0x16;

pub(crate) const MAX_V4_ACTIONS: usize = 64;

pub(crate) fn decode_exact_input(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    exact_input::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_exact_input_single(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    exact_input_single::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_exact_output(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    exact_output::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_exact_output_single(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    exact_output_single::decode(tx, tokens, input, meta)
}

pub(crate) fn pool_key_tokens(
    chain_id: ChainId,
    tokens: &TokenLookup,
    pool_key: &PoolKey,
    zero_for_one: bool,
) -> (Token, Token) {
    let currency0 = currency_to_policy_address(pool_key.currency0);
    let currency1 = currency_to_policy_address(pool_key.currency1);
    if zero_for_one {
        (
            tokens.get(chain_id, &currency0),
            tokens.get(chain_id, &currency1),
        )
    } else {
        (
            tokens.get(chain_id, &currency1),
            tokens.get(chain_id, &currency0),
        )
    }
}

pub(crate) fn v4_fee_bips(fee: U24) -> Option<u32> {
    let fee = u32_from_u24(fee);
    if fee & 0x0080_0000 != 0 {
        None
    } else {
        Some(fee / 100)
    }
}

pub(crate) fn v4_fee_bips_avg(fees: &[u32]) -> Option<u32> {
    if fees.is_empty() || fees.iter().any(|fee| fee & 0x0080_0000 != 0) {
        None
    } else {
        let len = u32::try_from(fees.len()).ok()?;
        Some(fees.iter().sum::<u32>() / len / 100)
    }
}

pub(crate) fn u32_from_u24(value: U24) -> u32 {
    value.to::<u32>()
}
