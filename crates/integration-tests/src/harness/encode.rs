//! Calldata encoding helpers + seed mixing.
//!
//! Mirrors `encode_calldata` from
//! `crates/policy-engine-wasm/tests/declarative_v3_route.rs:35` so the harness
//! produces byte-identical calldata to the route tests.

use alloy_dyn_abi::DynSolValue;

/// Selector (`0x` + 8 hex) + ABI-encoded params → `0x`-prefixed calldata hex.
///
/// The params are wrapped in a tuple and `abi_encode_params`-encoded, matching
/// Solidity's external-call ABI (head/tail encoding of the argument list).
#[must_use]
pub fn encode_calldata(selector: &str, args: &[DynSolValue]) -> String {
    let sel = hex::decode(selector.trim_start_matches("0x")).unwrap_or_default();
    let body = DynSolValue::Tuple(args.to_vec()).abi_encode_params();
    format!("0x{}{}", hex::encode(sel), hex::encode(body))
}

/// FNV-1a 64-bit hash of a string — used to derive a position-stable fuzz seed
/// from a callkey (`seed = fnv1a64(callkey) ^ global_seed ^ iteration`).
#[must_use]
pub fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::{encode_calldata, fnv1a64};
    use alloy_dyn_abi::DynSolValue;
    use alloy_primitives::{Address, U256};

    #[test]
    fn erc20_approve_calldata_shape() {
        // approve(address,uint256) selector 0x095ea7b3
        let cd = encode_calldata(
            "0x095ea7b3",
            &[
                DynSolValue::Address(Address::ZERO),
                DynSolValue::Uint(U256::from(1u64), 256),
            ],
        );
        // 0x + 8 selector + 2 * 64 hex words.
        assert_eq!(cd.len(), 2 + 8 + 64 * 2);
        assert!(cd.starts_with("0x095ea7b3"));
    }

    #[test]
    fn fnv_is_stable_and_distinct() {
        assert_eq!(fnv1a64("abc"), fnv1a64("abc"));
        assert_ne!(fnv1a64("abc"), fnv1a64("abd"));
    }
}
