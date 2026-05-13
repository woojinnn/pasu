//! Pancake Infinity action table (opcode-dispatched, nested under
//! Pancake UR's `INFI_SWAP` opcode `0x10`).
//!
//! When Pancake UR executes opcode `0x10 INFI_SWAP`, the inner `inputs[i]`
//! is a `(bytes actions, bytes[] params)` pair driven by Pancake Infinity
//! periphery's [`Actions.sol`] library. This module provides the action
//! table — both CL (concentrated-liquidity) and Bin pool flavours, plus
//! the shared settlement opcodes.
//!
//! Source verified against:
//! - `pancakeswap/infinity-periphery @ main` →
//!   `src/libraries/Actions.sol` (opcode constants 0x00–0x20)
//! - `src/InfinityRouter.sol::_handleAction` (input ABI shapes for swap
//!   and settlement opcodes)
//! - `src/interfaces/IInfinityRouter.sol`,
//!   `src/pool-cl/interfaces/ICLRouterBase.sol`,
//!   `src/pool-bin/interfaces/IBinRouterBase.sol` (parameter structs)
//! - `pancakeswap/infinity-core @ main` →
//!   `src/types/PoolKey.sol` (PoolKey struct — note: 6 fields, not the
//!   5-field V4 PoolKey)
//! - `pancakeswap/infinity-periphery/src/libraries/PathKey.sol` (PathKey
//!   struct — 6 fields, includes `IPoolManager poolManager` and
//!   `bytes32 parameters`)
//!
//! Liquidity actions (CL_INCREASE_LIQUIDITY etc., 0x00-0x05; BIN_*,
//! 0x19-0x1b) are PositionManager-only and are kept label-only here. Their
//! schemas live in PositionManager source and can be added when the
//! INFI_CL_POSITION_CALL / INFI_BIN_POSITION_CALL recursion lands.
//!
//! [`Actions.sol`]: https://github.com/pancakeswap/infinity-periphery/blob/main/src/libraries/Actions.sol

use alloy_dyn_abi::DynSolValue;

use crate::subdecode::opcode_stream::{DecodedStep, OpcodeEntry, OpcodeTable};

// ---------------------------------------------------------------------------
// JSON-ABI literals — Pancake Infinity router swap params.
// ---------------------------------------------------------------------------
//
// Note: Pancake Infinity PoolKey is **6 fields** (currency0, currency1,
// hooks, poolManager, fee, parameters), DIFFERENT from Uniswap V4 PoolKey
// (5 fields, no poolManager/parameters). PathKey similarly has 6 fields
// including hookData and parameters.

const POOL_KEY_INFI_JSON: &str = r#"
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "hooks",       "type": "address" },
            { "name": "poolManager", "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "parameters",  "type": "bytes32" }
        ]}
"#;

const PATH_KEY_INFI_TUPLE: &str = r#"
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "hooks",                "type": "address" },
            { "name": "poolManager",          "type": "address" },
            { "name": "hookData",             "type": "bytes" },
            { "name": "parameters",           "type": "bytes32" }
"#;

// CL_SWAP_EXACT_IN_SINGLE — `CLSwapExactInputSingleParams` =
// `(PoolKey poolKey, bool zeroForOne, uint128 amountIn, uint128 amountOutMinimum, bytes hookData)`.
// The struct is dynamic (bytes hookData) → single-tuple-arg encoding.
fn cl_swap_exact_in_single_json() -> &'static str {
    r#"[{
        "name": "params",
        "type": "tuple",
        "components": [
            { "name": "poolKey", "type": "tuple", "components": [
                { "name": "currency0",   "type": "address" },
                { "name": "currency1",   "type": "address" },
                { "name": "hooks",       "type": "address" },
                { "name": "poolManager", "type": "address" },
                { "name": "fee",         "type": "uint24" },
                { "name": "parameters",  "type": "bytes32" }
            ]},
            { "name": "zeroForOne",       "type": "bool" },
            { "name": "amountIn",         "type": "uint128" },
            { "name": "amountOutMinimum", "type": "uint128" },
            { "name": "hookData",         "type": "bytes" }
        ]
    }]"#
}

