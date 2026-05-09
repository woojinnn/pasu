//! Uniswap V4 router opcode table (Cat B nested under UR's V4_SWAP).
//!
//! When the Universal Router executes opcode `0x10 V4_SWAP`, the inner
//! `inputs[i]` is a `(bytes actions, bytes[] params)` pair driven by V4
//! periphery's [`Actions.sol`] library. This module provides the action
//! table for the **router** path (swaps + delta settlement); position-
//! management actions (used by V4 PositionManager, not V4_SWAP) are
//! recognised by name but kept label-only so the table doubles as a
//! reference for that follow-up wiring.
//!
//! Source: `Uniswap/v4-periphery @ main`:
//! - `src/libraries/Actions.sol` — opcode constants
//! - `src/V4Router.sol::_handleAction` — input ABI shapes
//! - `src/interfaces/IV4Router.sol` — `ExactInput*Params` / `ExactOutput*Params`
//! - `src/libraries/PathKey.sol` — `PathKey` struct
//!
//! [`Actions.sol`]: https://github.com/Uniswap/v4-periphery/blob/main/src/libraries/Actions.sol

use alloy_dyn_abi::DynSolValue;

use crate::subdecode::opcode_stream::{DecodedStep, OpcodeEntry, OpcodeTable};

// ---------------------------------------------------------------------------
// JSON-ABI literals for the four V4Router swap actions. Using JSON instead of
// Solidity signature strings is what lets us preserve named fields **inside**
// the parameter struct (`params.poolKey.currency0`, `params.path[].fee`, …) —
// alloy's signature parser only accepts named identifiers at the outer
// function-arg level. Each constant is a JSON array of standard ABI Param
// objects describing the inputs to a synthetic 1-arg function whose only arg
// is the action's parameter struct.
// ---------------------------------------------------------------------------

/// `IV4Router.ExactInputSingleParams`.
const SWAP_EXACT_IN_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "tickSpacing", "type": "int24" },
            { "name": "hooks",       "type": "address" }
        ]},
        { "name": "zeroForOne",       "type": "bool" },
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" },
        { "name": "minHopPriceX36",   "type": "uint256" },
        { "name": "hookData",         "type": "bytes" }
    ]
}]"#;

/// `IV4Router.ExactInputParams` (multi-hop exact-in).
const SWAP_EXACT_IN_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyIn", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "tickSpacing",          "type": "int24" },
            { "name": "hooks",                "type": "address" },
            { "name": "hookData",             "type": "bytes" }
        ]},
        { "name": "minHopPriceX36",   "type": "uint256[]" },
        { "name": "amountIn",         "type": "uint128" },
        { "name": "amountOutMinimum", "type": "uint128" }
    ]
}]"#;

/// `IV4Router.ExactOutputSingleParams`.
const SWAP_EXACT_OUT_SINGLE_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "poolKey", "type": "tuple", "components": [
            { "name": "currency0",   "type": "address" },
            { "name": "currency1",   "type": "address" },
            { "name": "fee",         "type": "uint24" },
            { "name": "tickSpacing", "type": "int24" },
            { "name": "hooks",       "type": "address" }
        ]},
        { "name": "zeroForOne",      "type": "bool" },
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" },
        { "name": "minHopPriceX36",  "type": "uint256" },
        { "name": "hookData",        "type": "bytes" }
    ]
}]"#;

/// `IV4Router.ExactOutputParams` (multi-hop exact-out).
const SWAP_EXACT_OUT_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "currencyOut", "type": "address" },
        { "name": "path", "type": "tuple[]", "components": [
            { "name": "intermediateCurrency", "type": "address" },
            { "name": "fee",                  "type": "uint24" },
            { "name": "tickSpacing",          "type": "int24" },
            { "name": "hooks",                "type": "address" },
            { "name": "hookData",             "type": "bytes" }
        ]},
        { "name": "minHopPriceX36",  "type": "uint256[]" },
        { "name": "amountOut",       "type": "uint128" },
        { "name": "amountInMaximum", "type": "uint128" }
    ]
}]"#;

/// V4 routers/PositionManager use the full byte for the opcode — there is no
/// `allowRevert` flag like UR's `0x80` bit. We mirror UR's table struct shape
/// by setting `mask = 0xff` and `allow_revert_bit = 0`.
pub const V4_ROUTER_MASK: u8 = 0xff;

/// Action table for the V4 router path. Used when dispatching the inner
/// `(actions, params)` pair from a UR `V4_SWAP` step.
pub static V4_ROUTER_TABLE: OpcodeTable = OpcodeTable {
    mask: V4_ROUTER_MASK,
    allow_revert_bit: 0,
    entries: ENTRIES,
};

