//! Uniswap Universal Router opcode table (opcode-dispatched).
//!
//! Each entry maps an opcode (after applying the table's `mask` — see
//! [`UNISWAP_UR_TABLE`]) to its name and a list of candidate Solidity tuple
//! types to try for `inputs[i]`. The list approach handles the fact that the
//! V2/V3 swap opcode schemas changed between deployments: the current
//! `Dispatcher.sol` on `main` uses
//! `(address,uint256,uint256,bytes,bool,uint256[])` (with
//! `uint256[] minHopPriceX36`), while earlier deployments still in production
//! use the shorter `(address,uint256,uint256,bytes,bool)` shape. We list
//! both and the engine picks whichever decodes cleanly.
//!
//! The opcode set itself was cross-checked against the upstream
//! `contracts/libraries/Commands.sol` and `contracts/base/Dispatcher.sol`
//! (Uniswap/universal-router @ `main`). Placeholder ranges (0x0f, 0x15-0x20,
//! 0x22-0x3f, 0x41-0x5f) are intentionally omitted — the dispatcher reverts
//! on those, and our engine falls back to `UNKNOWN(raw …)` which is the
//! right under-decode behaviour.
//!
//! Coverage is partial: opcodes whose `inputs[i]` is a complex variable-
//! length struct (`PERMIT2_PERMIT*`, NPM permits/calls, V4_SWAP) are
//! recognised by name but kept as raw bytes for now. Adding their schemas
//! is independent of the engine.

use alloy_primitives::Address;

use crate::subdecode::opcode_stream::{OpcodeEntry, OpcodeTable};

/// Uniswap UR command bytes use the high bit (`0x80`) for `allowRevert`.
pub const UNISWAP_UR_MASK: u8 = 0x7f;
/// Bit set on a command byte when its revert should be tolerated by the
/// dispatcher.
pub const UNISWAP_UR_ALLOW_REVERT: u8 = 0x80;

// ---------------------------------------------------------------------------
// JSON-ABI literals for the UR opcodes whose `inputs[i]` carries a named
// struct. Using JSON instead of a Solidity signature string preserves field
// names at every level (`params.permitSingle.details.token`,
// `params.poolKey.currency0`, …) — alloy's signature parser only accepts
// names at the outer function-arg level.
// ---------------------------------------------------------------------------

/// `IAllowanceTransfer.PermitSingle` plus the trailing `bytes signature`.
const PERMIT2_PERMIT_JSON: &str = r#"[
    { "name": "permitSingle", "type": "tuple", "components": [
        { "name": "details", "type": "tuple", "components": [
            { "name": "token",      "type": "address" },
            { "name": "amount",     "type": "uint160" },
            { "name": "expiration", "type": "uint48" },
            { "name": "nonce",      "type": "uint48" }
        ]},
        { "name": "spender",     "type": "address" },
        { "name": "sigDeadline", "type": "uint256" }
    ]},
    { "name": "signature", "type": "bytes" }
]"#;

/// `IAllowanceTransfer.PermitBatch` plus the trailing `bytes signature`.
const PERMIT2_PERMIT_BATCH_JSON: &str = r#"[
    { "name": "permitBatch", "type": "tuple", "components": [
        { "name": "details", "type": "tuple[]", "components": [
            { "name": "token",      "type": "address" },
            { "name": "amount",     "type": "uint160" },
            { "name": "expiration", "type": "uint48" },
            { "name": "nonce",      "type": "uint48" }
        ]},
        { "name": "spender",     "type": "address" },
        { "name": "sigDeadline", "type": "uint256" }
    ]},
    { "name": "signature", "type": "bytes" }
]"#;

/// `IAllowanceTransfer.AllowanceTransferDetails[]` (single arg).
const PERMIT2_TRANSFER_FROM_BATCH_JSON: &str = r#"[
    { "name": "transferDetails", "type": "tuple[]", "components": [
        { "name": "from",   "type": "address" },
        { "name": "to",     "type": "address" },
        { "name": "amount", "type": "uint160" },
        { "name": "token",  "type": "address" }
    ]}
]"#;

