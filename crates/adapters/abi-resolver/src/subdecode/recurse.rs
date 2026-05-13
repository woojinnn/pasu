//! Recursive sub-call extraction (selector → child calldata).
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
use alloy_primitives::Address;

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

// ---------------------------------------------------------------------------
// Generalized recursion patterns — recognise selectors whose calldata wraps further sub-calls
// ---------------------------------------------------------------------------
//
// Beyond `multicall(bytes[])` self-call recursion, several router shapes wrap
// further calldata in different layouts:
//
// 1. **Named-target single-bytes** — one `bytes` argument that's calldata for
//    a *separate* `address` argument (Sushi `snwap.executorData`, 1inch
//    AggregationRouter executor pattern, …). Selector + (target_arg,
//    payload_arg) is enough to recurse with the right `to` address.
//
// 2. **Address-bytes tuple array** — `(address, bytes, …)[]` — Morpho
//    Bundler's `multicall` and similar batch executors. Each entry has its
//    own target, so the orchestrator recurses with each target/payload.
//
// We keep the selector → rule mapping as small `match` expressions for the
// few selectors we know; new ones can be added one line at a time.

/// Sushi Smart Router `snwap(address,uint256,address,address,uint256,address,bytes)`.
pub const SUSHI_SNWAP_SELECTOR: [u8; 4] = [0x5f, 0x3b, 0xd1, 0xc8];

/// Morpho Bundler `multicall((address,bytes,uint256,bool,bytes32)[])`.
pub const MORPHO_BUNDLER_MULTICALL_SELECTOR: [u8; 4] = [0x37, 0x4f, 0x43, 0x5d];

/// 1inch AggregationRouter V5
/// `swap(address executor, (address,address,address,address,uint256,uint256,uint256) desc, bytes permit, bytes data)`.
/// `executor` (arg 0) is the target the router `.call(data)` against;
/// `data` (arg 3) is the executor's calldata.
pub const ONEINCH_V5_SWAP_SELECTOR: [u8; 4] = [0x12, 0xaa, 0x3c, 0xaf];

/// CoW Protocol GPv2Settlement
/// `settle(IERC20[] tokens, uint256[] clearingPrices, GPv2Trade.Data[] trades,
///         GPv2Interaction.Data[][3] interactions)`.
/// `interactions` (arg 3) is a fixed-size group of three dynamic arrays
/// (pre-trade, intra-trade, post-trade). Each entry is
/// `(address target, uint256 value, bytes callData)` — every `callData`
/// is calldata for its `target`.
pub const COW_SETTLEMENT_SETTLE_SELECTOR: [u8; 4] = [0x13, 0xd7, 0x9a, 0x0b];

/// One sub-call extracted from a decoded outer call — `(target, calldata)`.
#[derive(Debug, Clone)]
pub struct ChildCall {
    /// Recipient of the inner call. For self-multicall this equals the parent
    /// target; for named-target / tuple-array shapes it's pulled out of the
    /// decoded arguments.
    pub target: Address,
    /// Raw calldata to feed back through the resolver.
    pub calldata: Vec<u8>,
}

/// Recursion rule — describes how to extract the children of a known
/// outer-call selector.
#[derive(Debug, Clone, Copy)]
pub enum RecurseRule {
    /// `bytes[]` arg N. Target is the parent's `to` address (self-call).
    SelfArrayBytes { array_arg: usize },
    /// `bytes` arg N is calldata for `address` arg M.
    NamedTarget {
        target_arg: usize,
        payload_arg: usize,
    },
    /// `(address, bytes, …)[]` arg N — each tuple has a per-entry target.
    AddressBytesTuples {
        array_arg: usize,
        target_field: usize,
        payload_field: usize,
    },
    /// `(address, …, bytes, …)[][N]` arg M — fixed-size `[N]` group of dynamic
    /// arrays of address-bytes tuples. CoW Settlement uses `[][3]`
    /// (pre/intra/post phases of `settle`).
    NestedAddressBytesTuples {
        array_arg: usize,
        target_field: usize,
        payload_field: usize,
    },
}