const ENTRIES: &[OpcodeEntry] = &[
    // ---- liquidity actions (PositionManager only — kept label-only) ----
    OpcodeEntry {
        opcode: 0x00,
        name: "INCREASE_LIQUIDITY",
        // PositionManager schema: (uint256 tokenId, uint256 liquidity,
        // uint128 amount0Max, uint128 amount1Max, bytes hookData). Wired up
        // when V4_POSITION_MANAGER_CALL recursion lands.
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x01,
        name: "DECREASE_LIQUIDITY",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x02,
        name: "MINT_POSITION",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x03,
        name: "BURN_POSITION",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x04,
        name: "INCREASE_LIQUIDITY_FROM_DELTAS",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x05,
        name: "MINT_POSITION_FROM_DELTAS",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- swapping (V4Router) ----
    // V4Router decodes each swap action's `params` via
    // `abi.decode(params, (Struct))`, which is single-arg encoding with a
    // leading offset (the structs are all dynamic — they contain
    // `bytes hookData` and/or `PathKey[] path`). We therefore wrap the
    // struct in an outer tuple here. Inner field names are dropped because
    // alloy's signature parser only accepts named identifiers at the outer
    // function-arg level.
    OpcodeEntry {
        opcode: 0x06,
        name: "SWAP_EXACT_IN_SINGLE",
        input_signatures: &[
            "(((address,address,uint24,int24,address),bool,uint128,uint128,uint256,bytes) params)",
        ],
        input_json_abi: Some(SWAP_EXACT_IN_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x07,
        name: "SWAP_EXACT_IN",
        input_signatures: &[
            "((address,(address,uint24,int24,address,bytes)[],uint256[],uint128,uint128) params)",
        ],
        input_json_abi: Some(SWAP_EXACT_IN_JSON),
    },
    OpcodeEntry {
        opcode: 0x08,
        name: "SWAP_EXACT_OUT_SINGLE",
        input_signatures: &[
            "(((address,address,uint24,int24,address),bool,uint128,uint128,uint256,bytes) params)",
        ],
        input_json_abi: Some(SWAP_EXACT_OUT_SINGLE_JSON),
    },
    OpcodeEntry {
        opcode: 0x09,
        name: "SWAP_EXACT_OUT",
        input_signatures: &[
            "((address,(address,uint24,int24,address,bytes)[],uint256[],uint128,uint128) params)",
        ],
        input_json_abi: Some(SWAP_EXACT_OUT_JSON),
    },
    // ---- donate (not supported by router or PM in current periphery) ----
    OpcodeEntry {
        opcode: 0x0a,
        name: "DONATE",
        input_signatures: &[],
        input_json_abi: None,
    },
    // ---- delta settlement (shared between V4Router & PositionManager) ----
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
    // ---- close / sweep / clear (PositionManager primarily) ----
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
    // ---- wrap / unwrap (PositionManager) ----
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
    // ---- ERC-6909 (not supported in V4Router or PositionManager) ----
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
];

/// Pull the inner `(bytes actions, bytes[] params)` pair out of a Universal
/// Router `V4_SWAP` step.
///
/// Caller must already have decoded the V4_SWAP step against UR's table —
/// that produces two named args (`actions`, `params`) from the schema
/// `(bytes actions, bytes[] params)`. This helper grabs them back as raw
/// bytes so the orchestrator can re-dispatch through [`V4_ROUTER_TABLE`].
///
/// Returns `None` when the step's args don't structurally match (e.g. the
/// caller handed in a different opcode's step by mistake, or ABI decode
/// fell back to raw input).
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

    fn encode(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
        let func = Function::parse(&format!("step{sig}")).unwrap();
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn settle_take_decode_against_table() {
        // 0x0b SETTLE, 0x0e TAKE — the standard pair after a V4 swap.
        let settle_input = encode(
            "(address,uint256,bool)",
            vec![
                DynSolValue::Address(Address::from([0x11; 20])),
                DynSolValue::Uint(U256::from(1_000_000_u64), 256),
                DynSolValue::Bool(true),
            ],
        );
        let take_input = encode(
            "(address,address,uint256)",
            vec![
                DynSolValue::Address(Address::from([0x22; 20])),
                DynSolValue::Address(Address::from([0x33; 20])),
                DynSolValue::Uint(U256::from(2_000_000_u64), 256),
            ],
        );
        let actions = vec![0x0b, 0x0e];
        let inputs = vec![settle_input, take_input];
        let steps = dispatch(&actions, &inputs, &V4_ROUTER_TABLE);

        assert_eq!(steps[0].name, "SETTLE");
        assert!(steps[0].args.is_some());
        let settle_args = steps[0].args.as_ref().unwrap();
        assert_eq!(settle_args[0].name, "currency");
        assert_eq!(settle_args[1].name, "amount");
        assert_eq!(settle_args[2].name, "payerIsUser");

        assert_eq!(steps[1].name, "TAKE");
        assert!(steps[1].args.is_some());
        let take_args = steps[1].args.as_ref().unwrap();
        assert_eq!(take_args[0].name, "currency");
        assert_eq!(take_args[1].name, "recipient");
        assert_eq!(take_args[2].name, "amount");
    }

    #[test]
    fn unsupported_or_unimplemented_opcode_falls_back_gracefully() {
        // INCREASE_LIQUIDITY (0x00) is label-only here.
        let actions = vec![0x00];
        let inputs = vec![vec![0u8; 32]];
        let steps = dispatch(&actions, &inputs, &V4_ROUTER_TABLE);
        assert_eq!(steps[0].name, "INCREASE_LIQUIDITY");
        assert!(steps[0].args.is_none());
    }
}