const CL_SWAP_EXACT_IN_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "hooks",       "type": "address" },
            { "name": "poolManager", "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "parameters",  "type": "bytes32" }
        ]},
        { "name": "zeroForOne",       "type": "bool" },
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" },
        { "name": "hookData",         "type": "bytes" }
    ]
}]"#;

// CL_SWAP_EXACT_IN (multi-hop) — `CLSwapExactInputParams` =
// `(Currency currencyIn, PathKey[] path, uint128 amountIn, uint128 amountOutMinimum)`.
// Note: NO `minHopPriceX36` (Uniswap V4Router has it; Pancake Infinity router doesn't).
const CL_SWAP_EXACT_IN_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyIn", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "hooks",                "type": "address" },
            { "name": "poolManager",          "type": "address" },
            { "name": "hookData",             "type": "bytes" },
            { "name": "parameters",           "type": "bytes32" }
        ]},
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" }
    ]
}]"#;

const CL_SWAP_EXACT_OUT_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "hooks",       "type": "address" },
            { "name": "poolManager", "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "parameters",  "type": "bytes32" }
        ]},
        { "name": "zeroForOne",      "type": "bool" },
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" },
        { "name": "hookData",        "type": "bytes" }
    ]
}]"#;

const CL_SWAP_EXACT_OUT_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyOut", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "hooks",                "type": "address" },
            { "name": "poolManager",          "type": "address" },
            { "name": "hookData",             "type": "bytes" },
            { "name": "parameters",           "type": "bytes32" }
        ]},
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" }
    ]
}]"#;

// Bin swap params — same shape as CL but `swapForY` instead of `zeroForOne`.

const BIN_SWAP_EXACT_IN_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "hooks",       "type": "address" },
            { "name": "poolManager", "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "parameters",  "type": "bytes32" }
        ]},
        { "name": "swapForY",         "type": "bool" },
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" },
        { "name": "hookData",         "type": "bytes" }
    ]
}]"#;

const BIN_SWAP_EXACT_IN_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyIn", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "hooks",                "type": "address" },
            { "name": "poolManager",          "type": "address" },
            { "name": "hookData",             "type": "bytes" },
            { "name": "parameters",           "type": "bytes32" }
        ]},
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" }
    ]
}]"#;

const BIN_SWAP_EXACT_OUT_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "hooks",       "type": "address" },
            { "name": "poolManager", "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "parameters",  "type": "bytes32" }
        ]},
        { "name": "swapForY",        "type": "bool" },
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" },
        { "name": "hookData",        "type": "bytes" }
    ]
}]"#;

const BIN_SWAP_EXACT_OUT_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyOut", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "hooks",                "type": "address" },
            { "name": "poolManager",          "type": "address" },
            { "name": "hookData",             "type": "bytes" },
            { "name": "parameters",           "type": "bytes32" }
        ]},
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" }
    ]
}]"#;

// Suppress dead-code warning for the helper retained for documentation
// symmetry with the V4Router file's pattern.
#[allow(dead_code)]
const _UNUSED_REFERENCES: (&str, &str, fn() -> &'static str) = (
    POOL_KEY_INFI_JSON,
    PATH_KEY_INFI_TUPLE,
    cl_swap_exact_in_single_json,
);

/// Pancake Infinity action stream uses the full byte for the opcode and
/// has no `allowRevert` flag at the action level (the UR command's
/// `allowRevert` already governs the whole INFI_SWAP step).
pub const PANCAKE_INFI_MASK: u8 = 0xff;

/// Pancake Infinity action dispatch table — feeds inside Pancake UR's
/// `INFI_SWAP` opcode (`0x10`).
pub static PANCAKE_INFI_TABLE: OpcodeTable = OpcodeTable {
    mask: PANCAKE_INFI_MASK,
    allow_revert_bit: 0,
    entries: ENTRIES,
};

