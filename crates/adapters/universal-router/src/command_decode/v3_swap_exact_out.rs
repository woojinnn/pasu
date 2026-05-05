//! Universal Router command `V3_SWAP_EXACT_OUT`.

use crate::commands::{
    decode_v3_path, fee_bips_avg, map_recipient, swap_action, token, ActionMeta, RoutedAction,
};
use crate::common::TokenLookup;
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;

type Input = sol! { (address, uint256, uint256, bytes, bool, uint256[]) };

pub(crate) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    let (recipient, amount_out, amount_in_max, path, _payer_is_user, _min_hop) =
        Input::abi_decode_sequence(input, true)
            .map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
    let (path_tokens, fees) = decode_v3_path(path.as_ref())?;
    let token_out = token(tokens, tx.chain_id, *path_tokens.first().unwrap());
    let token_in = token(tokens, tx.chain_id, *path_tokens.last().unwrap());
    Ok(RoutedAction {
        action: swap_action(
            tx,
            "uniswap-v3",
            token_in,
            token_out,
            amount_in_max,
            amount_out,
            map_recipient(tx, recipient),
            fee_bips_avg(&fees),
        ),
        meta,
    })
}