/// Look up the recursion rule for a selector. Returns `None` for selectors
/// we don't recognise; the orchestrator then leaves the children empty.
#[must_use]
pub fn lookup_recurse_rule(selector: &[u8; 4]) -> Option<RecurseRule> {
    match *selector {
        // Self-multicall variants — last arg is `bytes[]`.
        MULTICALL_BYTES_ARRAY | MULTICALL_UINT256_BYTES_ARRAY | MULTICALL_BYTES32_BYTES_ARRAY => {
            // `multicall(bytes[])`           → arg 0
            // `multicall(uint256, bytes[])`  → arg 1
            // `multicall(bytes32, bytes[])`  → arg 1
            // We can't distinguish the overload from selector alone here;
            // callers have historically just taken the last arg via
            // [`extract_subcalls`]. Encode that semantically: `array_arg`
            // is the LAST arg, signalled by `usize::MAX` and resolved by the
            // extractor below.
            Some(RecurseRule::SelfArrayBytes {
                array_arg: usize::MAX,
            })
        }
        SUSHI_SNWAP_SELECTOR => Some(RecurseRule::NamedTarget {
            target_arg: 5,
            payload_arg: 6,
        }),
        ONEINCH_V5_SWAP_SELECTOR => Some(RecurseRule::NamedTarget {
            target_arg: 0,
            payload_arg: 3,
        }),
        MORPHO_BUNDLER_MULTICALL_SELECTOR => Some(RecurseRule::AddressBytesTuples {
            array_arg: 0,
            target_field: 0,
            payload_field: 1,
        }),
        COW_SETTLEMENT_SETTLE_SELECTOR => Some(RecurseRule::NestedAddressBytesTuples {
            array_arg: 3,
            target_field: 0,
            payload_field: 2,
        }),
        _ => None,
    }
}

