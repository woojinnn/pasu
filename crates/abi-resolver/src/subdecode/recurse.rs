//! Cat A — recursive sub-call extraction.
//!
//! Several router patterns wrap an array of further calldata in a single
//! `bytes[]` argument and execute each entry as a self-call. Examples:
//!
//! - Uniswap V3 SwapRouter `multicall(bytes[])` (`0xac9650d8`)
//! - SwapRouter02 `multicall(uint256, bytes[])` (`0x5ae401dc`)
//! - SwapRouter02 `multicall(bytes32, bytes[])` (`0x1f0464d1`)
//! - PancakeSwap SmartRouter shares the three selectors above
//!
//! All three send each `data[i]` back through the same router (`to == self`),
//! so the orchestrator's recursion target is the parent's `to` address.
//!
//! This module identifies those wrappers and pulls out the inner calldata
//! payloads as raw `Vec<u8>`. The orchestrator (typically a web handler) then
//! invokes [`crate::resolver::Resolver::resolve`] for each payload.

use alloy_dyn_abi::DynSolValue;

use crate::decode::DecodedCall;

/// `multicall(bytes[])` — Uniswap V3 SwapRouter, V3 NPM, Pancake V3 router,
/// Pancake SmartRouter.
pub const MULTICALL_BYTES_ARRAY: [u8; 4] = [0xac, 0x96, 0x50, 0xd8];

/// `multicall(uint256, bytes[])` — SwapRouter02 deadline overload.
pub const MULTICALL_UINT256_BYTES_ARRAY: [u8; 4] = [0x5a, 0xe4, 0x01, 0xdc];

/// `multicall(bytes32, bytes[])` — SwapRouter02 previousBlockhash overload.
pub const MULTICALL_BYTES32_BYTES_ARRAY: [u8; 4] = [0x1f, 0x04, 0x64, 0xd1];

/// True when the selector corresponds to one of the recognised self-call
/// multicall patterns.
#[must_use]
pub fn is_self_multicall(selector: &[u8; 4]) -> bool {
    matches!(
        *selector,
        MULTICALL_BYTES_ARRAY | MULTICALL_UINT256_BYTES_ARRAY | MULTICALL_BYTES32_BYTES_ARRAY
    )
}

/// Pull the inner `bytes[]` argument from a decoded multicall.
///
/// Returns `None` when the structural shape doesn't match — i.e. the last
/// argument isn't `bytes[]` or any element isn't `bytes`. Caller should already
/// have verified the selector via [`is_self_multicall`] before calling this.
#[must_use]
pub fn extract_subcalls(decoded: &DecodedCall) -> Option<Vec<Vec<u8>>> {
    let last = decoded.args.last()?;
    let DynSolValue::Array(items) = &last.value else {
        return None;
    };
    let mut subcalls = Vec::with_capacity(items.len());
    for v in items {
        let DynSolValue::Bytes(b) = v else {
            return None;
        };
        subcalls.push(b.clone());
    }
    Some(subcalls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::DecodedArg;
    use alloy_primitives::U256;

    fn fake_decoded_multicall(items: Vec<DynSolValue>) -> DecodedCall {
        DecodedCall {
            function_name: "multicall".into(),
            signature: "multicall(bytes[])".into(),
            args: vec![DecodedArg {
                name: "data".into(),
                sol_type: "bytes[]".into(),
                value: DynSolValue::Array(items),
                components: vec![],
            }],
        }
    }

    #[test]
    fn selectors_recognised() {
        assert!(is_self_multicall(&MULTICALL_BYTES_ARRAY));
        assert!(is_self_multicall(&MULTICALL_UINT256_BYTES_ARRAY));
        assert!(is_self_multicall(&MULTICALL_BYTES32_BYTES_ARRAY));
        assert!(!is_self_multicall(&[0x00, 0x00, 0x00, 0x00]));
    }

    #[test]
    fn extract_pulls_bytes_array() {
        let decoded = fake_decoded_multicall(vec![
            DynSolValue::Bytes(vec![0x01, 0x02]),
            DynSolValue::Bytes(vec![0xab, 0xcd, 0xef]),
        ]);
        let calls = extract_subcalls(&decoded).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], vec![0x01, 0x02]);
        assert_eq!(calls[1], vec![0xab, 0xcd, 0xef]);
    }

    #[test]
    fn extract_returns_none_when_last_arg_is_not_array() {
        let decoded = DecodedCall {
            function_name: "approve".into(),
            signature: "approve(address,uint256)".into(),
            args: vec![DecodedArg {
                name: "amount".into(),
                sol_type: "uint256".into(),
                value: DynSolValue::Uint(U256::from(1u64), 256),
                components: vec![],
            }],
        };
        assert!(extract_subcalls(&decoded).is_none());
    }

    #[test]
    fn extract_returns_none_when_array_element_not_bytes() {
        let decoded = fake_decoded_multicall(vec![DynSolValue::Uint(U256::from(1u64), 256)]);
        assert!(extract_subcalls(&decoded).is_none());
    }

    #[test]
    fn extract_handles_empty_array() {
        let decoded = fake_decoded_multicall(vec![]);
        let calls = extract_subcalls(&decoded).unwrap();
        assert!(calls.is_empty());
    }
}