const ENTRIES: &[OpcodeEntry] = &[
    // ---- CL pool liquidity actions (0x00–0x05) — PositionManager-only ----
    OpcodeEntry {
        opcode: 0x00,
        name: "CL_INCREASE_LIQUIDITY",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x01,
        name: "CL_DECREASE_LIQUIDITY",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x02,
        name: "CL_MINT_POSITION",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x03,
        name: "CL_BURN_POSITION",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x04,
        name: "CL_INCREASE_LIQUIDITY_FROM_DELTAS",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x05,
        name: "CL_MINT_POSITION_FROM_DELTAS",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- CL pool swapping (0x06–0x09) ----
    OpcodeEntry {
        opcode: 0x06,
        name: "CL_SWAP_EXACT_IN_SINGLE",
        input_signatures: &[],
        input_json_abi: Some(CL_SWAP_EXACT_IN_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x07,
        name: "CL_SWAP_EXACT_IN",
        input_signatures: &[],
        input_json_abi: Some(CL_SWAP_EXACT_IN_JSON),
    },
    OpcodeEntry {
        opcode: 0x08,
        name: "CL_SWAP_EXACT_OUT_SINGLE",
        input_signatures: &[],
        input_json_abi: Some(CL_SWAP_EXACT_OUT_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x09,
        name: "CL_SWAP_EXACT_OUT",
        input_signatures: &[],
        input_json_abi: Some(CL_SWAP_EXACT_OUT_JSON),
    },
    OpcodeEntry {
        opcode: 0x0a,
        name: "CL_DONATE",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- shared settlement (0x0b–0x16) — same shapes as Uniswap V4Router ----
    OpcodeEntry {
        opcode: 0x0b,
        name: "SETTLE",
        input_signatures: &["(address currency, uint256 amount, bool payerIsUser)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0c,
        name: "SETTLE_ALL",
        input_signatures: &["(address currency, uint256 maxAmount)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0d,
        name: "SETTLE_PAIR",
        input_signatures: &["(address currency0, address currency1)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0e,
        name: "TAKE",
        input_signatures: &["(address currency, address recipient, uint256 amount)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0f,
        name: "TAKE_ALL",
        input_signatures: &["(address currency, uint256 minAmount)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x10,
        name: "TAKE_PORTION",
        input_signatures: &["(address currency, address recipient, uint256 bips)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x11,
        name: "TAKE_PAIR",
        input_signatures: &["(address currency0, address currency1, address recipient)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x12,
        name: "CLOSE_CURRENCY",
        input_signatures: &["(address currency)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x13,
        name: "CLEAR_OR_TAKE",
        input_signatures: &["(address currency, uint256 amountMax)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x14,
        name: "SWEEP",
        input_signatures: &["(address currency, address to)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x15,
        name: "WRAP",
        input_signatures: &["(uint256 amount)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x16,
        name: "UNWRAP",
        input_signatures: &["(uint256 amount)"],
        input_json_abi: None,
    },
    // ---- ERC-6909 (not supported in router or PM upstream) ----
    OpcodeEntry {
        opcode: 0x17,
        name: "MINT_6909",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x18,
        name: "BURN_6909",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- Bin pool liquidity actions (0x19–0x1b) — PositionManager-only ----
    OpcodeEntry {
        opcode: 0x19,
        name: "BIN_ADD_LIQUIDITY",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x1a,
        name: "BIN_REMOVE_LIQUIDITY",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x1b,
        name: "BIN_ADD_LIQUIDITY_FROM_DELTAS",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- Bin pool swapping (0x1c–0x1f) ----
    OpcodeEntry {
        opcode: 0x1c,
        name: "BIN_SWAP_EXACT_IN_SINGLE",
        input_signatures: &[],
        input_json_abi: Some(BIN_SWAP_EXACT_IN_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x1d,
        name: "BIN_SWAP_EXACT_IN",
        input_signatures: &[],
        input_json_abi: Some(BIN_SWAP_EXACT_IN_JSON),
    },
    OpcodeEntry {
        opcode: 0x1e,
        name: "BIN_SWAP_EXACT_OUT_SINGLE",
        input_signatures: &[],
        input_json_abi: Some(BIN_SWAP_EXACT_OUT_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x1f,
        name: "BIN_SWAP_EXACT_OUT",
        input_signatures: &[],
        input_json_abi: Some(BIN_SWAP_EXACT_OUT_JSON),
    },
    OpcodeEntry {
        opcode: 0x20,
        name: "BIN_DONATE",
        input_signatures: &[],
        input_json_abi: None,
    },
];

/// Pull the inner `(bytes actions, bytes[] params)` pair out of a Pancake
/// UR `INFI_SWAP` step (UR opcode `0x10`). Same shape as Uniswap UR's
/// `V4_SWAP` payload.
#[must_use]
pub fn extract_actions_and_params(step: &DecodedStep) -> Option<(Vec<u8>, Vec<Vec<u8>>)> {
    let args = step.args.as_ref()?;
    if args.len() < 2 {
        return None;
    }
    let DynSolValue::Bytes(actions) = &args[0].value else {
        return None;
    };
    let DynSolValue::Array(params_items) = &args[1].value else {
        return None;
    };
    let mut params = Vec::with_capacity(params_items.len());
    for v in params_items {
        let DynSolValue::Bytes(b) = v else {
            return None;
        };
        params.push(b.clone());
    }
    Some((actions.clone(), params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::opcode_stream::dispatch;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::{Address, U256};

    #[test]
    fn settle_take_decode_against_table() {
        // 0x0b SETTLE + 0x0e TAKE — same shape as V4Router. Verify the
        // shared settlement opcodes line up.
        fn enc(sig: &str, vals: Vec<DynSolValue>) -> Vec<u8> {
            let f = Function::parse(&format!("step{sig}")).unwrap();
            f.abi_encode_input(&vals).unwrap()[4..].to_vec()
        }
        let settle = enc(
            "(address,uint256,bool)",
            vec![
                DynSolValue::Address(Address::from([0x11; 20])),
                DynSolValue::Uint(U256::from(123u64), 256),
                DynSolValue::Bool(true),
            ],
        );
        let take = enc(
            "(address,address,uint256)",
            vec![
                DynSolValue::Address(Address::from([0x22; 20])),
                DynSolValue::Address(Address::from([0x33; 20])),
                DynSolValue::Uint(U256::from(456u64), 256),
            ],
        );
        let steps = dispatch(&[0x0b, 0x0e], &[settle, take], &PANCAKE_INFI_TABLE);
        assert_eq!(steps[0].name, "SETTLE");
        assert_eq!(steps[1].name, "TAKE");
        assert!(steps[0].args.is_some());
        assert!(steps[1].args.is_some());
    }

    #[test]
    fn cl_swap_exact_in_single_decodes_named_fields() {
        // Build CLSwapExactInputSingleParams with native-currency PoolKey.
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(Address::ZERO),
            DynSolValue::Address(Address::from([0xaa; 20])),
            DynSolValue::Address(Address::ZERO),
            DynSolValue::Address(Address::from([0xbb; 20])),
            DynSolValue::Uint(U256::from(2500u64), 24),
            DynSolValue::FixedBytes(alloy_primitives::B256::ZERO, 32),
        ]);
        let params_tuple = DynSolValue::Tuple(vec![
            pool_key,
            DynSolValue::Bool(true),
            DynSolValue::Uint(U256::from(1_000u64), 128),
            DynSolValue::Uint(U256::from(900u64), 128),
            DynSolValue::Bytes(Vec::new()),
        ]);
        let f = Function::parse(
            "step(((address,address,address,address,uint24,bytes32),bool,uint128,uint128,bytes))",
        )
        .unwrap();
        let payload = f.abi_encode_input(&[params_tuple]).unwrap()[4..].to_vec();
        let steps = dispatch(&[0x06], &[payload], &PANCAKE_INFI_TABLE);
        assert_eq!(steps[0].name, "CL_SWAP_EXACT_IN_SINGLE");
        let args = steps[0].args.as_ref().unwrap();
        assert_eq!(args[0].name, "params");
        let comp_names: Vec<&str> = args[0].components.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            comp_names,
            [
                "poolKey",
                "zeroForOne",
                "amountIn",
                "amountOutMinimum",
                "hookData"
            ]
        );
    }
}
