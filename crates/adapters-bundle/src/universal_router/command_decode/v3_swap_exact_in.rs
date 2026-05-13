//! Universal Router command `V3_SWAP_EXACT_IN`.

use super::super::common::TokenLookup;
use crate::universal_router::commands::{
    decode_v3_path, fee_bips_avg, map_recipient, path_endpoints, swap_action, token, ActionMeta,
    RoutedAction,
};
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;

type Input = sol! { (address, uint256, uint256, bytes, bool, uint256[]) };

pub(super) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, ActionAdapterError> {
    let (recipient, amount_in, amount_out_min, path, _payer_is_user, _min_hop) =
        Input::abi_decode_sequence(input, true)
            .map_err(|e| ActionAdapterError::BadCalldata(e.to_string()))?;
    let (path_tokens, fees) = decode_v3_path(path.as_ref())?;
    let (token_in_addr, token_out_addr) = path_endpoints(&path_tokens, "v3")?;
    let token_in = token(tokens, tx.chain_id, token_in_addr);
    let token_out = token(tokens, tx.chain_id, token_out_addr);
    let recipient = map_recipient(tx, recipient);
    let action = swap_action(
        tx,
        "uniswap-v3",
        token_in,
        token_out,
        amount_in,
        amount_out_min,
        &recipient,
        fee_bips_avg(&fees),
        &meta,
    );
    Ok(RoutedAction { action, meta })
}