/// `(PoolKey poolKey, uint160 sqrtPriceX96)` for `V4_INITIALIZE_POOL`.
const V4_INITIALIZE_POOL_JSON: &str = r#"[
    { "name": "poolKey", "type": "tuple", "components": [
        { "name": "currency0",   "type": "address" },
        { "name": "currency1",   "type": "address" },
        { "name": "fee",         "type": "uint24" },
        { "name": "tickSpacing", "type": "int24" },
        { "name": "hooks",       "type": "address" }
    ]},
    { "name": "sqrtPriceX96", "type": "uint160" }
]"#;

/// `AcrossV4DepositV3Params` — Across V3 bridge deposit struct.
const ACROSS_V4_DEPOSIT_V3_JSON: &str = r#"[{
    "name": "params",
    "type": "tuple",
    "components": [
        { "name": "depositor",            "type": "address" },
        { "name": "recipient",            "type": "address" },
        { "name": "inputToken",           "type": "address" },
        { "name": "outputToken",          "type": "address" },
        { "name": "inputAmount",          "type": "uint256" },
        { "name": "outputAmount",         "type": "uint256" },
        { "name": "destinationChainId",   "type": "uint256" },
        { "name": "exclusiveRelayer",     "type": "address" },
        { "name": "quoteTimestamp",       "type": "uint32" },
        { "name": "fillDeadline",         "type": "uint32" },
        { "name": "exclusivityDeadline",  "type": "uint32" },
        { "name": "message",              "type": "bytes" },
        { "name": "useNative",            "type": "bool" }
    ]
}]"#;

/// Uniswap UR opcode dispatch table.
///
/// See `crates/abi-resolver/docs` (or the inventory in CLAUDE.md §3.2) for
/// the full reference list.
pub static UNISWAP_UR_TABLE: OpcodeTable = OpcodeTable {
    mask: UNISWAP_UR_MASK,
    allow_revert_bit: UNISWAP_UR_ALLOW_REVERT,
    entries: ENTRIES,
};

