//! V4Router inner action decoders, dispatched from `super::command_decode::v4_swap`
//! against `V4_ROUTER_TABLE`.
//!
//! Only the four swap actions emit `SwapAction` envelopes; settle/take/
//! delta-management actions are intentionally skipped today.

pub(super) mod exact_input;
pub(super) mod exact_input_single;
pub(super) mod exact_output;
pub(super) mod exact_output_single;

use abi_resolver::subdecode::opcode_stream::DecodedStep;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use policy_engine::action::Address;

use crate::AdapterError;

/// Extract the top-level `params` struct from a `DecodedStep`. All V4Router
/// swap actions follow the shape `params: (poolKey/currency*, ...)`.
pub(super) fn v4_params_tuple(step: &DecodedStep) -> Result<&[DynSolValue], AdapterError> {
    let args = step.args.as_ref().ok_or_else(|| {
        AdapterError::Invalid(format!("V4 action {} carried no decoded args", step.name))
    })?;
    let first = args.first().ok_or_else(|| {
        AdapterError::Invalid(format!("V4 action {} args empty", step.name))
    })?;
    match &first.value {
        DynSolValue::Tuple(fields) => Ok(fields),
        other => Err(AdapterError::Invalid(format!(
            "V4 action {} expected params tuple, got {other:?}",
            step.name
        ))),
    }
}

pub(super) fn tuple_address(value: &DynSolValue, field_name: &str) -> Result<Address, AdapterError> {
    use std::str::FromStr as _;
    match value {
        DynSolValue::Address(addr) => Address::from_str(&format!("0x{}", hex::encode(addr.0)))
            .map_err(|e| AdapterError::Invalid(format!("invalid {field_name} address: {e}"))),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} address, got {other:?}"
        ))),
    }
}

pub(super) fn tuple_bool(value: &DynSolValue, field_name: &str) -> Result<bool, AdapterError> {
    match value {
        DynSolValue::Bool(b) => Ok(*b),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} bool, got {other:?}"
        ))),
    }
}

pub(super) fn tuple_uint(value: &DynSolValue, field_name: &str) -> Result<U256, AdapterError> {
    match value {
        DynSolValue::Uint(u, _) => Ok(*u),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} uint, got {other:?}"
        ))),
    }
}

pub(super) fn extract_pool_fee_bps(
    pool_key_fields: &[DynSolValue],
) -> Result<Option<u32>, AdapterError> {
    let fee_value = pool_key_fields
        .get(2)
        .ok_or_else(|| AdapterError::Invalid("V4 poolKey missing fee".into()))?;
    match fee_value {
        DynSolValue::Uint(u, _) => Ok(Some(u32::try_from(*u).unwrap_or(0) / 100)),
        _ => Ok(None),
    }
}
