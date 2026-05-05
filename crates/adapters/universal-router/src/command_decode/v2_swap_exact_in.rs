//! Universal Router command `V2_SWAP_EXACT_IN`.

use crate::commands::{map_recipient, swap_action, token, ActionMeta, RoutedAction};
use crate::common::TokenLookup;
use alloy_sol_types::{sol, SolType};
use policy_engine::prelude::*;

type Input = sol! { (address, uint256, uint256, address[], bool, uint256[]) };

pub(crate) fn decode(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    let (recipient, amount_in, amount_out_min, path, _payer_is_user, _min_hop) =
        Input::abi_decode_sequence(input, true)
            .map_err(|e| AdapterError::BadCalldata(e.to_string()))?;
    if path.len() < 2 {
        return Err(AdapterError::BadCalldata(
            "v2 path must contain at least 2 tokens".into(),
        ));
    }
    let token_in = token(tokens, tx.chain_id, *path.first().unwrap());
    let token_out = token(tokens, tx.chain_id, *path.last().unwrap());
    Ok(RoutedAction {
        action: swap_action(
            tx,
            "uniswap-v2",
            token_in,
            token_out,
            amount_in,
            amount_out_min,
            map_recipient(tx, recipient),
            Some(30),
        ),
        meta,
    })
}
