//! Outer ABI decoding for Universal Router `execute(...)` overloads.
//!
//! UR ships two overloads:
//!   - `execute(bytes commands, bytes[] inputs)` (no deadline)
//!   - `execute(bytes commands, bytes[] inputs, uint256 deadline)`
//!
//! `decode_outer_call` returns `(commands, inputs, optional_validity)` so
//! the opcode dispatcher in `super::commands` can iterate without going
//! through a `DecodedCall`.

use abi_resolver::subdecode::protocols::universal_router::{
    EXECUTE_DEADLINE_SELECTOR, EXECUTE_SELECTOR,
};
use alloy_sol_types::{sol, SolCall};
use policy_engine::action::{Validity, ValiditySource};

use crate::AdapterError;

use super::common::decimal;

// Outer-call decoders for the two Uniswap Universal Router `execute`
// overloads. We use `sol!` inline so this CallAdapter doesn't depend on a
// per-function `Decoder` struct.
sol! {
    #[allow(clippy::too_many_arguments)]
    function execute(bytes commands, bytes[] inputs);
    #[allow(clippy::too_many_arguments)]
    function executeWithDeadline(
        bytes commands,
        bytes[] inputs,
        uint256 deadline,
    );
}

pub(super) fn decode_outer_call(
    calldata: &[u8],
) -> Result<(Vec<u8>, Vec<Vec<u8>>, Option<Validity>), AdapterError> {
    let selector: [u8; 4] = calldata
        .get(..4)
        .ok_or_else(|| AdapterError::Invalid("UR calldata shorter than selector".into()))?
        .try_into()
        .expect("slice length checked");

    // The two real on-chain Solidity functions are both named `execute` (an
    // overload pair distinguished by parameter count). Rust can't host two
    // `executeCall` types in one module, so the deadline overload is renamed
    // to `executeWithDeadline` for the `sol!` macro. That makes
    // `executeWithDeadlineCall::SELECTOR` ≠ the on-chain `0x3593564c`, so we
    // skip the macro's strict selector check by feeding the post-selector
    // payload directly into `abi_decode_raw`.
    let payload = &calldata[4..];
    match selector {
        EXECUTE_SELECTOR => {
            let call = executeCall::abi_decode_raw(payload, true).map_err(|e| {
                AdapterError::Invalid(format!("UR execute ABI decode failed: {e}"))
            })?;
            let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
            Ok((call.commands.to_vec(), inputs, None))
        }
        EXECUTE_DEADLINE_SELECTOR => {
            let call = executeWithDeadlineCall::abi_decode_raw(payload, true).map_err(|e| {
                AdapterError::Invalid(format!(
                    "UR executeWithDeadline ABI decode failed: {e}"
                ))
            })?;
            let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
            let validity = Some(Validity {
                expires_at: decimal(&call.deadline.to_string())?,
                source: ValiditySource::TxDeadline,
            });
            Ok((call.commands.to_vec(), inputs, validity))
        }
        _ => Err(AdapterError::Invalid(format!(
            "unrecognised UR selector 0x{}",
            hex::encode(selector)
        ))),
    }
}
