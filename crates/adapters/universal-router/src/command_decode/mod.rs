//! Per-command decoders for Universal Router command bytes.

use crate::commands::{ActionMeta, RoutedAction};
use crate::common::TokenLookup;
use policy_engine::prelude::*;

pub(crate) mod v2_swap_exact_in;
pub(crate) mod v2_swap_exact_out;
pub(crate) mod v3_swap_exact_in;
pub(crate) mod v3_swap_exact_out;
pub(crate) mod v4_swap;

pub(crate) fn decode_v2_swap_exact_in(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    v2_swap_exact_in::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_v2_swap_exact_out(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    v2_swap_exact_out::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_v3_swap_exact_in(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    v3_swap_exact_in::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_v3_swap_exact_out(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: ActionMeta,
) -> Result<RoutedAction, AdapterError> {
    v3_swap_exact_out::decode(tx, tokens, input, meta)
}

pub(crate) fn decode_v4_swap(
    tx: &TransactionRequest,
    tokens: &TokenLookup,
    input: &[u8],
    meta: &ActionMeta,
) -> Result<Vec<RoutedAction>, AdapterError> {
    v4_swap::decode(tx, tokens, input, meta)
}
