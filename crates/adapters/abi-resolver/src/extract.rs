//! Ergonomic extraction over `DynSolValue` + `Param.components`.
//!
//! `DecodedArg` carries a `DynSolValue` (positional) and a parallel
//! `Vec<Param>` (named, recursive). Combining the two lets callers navigate
//! decoded calldata by Solidity field name without the need for compile-time
//! `sol!` types.
//!
//! Mappers consume this in place of `<Foo>Call::abi_decode(...)`:
//!
//! ```ignore
//! let call: DecodedCall = ...;
//! let params = call.arg("params")?;          // tuple-typed arg
//! let path        = params.field("path")?.as_bytes()?;
//! let recipient   = params.field("recipient")?.as_address()?;
//! let amount_in   = params.field("amountIn")?.as_uint()?;
//! ```
//!
//! Sourcify keeps parameter names on tuple components, so by-name access is
//! reliable for top-level Solidity functions. The openchain fallback strips
//! names (args become `arg0..argN`), but mappers should only run after
//! Sourcify has matched the contract anyway.

use alloy_dyn_abi::DynSolValue;
use alloy_json_abi::Param;
use alloy_primitives::{Address, U256};

use crate::decode::DecodedCall;

/// Errors from `NamedValue::*` extraction.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("arg `{0}` not found on decoded call")]
    ArgNotFound(String),
    #[error("arg index {0} out of bounds (have {1})")]
    ArgIndexOutOfBounds(usize, usize),
    #[error("field `{field}` not found on `{parent}` (available: {available:?})")]
    FieldNotFound {
        parent: String,
        field: String,
        available: Vec<String>,
    },
    #[error("type mismatch on `{path}`: expected {expected}, got {got}")]
    TypeMismatch {
        path: String,
        expected: &'static str,
        got: &'static str,
    },
}

/// A decoded value paired with its parameter descriptor.
///
/// `components` mirrors `Param.components` for the value's parent param.
/// For a tuple, it lists the field params; for an array of tuples, the same
/// slice describes each element (alloy doesn't nest descriptors under array
/// layers — see `decode::DecodedArg`).
#[derive(Debug, Clone)]
pub struct NamedValue<'a> {
    pub path: String,
    pub value: &'a DynSolValue,
    pub components: &'a [Param],
}