const ENTRIES: &[OpcodeEntry] = &[
    // 0x00 V3_SWAP_EXACT_IN — current Dispatcher.sol uses the 6-tuple with
    // minHopPriceX36; older deployments use the 5-tuple shape.
    OpcodeEntry {
        opcode: 0x00,
        name: "V3_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser, uint256[] minHopPriceX36)",
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x01,
        name: "V3_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bool payerIsUser, uint256[] minHopPriceX36)",
            "(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x02,
        name: "PERMIT2_TRANSFER_FROM",
        input_signatures: &["(address token, address recipient, uint160 amount)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x03,
        name: "PERMIT2_PERMIT_BATCH",
        input_signatures: &[
            "(((address,uint160,uint48,uint48)[],address,uint256) permitBatch, bytes signature)",
        ],
        input_json_abi: Some(PERMIT2_PERMIT_BATCH_JSON),
    },
    OpcodeEntry {
        opcode: 0x04,
        name: "SWEEP",
        input_signatures: &["(address token, address recipient, uint256 amountMin)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x05,
        name: "TRANSFER",
        input_signatures: &["(address token, address recipient, uint256 value)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x06,
        name: "PAY_PORTION",
        input_signatures: &["(address token, address recipient, uint256 bips)"],
            input_json_abi: None,
},
    // 0x07 — Uniswap-only opcode (Pancake UR has placeholder here).
    OpcodeEntry {
        opcode: 0x07,
        name: "PAY_PORTION_FULL_PRECISION",
        input_signatures: &["(address token, address recipient, uint256 portion)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x08,
        name: "V2_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, address[] path, bool payerIsUser, uint256[] minHopPriceX36)",
            "(address recipient, uint256 amountIn, uint256 amountOutMin, address[] path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x09,
        name: "V2_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, address[] path, bool payerIsUser, uint256[] minHopPriceX36)",
            "(address recipient, uint256 amountOut, uint256 amountInMax, address[] path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0a,
        name: "PERMIT2_PERMIT",
        input_signatures: &[
            "(((address,uint160,uint48,uint48),address,uint256) permitSingle, bytes signature)",
        ],
        input_json_abi: Some(PERMIT2_PERMIT_JSON),
    },
    OpcodeEntry {
        opcode: 0x0b,
        name: "WRAP_ETH",
        input_signatures: &["(address recipient, uint256 amountMin)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x0c,
        name: "UNWRAP_WETH",
        input_signatures: &["(address recipient, uint256 amountMin)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x0d,
        name: "PERMIT2_TRANSFER_FROM_BATCH",
        input_signatures: &["((address,address,uint160,address)[] transferDetails)"],
        input_json_abi: Some(PERMIT2_TRANSFER_FROM_BATCH_JSON),
    },
    OpcodeEntry {
        opcode: 0x0e,
        name: "BALANCE_CHECK_ERC20",
        input_signatures: &["(address owner, address token, uint256 minBalance)"],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x10,
        name: "V4_SWAP",
        // Top-level shape is a 2-tuple `(bytes actions, bytes[] params)` —
        // the inner `actions` byte stream and per-action `params[i]` are
        // dispatched by the V4Router opcode table (Actions.sol), which is
        // not yet wired up here. Decoding the outer pair still gives the
        // user a peek at the action byte string and parameter sub-blobs.
        input_signatures: &["(bytes actions, bytes[] params)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x11,
        name: "V3_POSITION_MANAGER_PERMIT",
        // Input is a complete calldata for the V3 NonfungiblePositionManager
        // (selector + ABI args), `address(V3_POSITION_MANAGER).call(inputs)`
        // upstream. Decoding it cleanly needs recursive sub-decoding through
        // the resolver against the NPM ABI — out of PR3 scope.
        input_signatures: &[],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x12,
        name: "V3_POSITION_MANAGER_CALL",
        // Same as 0x11: input IS NPM calldata, not a tuple. Recurse via
        // resolver in a follow-up PR.
        input_signatures: &[],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x13,
        name: "V4_INITIALIZE_POOL",
        input_signatures: &[
            "((address,address,uint24,int24,address) poolKey, uint160 sqrtPriceX96)",
        ],
        input_json_abi: Some(V4_INITIALIZE_POOL_JSON),
    },
    OpcodeEntry {
        opcode: 0x14,
        name: "V4_POSITION_MANAGER_CALL",
        // Input is a complete calldata for V4 PositionManager
        // (selector + args), forwarded via `.call(inputs)` upstream. Decoding
        // requires recursive sub-decoding through the resolver — same reason
        // as 0x11 / 0x12.
        input_signatures: &[],
            input_json_abi: None,
},
    OpcodeEntry {
        opcode: 0x21,
        name: "EXECUTE_SUB_PLAN",
        // Top-level shape is `(bytes commands, bytes[] inputs)` — the same
        // pair shape as the outer `execute(...)` entrypoint. The orchestrator
        // would ideally re-dispatch this through the same UR opcode table
        // (self-recursive opcode-dispatched); for now we surface the inner pair so the
        // user can at least see the nested commands byte stream.
        input_signatures: &["(bytes commands, bytes[] inputs)"],
        input_json_abi: None,
    },
    // 0x40 — third-party integration: Across V3 bridge deposit. Upstream
    // does `abi.decode(input, (AcrossV4DepositV3Params))` — single-arg
    // encoding of a dynamic struct (the `bytes message` field makes it
    // dynamic). We mirror that here as a single-tuple-arg schema; field
    // names are dropped because alloy doesn't accept identifiers inside a
    // tuple type literal.
    OpcodeEntry {
        opcode: 0x40,
        name: "ACROSS_V4_DEPOSIT_V3",
        input_signatures: &[
            "((address,address,address,address,uint256,uint256,uint256,address,uint32,uint32,uint32,bytes,bool) params)",
        ],
        input_json_abi: Some(ACROSS_V4_DEPOSIT_V3_JSON),
    },
];

/// Selector for `execute(bytes,bytes[],uint256)` — the deadline-checked
/// Universal Router entrypoint. Most production txs use this overload.
pub const EXECUTE_DEADLINE_SELECTOR: [u8; 4] = [0x35, 0x93, 0x56, 0x4c];
/// Selector for `execute(bytes,bytes[])` — no deadline.
pub const EXECUTE_SELECTOR: [u8; 4] = [0x24, 0x85, 0x6b, 0xc3];

/// True when the selector matches one of the public Universal Router
/// `execute` overloads. **Selector match alone is not enough to safely
/// dispatch** — the same selector is shared by every UR fork (Pancake,
/// OKX, …) but each fork has its own opcode table. Use
/// [`is_uniswap_universal_router`] in tandem so the orchestrator only
/// applies [`UNISWAP_UR_TABLE`] to addresses we trust.
#[must_use]
pub fn is_universal_router_execute(selector: &[u8; 4]) -> bool {
    matches!(*selector, EXECUTE_DEADLINE_SELECTOR | EXECUTE_SELECTOR)
}

/// Allowlist of Uniswap Universal Router deployments — `(chain_id, address)`.
/// New deploys can be added here as they're verified.
const UNISWAP_UR_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet — original Universal Router (in our curated bundle).
    (
        1,
        Address::new(
            *b"\x66\xa9\x89\x3c\xc0\x7d\x91\xd9\x56\x44\xae\xdd\x05\xd0\x3f\x95\xe1\xdb\xa8\xaf",
        ),
    ),
    // Ethereum mainnet — V4-supporting Universal Router (not on Sourcify
    // yet; Etherscan-verified).
    (
        1,
        Address::new(
            *b"\x4c\x82\xd1\xfb\xfe\x28\xc9\x77\xcb\xb5\x8d\x8c\x7f\xf8\xfc\xf9\xf7\x0a\x2c\xca",
        ),
    ),
];

/// Returns true when `(chain_id, target)` matches a known Uniswap Universal
/// Router deployment we trust to use the [`UNISWAP_UR_TABLE`] dispatch shape.
#[must_use]
pub fn is_uniswap_universal_router(chain_id: u64, target: &Address) -> bool {
    UNISWAP_UR_ADDRESSES
        .iter()
        .any(|(chain, addr)| *chain == chain_id && addr == target)
}

/// Per-chain Uniswap V3 NonfungiblePositionManager addresses. The same
/// contract is `CREATE2`-deployed at the same address on most EVM chains
/// (mainnet/arbitrum/optimism/polygon/base/etc.) but the address registry
/// stays explicit so we don't blindly recurse against unknown chains.
const V3_NPM_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet, Polygon, Arbitrum, Optimism, Base — all share this
    // CREATE2 address.
    (
        1,
        Address::new(
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88",
        ),
    ),
];

/// Per-chain Uniswap V4 PositionManager addresses (different per chain — V4
/// is fresh-deployed without CREATE2 colocation).
const V4_PM_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet
    (
        1,
        Address::new(
            *b"\xbd\x21\x65\x13\xd7\x4c\x8c\xf1\x4c\xf4\x74\x7e\x6a\xaa\x64\x20\xff\x64\xee\x9e",
        ),
    ),
    // Optimism
    (
        10,
        Address::new(
            *b"\x3c\x3e\xa4\xb5\x7a\x46\x24\x1e\x54\x61\x0e\x5f\x02\x2e\x5c\x45\x85\x9a\x10\x17",
        ),
    ),
    // Polygon
    (
        137,
        Address::new(
            *b"\x1e\xc2\xeb\xf4\xf3\x7e\x73\x63\xfd\xfe\x35\x51\x60\x24\x25\xaf\x0b\x3c\xee\xf9",
        ),
    ),
    // Arbitrum One
    (
        42161,
        Address::new(
            *b"\xd8\x8f\x38\xf9\x30\xb7\x95\x2f\x2d\xb2\x43\x2c\xb0\x02\xe7\xab\xbf\x3d\xd8\x69",
        ),
    ),
    // Base
    (
        8453,
        Address::new(
            *b"\x7c\x5f\x5a\x4b\xbd\x8f\xd6\x31\x84\x57\x75\x25\x32\x61\x23\xb5\x19\x42\x9b\xdc",
        ),
    ),
    // Blast
    (
        81457,
        Address::new(
            *b"\x4a\xd2\xf4\xcc\xa2\x68\x2c\xbb\x5b\x95\x0d\x66\x0d\xd4\x58\xa1\xd3\xf1\xba\xad",
        ),
    ),
];

/// Look up the V3 NonfungiblePositionManager address for `chain_id`.
#[must_use]
pub fn v3_position_manager_address(chain_id: u64) -> Option<Address> {
    V3_NPM_ADDRESSES
        .iter()
        .find(|(chain, _)| *chain == chain_id)
        .map(|(_, addr)| *addr)
}

/// Look up the V4 PositionManager address for `chain_id`.
#[must_use]
pub fn v4_position_manager_address(chain_id: u64) -> Option<Address> {
    V4_PM_ADDRESSES
        .iter()
        .find(|(chain, _)| *chain == chain_id)
        .map(|(_, addr)| *addr)
}

/// Pull the `(commands, inputs)` pair out of a decoded `execute(...)` call.
///
/// Both UR overloads put `commands` at arg index 0 and `inputs` at arg index
/// 1; the deadline (when present) is arg 2 and is ignored here. Returns
/// `None` when the args don't structurally match — e.g. when callers pass a
/// non-execute decoded call by accident.
#[must_use]
pub fn extract_commands_and_inputs(
    decoded: &crate::decode::DecodedCall,
) -> Option<(Vec<u8>, Vec<Vec<u8>>)> {
    if decoded.args.len() < 2 {
        return None;
    }
    let alloy_dyn_abi::DynSolValue::Bytes(commands) = &decoded.args[0].value else {
        return None;
    };
    let alloy_dyn_abi::DynSolValue::Array(items) = &decoded.args[1].value else {
        return None;
    };
    let mut inputs = Vec::with_capacity(items.len());
    for v in items {
        let alloy_dyn_abi::DynSolValue::Bytes(b) = v else {
            return None;
        };
        inputs.push(b.clone());
    }
    Some((commands.clone(), inputs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::opcode_stream::dispatch;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::{Address, U256};

    fn encode_wrap_eth_input(recipient: [u8; 20], amount: u128) -> Vec<u8> {
        let func = Function::parse("step(address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(Address::from(recipient)),
            DynSolValue::Uint(U256::from(amount), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn execute_selectors_recognised() {
        assert!(is_universal_router_execute(&EXECUTE_DEADLINE_SELECTOR));
        assert!(is_universal_router_execute(&EXECUTE_SELECTOR));
        assert!(!is_universal_router_execute(&[0x09, 0x5e, 0xa7, 0xb3]));
    }

    #[test]
    fn dispatch_decodes_wrap_then_unwrap() {
        let commands = vec![0x0b, 0x0c];
        let inputs = vec![
            encode_wrap_eth_input([0xaa; 20], 1_000_000),
            encode_wrap_eth_input([0xbb; 20], 2_000_000),
        ];
        let steps = dispatch(&commands, &inputs, &UNISWAP_UR_TABLE);
        assert_eq!(steps[0].name, "WRAP_ETH");
        assert_eq!(steps[1].name, "UNWRAP_WETH");
        assert!(steps[0].args.is_some());
        assert!(steps[1].args.is_some());
    }

    #[test]
    fn opcodes_without_schema_keep_label() {
        let commands = vec![0x10, 0x21]; // V4_SWAP, EXECUTE_SUB_PLAN
        let inputs = vec![vec![0x00], vec![0x01]];
        let steps = dispatch(&commands, &inputs, &UNISWAP_UR_TABLE);
        assert_eq!(steps[0].name, "V4_SWAP");
        assert_eq!(steps[1].name, "EXECUTE_SUB_PLAN");
        assert!(steps[0].args.is_none());
        assert!(steps[1].args.is_none());
    }
}
