//! V4 action `SWAP_EXACT_IN`.

use crate::universal_router::commands::{swap_action, ActionMeta, RoutedAction};
use super::super::common::{currency_to_policy_address, TokenLookup};
use crate::universal_router::v4_actions::{u32_from_u24, v4_fee_bips_avg, V4ExactInputParams};
use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use policy_engine::prelude::*;

pub(super) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, ActionAdapterError> {
    let p = <V4ExactInputParams as SolValue>::abi_decode_sequence(input, true)
        .map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
    if p.path.is_empty() {
        return Err(ActionAdapterError::BadCalldata(
            "v4 exact-in path is empty".into(),
        ));
    }
    let token_in_addr = currency_to_policy_address(p.currencyIn);
    let token_out_addr = currency_to_policy_address(p.path[p.path.len() - 1].intermediateCurrency);
    let fees = p
        .path
        .iter()
        .map(|k| u32_from_u24(k.fee))
        .collect::<Vec<_>>();
    let action = swap_action(
        tx,
        "uniswap-v4",
        tokens.get(tx.chain_id, &token_in_addr),
        tokens.get(tx.chain_id, &token_out_addr),
        U256::from(p.amountIn),
        U256::from(p.amountOutMinimum),
        &tx.from,
        v4_fee_bips_avg(&fees),
        &meta,
    );
    Ok(RoutedAction { action, meta })
}
