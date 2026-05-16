//! Public trait surface implemented by adapter authors.

use crate::action::ActionEnvelope;
use crate::ctx::{CallCtx, SignCtx};
use crate::error::AdapterError;
use crate::sign::SignRequest;
use crate::types::DecodedCall;

pub trait Decoder {
    fn decode_call(&self, ctx: &CallCtx, calldata: &[u8])
        -> Result<DecodedCall, AdapterError>;
}

pub trait CallAdapter: Decoder {
    fn map_to_action(&self, ctx: &CallCtx, decoded: &DecodedCall)
        -> Result<Vec<ActionEnvelope>, AdapterError>;
}

pub trait SignAdapter {
    fn decode_sign(&self, ctx: &SignCtx, req: &SignRequest)
        -> Result<Vec<ActionEnvelope>, AdapterError>;
}