impl<'a> NamedValue<'a> {
    /// Navigate to a tuple field by name. Requires `value` to be `Tuple(_)`
    /// and the parent descriptor to list `name` among its components.
    pub fn field(&self, name: &str) -> Result<NamedValue<'a>, ExtractError> {
        let items = self.as_tuple_items()?;
        let idx = self
            .components
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| ExtractError::FieldNotFound {
                parent: self.path.clone(),
                field: name.to_string(),
                available: self.components.iter().map(|p| p.name.clone()).collect(),
            })?;
        let value = items.get(idx).ok_or_else(|| ExtractError::FieldNotFound {
            parent: self.path.clone(),
            field: name.to_string(),
            available: self.components.iter().map(|p| p.name.clone()).collect(),
        })?;
        let component = &self.components[idx];
        Ok(NamedValue {
            path: format!("{}.{}", self.path, name),
            value,
            components: component.components.as_slice(),
        })
    }

    /// Underlying tuple items. Useful when iterating struct-like values.
    pub fn as_tuple_items(&self) -> Result<&'a [DynSolValue], ExtractError> {
        match self.value {
            DynSolValue::Tuple(items) => Ok(items.as_slice()),
            other => Err(self.mismatch("tuple", other)),
        }
    }

    pub fn as_address(&self) -> Result<Address, ExtractError> {
        match self.value {
            DynSolValue::Address(a) => Ok(*a),
            other => Err(self.mismatch("address", other)),
        }
    }

    pub fn as_uint(&self) -> Result<U256, ExtractError> {
        match self.value {
            DynSolValue::Uint(u, _bits) => Ok(*u),
            other => Err(self.mismatch("uint", other)),
        }
    }

    pub fn as_bool(&self) -> Result<bool, ExtractError> {
        match self.value {
            DynSolValue::Bool(b) => Ok(*b),
            other => Err(self.mismatch("bool", other)),
        }
    }

    pub fn as_bytes(&self) -> Result<&'a [u8], ExtractError> {
        match self.value {
            DynSolValue::Bytes(b) => Ok(b.as_slice()),
            other => Err(self.mismatch("bytes", other)),
        }
    }

    /// Element list of a dynamic or fixed array. Each element shares this
    /// value's `components` (alloy doesn't nest descriptors under array layers).
    pub fn as_array(&self) -> Result<Vec<NamedValue<'a>>, ExtractError> {
        let items = match self.value {
            DynSolValue::Array(v) | DynSolValue::FixedArray(v) => v.as_slice(),
            other => return Err(self.mismatch("array", other)),
        };
        Ok(items
            .iter()
            .enumerate()
            .map(|(i, v)| NamedValue {
                path: format!("{}[{}]", self.path, i),
                value: v,
                components: self.components,
            })
            .collect())
    }

    /// `address[]` convenience.
    pub fn as_address_array(&self) -> Result<Vec<Address>, ExtractError> {
        let items = match self.value {
            DynSolValue::Array(v) | DynSolValue::FixedArray(v) => v,
            other => return Err(self.mismatch("address[]", other)),
        };
        items
            .iter()
            .enumerate()
            .map(|(i, v)| match v {
                DynSolValue::Address(a) => Ok(*a),
                other => Err(ExtractError::TypeMismatch {
                    path: format!("{}[{}]", self.path, i),
                    expected: "address",
                    got: kind_name(other),
                }),
            })
            .collect()
    }

    fn mismatch(&self, expected: &'static str, got: &DynSolValue) -> ExtractError {
        ExtractError::TypeMismatch {
            path: self.path.clone(),
            expected,
            got: kind_name(got),
        }
    }
}

/// Lookup by-name / by-index on the decoded call's top-level args.
impl DecodedCall {
    pub fn arg(&self, name: &str) -> Result<NamedValue<'_>, ExtractError> {
        let arg = self
            .args
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| ExtractError::ArgNotFound(name.to_string()))?;
        Ok(NamedValue {
            path: format!("{}.{}", self.function_name, arg.name),
            value: &arg.value,
            components: arg.components.as_slice(),
        })
    }

    pub fn arg_at(&self, index: usize) -> Result<NamedValue<'_>, ExtractError> {
        let arg = self
            .args
            .get(index)
            .ok_or(ExtractError::ArgIndexOutOfBounds(index, self.args.len()))?;
        Ok(NamedValue {
            path: format!("{}.{}", self.function_name, arg.name),
            value: &arg.value,
            components: arg.components.as_slice(),
        })
    }
}

