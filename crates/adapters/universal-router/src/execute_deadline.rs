//! Universal Router `execute(bytes,bytes[],uint256)`.

use crate::execute::{DecodeError, Params};
use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall};

sol! {
    function execute(bytes commands, bytes[] inputs, uint256 deadline) external payable;
}

/// Selector for `execute(bytes,bytes[],uint256)`.
pub const SELECTOR_EXECUTE_DEADLINE: [u8; 4] = executeCall::SELECTOR;

/// ABI-encode `execute(bytes,bytes[],uint256)` calldata.
#[must_use]
pub fn encode_execute_deadline(commands: Vec<u8>, inputs: Vec<Vec<u8>>, deadline: U256) -> Vec<u8> {
    executeCall {
        commands: commands.into(),
        inputs: inputs.into_iter().map(Into::into).collect(),
        deadline,
    }
    .abi_encode()
}

/// Decode `execute(bytes,bytes[],uint256)` calldata.
///
/// # Errors
///
/// Returns an error when calldata is too short, has the wrong selector, or
/// fails ABI decoding.
pub fn decode(calldata: &[u8]) -> Result<Params, DecodeError> {
    if calldata.len() < 4 {
        return Err(DecodeError::TooShort {
            need: 4,
            got: calldata.len(),
        });
    }

    let selector = [calldata[0], calldata[1], calldata[2], calldata[3]];
    if selector != SELECTOR_EXECUTE_DEADLINE {
        return Err(DecodeError::BadSelector {
            got: hex::encode(selector),
            want: hex::encode(SELECTOR_EXECUTE_DEADLINE),
        });
    }

    // Non-strict (validate=false) — see comment in execute.rs::decode.
    // Wallet-produced calldata isn't always byte-canonical, but the
    // decoded fields are what we use for policy evaluation.
    let call = executeCall::abi_decode(calldata, false)
        .map_err(|e| DecodeError::AbiDecode(e.to_string()))?;
    Ok(Params {
        commands: call.commands.to_vec(),
        inputs: call.inputs.into_iter().map(|b| b.to_vec()).collect(),
        deadline: Some(call.deadline),
    })
}
