//! V4 action `SWAP_EXACT_IN_SINGLE`.

use crate::universal_router::commands::{swap_action, ActionMeta, RoutedAction};
use super::super::common::TokenLookup;
use crate::universal_router::v4_actions::{pool_key_tokens, v4_fee_bips, V4ExactInputSingleParams};
use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use policy_engine::prelude::*;

pub(super) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, ActionAdapterError> {
    let p = <V4ExactInputSingleParams as SolValue>::abi_decode_sequence(input, true)
        .map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
    let (token_in, token_out) = pool_key_tokens(tx.chain_id, tokens, &p.poolKey, p.zeroForOne);
    let action = swap_action(
        tx,
        "uniswap-v4",
        token_in,
        token_out,
        U256::from(p.amountIn),
        U256::from(p.amountOutMinimum),
        &tx.from,
        v4_fee_bips(p.poolKey.fee),
        &meta,
    );
    Ok(RoutedAction { action, meta })
}