fn kind_name(v: &DynSolValue) -> &'static str {
    match v {
        DynSolValue::Address(_) => "address",
        DynSolValue::Bool(_) => "bool",
        DynSolValue::Bytes(_) => "bytes",
        DynSolValue::FixedBytes(_, _) => "fixed_bytes",
        DynSolValue::Int(_, _) => "int",
        DynSolValue::Uint(_, _) => "uint",
        DynSolValue::String(_) => "string",
        DynSolValue::Array(_) => "array",
        DynSolValue::FixedArray(_) => "fixed_array",
        DynSolValue::Tuple(_) => "tuple",
        DynSolValue::Function(_) => "function",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_with_function;
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    fn approve_function() -> Function {
        let abi_json = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        });
        serde_json::from_value(abi_json).unwrap()
    }

    fn approve_calldata(spender: [u8; 20], amount: u128) -> Vec<u8> {
        let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
        let mut spender_word = [0u8; 32];
        spender_word[12..].copy_from_slice(&spender);
        data.extend_from_slice(&spender_word);
        data.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
        data
    }

    #[test]
    fn arg_by_name_extracts_address_and_uint() {
        let calldata = approve_calldata([0x42; 20], 12345);
        let decoded = decode_with_function(&approve_function(), &calldata).unwrap();
        let spender = decoded.arg("spender").unwrap().as_address().unwrap();
        let amount = decoded.arg("amount").unwrap().as_uint().unwrap();
        assert_eq!(spender, Address::from([0x42; 20]));
        assert_eq!(amount, U256::from(12345u64));
    }

    #[test]
    fn arg_not_found_returns_error() {
        let calldata = approve_calldata([0x42; 20], 1);
        let decoded = decode_with_function(&approve_function(), &calldata).unwrap();
        let err = decoded.arg("nonexistent").unwrap_err();
        assert!(matches!(err, ExtractError::ArgNotFound(_)));
    }

    #[test]
    fn type_mismatch_returns_error() {
        let calldata = approve_calldata([0x42; 20], 1);
        let decoded = decode_with_function(&approve_function(), &calldata).unwrap();
        let err = decoded.arg("spender").unwrap().as_uint().unwrap_err();
        assert!(matches!(err, ExtractError::TypeMismatch { .. }));
    }

    /// Verifies tuple-field-by-name navigation by hand-constructing a DecodedCall.
    /// Avoids the alloy encode/decode round-trip whose head/tail layout we
    /// don't need to exercise here.
    #[test]
    fn nested_tuple_field_navigation() {
        use crate::decode::{DecodedArg, DecodedCall};
        let path_bytes = vec![0xaa, 0xbb, 0xcc];
        let recipient = Address::from([0x11; 20]);
        let amount_in = U256::from(1_000_000u64);
        let amount_out_min = U256::from(950_000u64);

        let params_components: Vec<Param> = serde_json::from_value(serde_json::json!([
            { "name": "path",             "type": "bytes",   "internalType": "bytes",   "components": [] },
            { "name": "recipient",        "type": "address", "internalType": "address", "components": [] },
            { "name": "deadline",         "type": "uint256", "internalType": "uint256", "components": [] },
            { "name": "amountIn",         "type": "uint256", "internalType": "uint256", "components": [] },
            { "name": "amountOutMinimum", "type": "uint256", "internalType": "uint256", "components": [] }
        ]))
        .unwrap();

        let tuple = DynSolValue::Tuple(vec![
            DynSolValue::Bytes(path_bytes.clone()),
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(1_700_000_000u64), 256),
            DynSolValue::Uint(amount_in, 256),
            DynSolValue::Uint(amount_out_min, 256),
        ]);

        let call = DecodedCall {
            function_name: "exactInput".into(),
            signature: "exactInput((bytes,address,uint256,uint256,uint256))".into(),
            args: vec![DecodedArg {
                name: "params".into(),
                sol_type: "tuple".into(),
                value: tuple,
                components: params_components,
            }],
        };

        let params = call.arg("params").unwrap();
        assert_eq!(
            params.field("path").unwrap().as_bytes().unwrap(),
            path_bytes.as_slice()
        );
        assert_eq!(
            params.field("recipient").unwrap().as_address().unwrap(),
            recipient
        );
        assert_eq!(
            params.field("amountIn").unwrap().as_uint().unwrap(),
            amount_in
        );
        assert_eq!(
            params.field("amountOutMinimum").unwrap().as_uint().unwrap(),
            amount_out_min
        );
        // Path error contains the navigation chain.
        let err = params.field("missing").unwrap_err();
        match err {
            ExtractError::FieldNotFound { parent, field, .. } => {
                assert_eq!(parent, "exactInput.params");
                assert_eq!(field, "missing");
            }
            other => panic!("expected FieldNotFound, got {other:?}"),
        }
    }
}