/// Apply `rule` to a decoded outer call and return the children to recurse
/// on. `parent_target` is used when the rule is `SelfArrayBytes` (multicall).
#[must_use]
pub fn extract_children(
    decoded: &DecodedCall,
    rule: RecurseRule,
    parent_target: Address,
) -> Option<Vec<ChildCall>> {
    match rule {
        RecurseRule::SelfArrayBytes { array_arg } => {
            let arg = if array_arg == usize::MAX {
                decoded.args.last()?
            } else {
                decoded.args.get(array_arg)?
            };
            let DynSolValue::Array(items) = &arg.value else {
                return None;
            };
            let mut out = Vec::with_capacity(items.len());
            for v in items {
                let DynSolValue::Bytes(b) = v else {
                    return None;
                };
                out.push(ChildCall {
                    target: parent_target,
                    calldata: b.clone(),
                });
            }
            Some(out)
        }
        RecurseRule::NamedTarget {
            target_arg,
            payload_arg,
        } => {
            let target = match decoded.args.get(target_arg)?.value {
                DynSolValue::Address(a) => a,
                _ => return None,
            };
            let payload = match &decoded.args.get(payload_arg)?.value {
                DynSolValue::Bytes(b) => b.clone(),
                _ => return None,
            };
            Some(vec![ChildCall {
                target,
                calldata: payload,
            }])
        }
        RecurseRule::AddressBytesTuples {
            array_arg,
            target_field,
            payload_field,
        } => {
            let arg = decoded.args.get(array_arg)?;
            let DynSolValue::Array(items) = &arg.value else {
                return None;
            };
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let DynSolValue::Tuple(fields) = item else {
                    return None;
                };
                let target = match fields.get(target_field)? {
                    DynSolValue::Address(a) => *a,
                    _ => return None,
                };
                let payload = match fields.get(payload_field)? {
                    DynSolValue::Bytes(b) => b.clone(),
                    _ => return None,
                };
                out.push(ChildCall {
                    target,
                    calldata: payload,
                });
            }
            Some(out)
        }
        RecurseRule::NestedAddressBytesTuples {
            array_arg,
            target_field,
            payload_field,
        } => {
            let arg = decoded.args.get(array_arg)?;
            let groups = match &arg.value {
                DynSolValue::FixedArray(g) | DynSolValue::Array(g) => g,
                _ => return None,
            };
            let mut out = Vec::new();
            for group in groups {
                let DynSolValue::Array(items) = group else {
                    return None;
                };
                for item in items {
                    let DynSolValue::Tuple(fields) = item else {
                        return None;
                    };
                    let target = match fields.get(target_field)? {
                        DynSolValue::Address(a) => *a,
                        _ => return None,
                    };
                    let payload = match fields.get(payload_field)? {
                        DynSolValue::Bytes(b) => b.clone(),
                        _ => return None,
                    };
                    out.push(ChildCall {
                        target,
                        calldata: payload,
                    });
                }
            }
            Some(out)
        }
    }
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

    #[test]
    fn cow_settlement_selector_maps_to_nested_rule() {
        match lookup_recurse_rule(&COW_SETTLEMENT_SETTLE_SELECTOR) {
            Some(RecurseRule::NestedAddressBytesTuples {
                array_arg: 3,
                target_field: 0,
                payload_field: 2,
            }) => {}
            other => panic!("unexpected rule: {other:?}"),
        }
    }

    #[test]
    fn nested_address_bytes_tuples_extracts_children_across_groups() {
        // Build a synthetic decoded call shaped like
        //   settle(_, _, _, interactions: (address,uint256,bytes)[][3])
        // with one interaction in group 0, none in group 1, two in group 2.
        let make_tuple = |target: Address, value: u64, data: Vec<u8>| -> DynSolValue {
            DynSolValue::Tuple(vec![
                DynSolValue::Address(target),
                DynSolValue::Uint(U256::from(value), 256),
                DynSolValue::Bytes(data),
            ])
        };
        let t1 = Address::repeat_byte(0x11);
        let t2 = Address::repeat_byte(0x22);
        let t3 = Address::repeat_byte(0x33);
        let group0 = DynSolValue::Array(vec![make_tuple(t1, 0, vec![0xaa])]);
        let group1 = DynSolValue::Array(vec![]);
        let group2 = DynSolValue::Array(vec![
            make_tuple(t2, 7, vec![0xbb, 0xcc]),
            make_tuple(t3, 0, vec![0xdd]),
        ]);
        let interactions = DynSolValue::FixedArray(vec![group0, group1, group2]);

        let decoded = DecodedCall {
            function_name: "settle".into(),
            signature: "settle(address[],uint256[],...)".into(),
            args: vec![
                DecodedArg {
                    name: "tokens".into(),
                    sol_type: "address[]".into(),
                    value: DynSolValue::Array(vec![]),
                    components: vec![],
                },
                DecodedArg {
                    name: "clearingPrices".into(),
                    sol_type: "uint256[]".into(),
                    value: DynSolValue::Array(vec![]),
                    components: vec![],
                },
                DecodedArg {
                    name: "trades".into(),
                    sol_type: "tuple[]".into(),
                    value: DynSolValue::Array(vec![]),
                    components: vec![],
                },
                DecodedArg {
                    name: "interactions".into(),
                    sol_type: "tuple[][3]".into(),
                    value: interactions,
                    components: vec![],
                },
            ],
        };

        let rule = lookup_recurse_rule(&COW_SETTLEMENT_SETTLE_SELECTOR).unwrap();
        let parent = Address::repeat_byte(0x99);
        let children = extract_children(&decoded, rule, parent).unwrap();
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].target, t1);
        assert_eq!(children[0].calldata, vec![0xaa]);
        assert_eq!(children[1].target, t2);
        assert_eq!(children[1].calldata, vec![0xbb, 0xcc]);
        assert_eq!(children[2].target, t3);
        assert_eq!(children[2].calldata, vec![0xdd]);
    }
}
