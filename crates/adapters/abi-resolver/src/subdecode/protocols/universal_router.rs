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
        // dispatched by the V4Router opcode table (Actions.sol). The
        // declarative `opcode_stream_dispatch` layer wires that inner
        // dispatch via `extract_actions_and_params` + `V4_ROUTER_TABLE`
        // (cross-table recursion, mirror of EXECUTE_SUB_PLAN's self-
        // recursion). Decoding the outer pair gives the user the action
        // byte string and per-action parameter sub-blobs.
        input_signatures: &["(bytes actions, bytes[] params)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x11,
        name: "V3_POSITION_MANAGER_PERMIT",
        // Input is a complete calldata for the V3 NonfungiblePositionManager
        // (selector + ABI args), `address(V3_POSITION_MANAGER).call(inputs)`
        // upstream. The outer envelope is a single `bytes data` blob — the
        // declarative `execute_position_manager_step` layer pulls the inner
        // calldata back out, looks up the per-chain V3 NPM address, and
        // dispatches through `ctx.resolver` (mirror of the V4_SWAP cross-table
        // dispatch in [`opcode_stream::execute_v4_swap_step`]).
        input_signatures: &["(bytes data)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x12,
        name: "V3_POSITION_MANAGER_CALL",
        // Same shape as 0x11: input IS a single `bytes` blob carrying NPM
        // calldata. Recursive dispatch is handled by the declarative
        // `execute_position_manager_step`.
        input_signatures: &["(bytes data)"],
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
        // (selector + args), forwarded via `.call(inputs)` upstream. Same
        // recursive-dispatch flow as 0x11 / 0x12; the per-chain V4 PM address
        // is supplied by [`v4_position_manager_address`].
        input_signatures: &["(bytes data)"],
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
/// Source: https://github.com/Uniswap/universal-router/tree/main/deploy-addresses
/// Includes both `UniversalRouterV1_2_V2Support` (V2 swap pool routing) and
/// `UniversalRouterV2` (V4-supporting). `UniversalRouterV1_2_NoV2Support` is
/// intentionally omitted since it cannot route V2 swap opcodes.
const UNISWAP_UR_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet
    (
        1,
        Address::new(
            *b"\x3f\xc9\x1a\x3a\xfd\x70\x39\x5c\xd4\x96\xc6\x47\xd5\xa6\xcc\x9d\x4b\x2b\x7f\xad",
        ),
    ), // UniversalRouterV1_2_V2Support
    (
        1,
        Address::new(
            *b"\x66\xa9\x89\x3c\xc0\x7d\x91\xd9\x56\x44\xae\xdd\x05\xd0\x3f\x95\xe1\xdb\xa8\xaf",
        ),
    ), // UniversalRouterV2 (V4-supporting)
    // Base
    (
        8453,
        Address::new(
            *b"\x3f\xc9\x1a\x3a\xfd\x70\x39\x5c\xd4\x96\xc6\x47\xd5\xa6\xcc\x9d\x4b\x2b\x7f\xad",
        ),
    ), // UniversalRouterV1_2_V2Support (shared CREATE2 addr with mainnet)
    (
        8453,
        Address::new(
            *b"\x6f\xf5\x69\x3b\x99\x21\x2d\xa7\x6a\xd3\x16\x17\x8a\x18\x4a\xb5\x6d\x29\x9b\x43",
        ),
    ), // UniversalRouterV2
    // Optimism
    (
        10,
        Address::new(
            *b"\xcb\x13\x55\xff\x08\xab\x38\xbb\xce\x60\x11\x1f\x1b\xb2\xb7\x84\xbe\x25\xd7\xe8",
        ),
    ), // UniversalRouterV1_2_V2Support
    (
        10,
        Address::new(
            *b"\x85\x11\x16\xd9\x22\x3f\xab\xed\x8e\x56\xc0\xe6\xb8\xad\x0c\x31\xd9\x8b\x35\x07",
        ),
    ), // UniversalRouterV2
    // Arbitrum One
    (
        42161,
        Address::new(
            *b"\x5e\x32\x5e\xda\x80\x64\xb4\x56\xf4\x78\x10\x70\xc0\x73\x8d\x84\x9c\x82\x42\x58",
        ),
    ), // UniversalRouterV1_2_V2Support
    (
        42161,
        Address::new(
            *b"\xa5\x1a\xfa\xfe\x02\x63\xb4\x0e\xda\xef\x0d\xf8\x78\x1e\xa9\xaa\x03\xe3\x81\xa3",
        ),
    ), // UniversalRouterV2
    // Polygon
    (
        137,
        Address::new(
            *b"\xec\x7b\xe8\x9e\x9d\x10\x9e\x7e\x3f\xec\x59\xc2\x22\xcf\x29\x71\x25\xfe\xfd\xa2",
        ),
    ), // UniversalRouterV1_2_V2Support
    (
        137,
        Address::new(
            *b"\x10\x95\x69\x2a\x62\x37\xd8\x3c\x6a\x72\xf3\xf5\xef\xed\xb9\xa6\x70\xc4\x92\x23",
        ),
    ), // UniversalRouterV2
    // Avalanche
    (
        43114,
        Address::new(
            *b"\x4d\xae\x2f\x93\x9a\xcf\x50\x40\x8e\x13\xd5\x85\x34\xff\x8c\x27\x76\xd4\x52\x65",
        ),
    ), // UniversalRouterV1_2_V2Support
    // BNB Chain
    (
        56,
        Address::new(
            *b"\x4d\xae\x2f\x93\x9a\xcf\x50\x40\x8e\x13\xd5\x85\x34\xff\x8c\x27\x76\xd4\x52\x65",
        ),
    ), // UniversalRouterV1_2_V2Support (shared CREATE2 with Avalanche)
    (
        56,
        Address::new(
            *b"\x19\x06\xc1\xd6\x72\xb8\x8c\xd1\xb9\xac\x75\x93\x30\x1c\xa9\x90\xf9\x4e\xae\x07",
        ),
    ), // UniversalRouterV2
    // Blast
    (
        81457,
        Address::new(
            *b"\x64\x37\x70\xe2\x79\xd5\xd0\x73\x3f\x21\xd6\xdc\x03\xa8\xef\xba\xbf\x32\x55\xb4",
        ),
    ), // UniversalRouterV1_2_V2Support
    (
        81457,
        Address::new(
            *b"\xea\xbb\xcb\x3e\x8e\x41\x53\x06\x20\x7e\xf5\x14\xf6\x60\xa3\xf8\x20\x02\x5b\xe3",
        ),
    ), // UniversalRouterV2
    // Celo
    (
        42220,
        Address::new(
            *b"\x64\x37\x70\xe2\x79\xd5\xd0\x73\x3f\x21\xd6\xdc\x03\xa8\xef\xba\xbf\x32\x55\xb4",
        ),
    ), // UniversalRouterV1_2_V2Support (shared CREATE2 with Blast)
    // Ink
    (
        57073,
        Address::new(
            *b"\x11\x29\x08\xda\xc8\x6e\x20\xe7\x24\x1b\x09\x27\x47\x9e\xa3\xbf\x93\x5d\x1f\xa0",
        ),
    ), // UniversalRouterV2
    // Unichain
    (
        130,
        Address::new(
            *b"\xef\x74\x0b\xf2\x3a\xca\xe2\x6f\x64\x92\xb1\x0d\xe6\x45\xd6\xb9\x8d\xc8\xea\xf3",
        ),
    ), // UniversalRouterV2
    // World Chain
    (
        480,
        Address::new(
            *b"\x03\xc4\xf6\xb5\x57\x33\xcd\xf3\xca\xa0\x7c\x01\xe5\xb8\x3d\xde\xe3\x38\x1f\x60",
        ),
    ), // UniversalRouter — corrected: prior value 0x7a250d56…488D was the
    // Ethereum mainnet UniswapV2Router02 address, not a World Chain UR.
    // Source: Uniswap/contracts deployments/480.md.
    (
        480,
        Address::new(
            *b"\x8a\xc7\xbe\xe9\x93\xbb\x44\xda\xb5\x64\xea\x4b\xc9\xea\x67\xbf\x9e\xb5\xe7\x43",
        ),
    ), // UniversalRouterV2
    // Zora
    (
        7777777,
        Address::new(
            *b"\x33\x15\xef\x7c\xa2\x8d\xb7\x4a\xba\xdc\x6c\x44\x57\x0e\xfd\xf0\x6b\x04\xb0\x20",
        ),
    ), // UniversalRouterV2
];

/// Returns true when `(chain_id, target)` matches a known Uniswap Universal
/// Router deployment we trust to use the [`UNISWAP_UR_TABLE`] dispatch shape.
#[must_use]
pub fn is_uniswap_universal_router(chain_id: u64, target: &Address) -> bool {
    UNISWAP_UR_ADDRESSES
        .iter()
        .any(|(chain, addr)| *chain == chain_id && addr == target)
}

/// Iterator over all `(chain_id, address)` Uniswap Universal Router
/// deployments. Used by `call-adapter::MultiRouterCallAdapter::match_keys`
/// to enumerate the (chain, to, selector) keys the adapter handles.
pub fn uniswap_universal_router_deployments() -> impl Iterator<Item = (u64, Address)> {
    UNISWAP_UR_ADDRESSES.iter().copied()
}

/// Per-chain Uniswap V3 NonfungiblePositionManager addresses. The base CREATE2
/// deployment (`0xC36442b4a4522E871399CD717aBDD847Ab11FE88`) is shared by
/// mainnet, Optimism, Arbitrum and Polygon. Other chains have their own
/// distinct NFPM addresses (Base, BNB, Avalanche, Blast, Celo, Zora, Ink,
/// Unichain, World Chain). The address registry is explicit so we don't
/// blindly recurse against unknown chains.
///
/// Source: https://github.com/Uniswap/contracts/tree/main/deployments
/// (`v3NFTPositionManager` field per chain).
const V3_NPM_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet — CREATE2-shared with Optimism/Arbitrum/Polygon.
    (
        1,
        Address::new(
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88",
        ),
    ),
    // Optimism — same CREATE2 address.
    (
        10,
        Address::new(
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88",
        ),
    ),
    // Polygon — same CREATE2 address.
    (
        137,
        Address::new(
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88",
        ),
    ),
    // Arbitrum One — same CREATE2 address.
    (
        42161,
        Address::new(
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88",
        ),
    ),
    // Base — distinct deployment.
    (
        8453,
        Address::new(
            *b"\x03\xa5\x20\xb3\x2c\x04\xbf\x3b\xee\xf7\xbe\xb7\x2e\x91\x9c\xf8\x22\xed\x34\xf1",
        ),
    ),
    // BNB Chain — distinct deployment.
    (
        56,
        Address::new(
            *b"\x7b\x8a\x01\xb3\x9d\x58\x27\x8b\x5d\xe7\xe4\x8c\x84\x49\xc9\xf4\xf5\x17\x06\x13",
        ),
    ),
    // Avalanche C-Chain — distinct deployment.
    (
        43114,
        Address::new(
            *b"\x65\x5c\x40\x6e\xbf\xa1\x4e\xe2\x00\x62\x50\x92\x5e\x54\xec\x43\xad\x18\x4f\x8b",
        ),
    ),
    // Blast — distinct deployment.
    (
        81457,
        Address::new(
            *b"\xb2\x18\xe4\xf7\xcf\x05\x33\xd4\x69\x6f\xdf\xc4\x19\xa0\x02\x3d\x33\x34\x5f\x28",
        ),
    ),
    // Celo — distinct deployment.
    (
        42220,
        Address::new(
            *b"\x3d\x79\xed\xaa\xbc\x0e\xab\x6f\x08\xed\x88\x5c\x05\xfc\x0b\x01\x42\x90\xd9\x5a",
        ),
    ),
    // Zora — distinct deployment.
    (
        7777777,
        Address::new(
            *b"\xbc\x91\xe8\xdf\xa3\xff\x18\xde\x43\x85\x33\x72\xa3\xd7\xdf\xe5\x85\x13\x7d\x78",
        ),
    ),
    // Ink — distinct deployment.
    (
        57073,
        Address::new(
            *b"\xc0\x83\x6e\x5b\x05\x8b\xbe\x22\xae\x22\x66\xe1\xac\x48\x8a\x1a\x0f\xd8\xdc\xe8",
        ),
    ),
    // Unichain — distinct deployment.
    (
        130,
        Address::new(
            *b"\x94\x3e\x6e\x07\xa7\xe8\xe7\x91\xda\xfc\x44\x08\x3e\x54\x04\x1d\x74\x3c\x46\xe9",
        ),
    ),
    // World Chain — distinct deployment.
    (
        480,
        Address::new(
            *b"\xec\x12\xa9\xf9\xa0\x9f\x50\x55\x06\x86\x36\x37\x66\xcc\x15\x3d\x03\xc2\x7b\x5e",
        ),
    ),
];

/// Per-chain Uniswap V4 PositionManager addresses (different per chain — V4
/// is fresh-deployed without CREATE2 colocation). Covers the 13 UR-supported
/// chains.
///
/// Source: https://github.com/Uniswap/contracts/tree/main/deployments/json
/// (`PositionManager` field per chain `<chainId>.json`).
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
    // BNB Chain
    (
        56,
        Address::new(
            *b"\x7a\x4a\x5c\x91\x9a\xe2\x54\x1a\xed\x11\x04\x1a\x1a\xee\xe6\x8f\x12\x87\xf9\x5b",
        ),
    ),
    // Celo
    (
        42220,
        Address::new(
            *b"\xf7\x96\x5f\x39\x81\xe4\xd5\xbc\x38\x3b\xfb\xcb\x61\x50\x17\x63\xe9\x06\x8c\xa9",
        ),
    ),
    // Avalanche C-Chain
    (
        43114,
        Address::new(
            *b"\xb7\x4b\x1f\x14\xd2\x75\x4a\xcf\xcb\xbe\x1a\x22\x10\x23\xa5\xcf\x50\xab\x8a\xcd",
        ),
    ),
    // Ink
    (
        57073,
        Address::new(
            *b"\x1b\x35\xd1\x3a\x2e\x25\x28\xf1\x92\x63\x7f\x14\xb0\x5f\x0d\xc0\xe7\xde\xb5\x66",
        ),
    ),
    // Unichain
    (
        130,
        Address::new(
            *b"\x45\x29\xa0\x1c\x7a\x04\x10\x16\x7c\x57\x40\xc4\x87\xa8\xde\x60\x23\x26\x17\xbf",
        ),
    ),
    // World Chain
    (
        480,
        Address::new(
            *b"\xc5\x85\xe0\xf5\x04\x61\x3b\x5f\xbf\x87\x4f\x21\xaf\x14\xc6\x52\x60\xfb\x41\xfa",
        ),
    ),
    // Zora
    (
        7777777,
        Address::new(
            *b"\xf6\x6c\x7b\x99\xe2\x04\x0f\x0d\x9b\x32\x6b\x3b\x7c\x15\x2e\x96\x63\x54\x3d\x63",
        ),
    ),
];

/// Per-chain Uniswap V4 **PoolManager** addresses — the singleton flash-
/// accounting contract that owns all V4 pools. Distinct from the
/// PositionManager ([`V4_PM_ADDRESSES`]); used to fill `pool.address` for
/// `initialize_pool` / `donate` envelopes and the UR `V4_INITIALIZE_POOL`
/// opcode. Covers the 13 UR-supported chains.
///
/// V4 is fresh-deployed without CREATE2 colocation, so addresses differ per
/// chain — except Arbitrum One (42161) and Ink (57073) which share a CREATE2
/// address (`0x360e68fa…fb32`), and Unichain (130) whose address uses a
/// `0x1f984000…0004` vanity prefix.
///
/// Source: https://github.com/Uniswap/contracts/tree/main/deployments/json
/// (`PoolManager` field per chain `<chainId>.json`).
const V4_POOL_MANAGER_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet
    (
        1,
        Address::new(
            *b"\x00\x00\x00\x00\x00\x04\x44\x4c\x5d\xc7\x5c\xb3\x58\x38\x0d\x2e\x3d\xe0\x8a\x90",
        ),
    ),
    // Base
    (
        8453,
        Address::new(
            *b"\x49\x85\x81\xff\x71\x89\x22\xc3\xf8\xe6\xa2\x44\x95\x6a\xf0\x99\xb2\x65\x2b\x2b",
        ),
    ),
    // Optimism
    (
        10,
        Address::new(
            *b"\x9a\x13\xf9\x8c\xb9\x87\x69\x4c\x9f\x08\x6b\x1f\x5e\xb9\x90\xee\xa8\x26\x4e\xc3",
        ),
    ),
    // Arbitrum One
    (
        42161,
        Address::new(
            *b"\x36\x0e\x68\xfa\xcc\xca\x8c\xa4\x95\xc1\xb7\x59\xfd\x9e\xee\x46\x6d\xb9\xfb\x32",
        ),
    ),
    // Polygon
    (
        137,
        Address::new(
            *b"\x67\x36\x67\x82\x80\x58\x70\x06\x01\x51\x38\x3f\x4b\xbf\xf9\xda\xb5\x3e\x5c\xd6",
        ),
    ),
    // Avalanche C-Chain
    (
        43114,
        Address::new(
            *b"\x06\x38\x0c\x0e\x09\x12\x31\x2b\x51\x50\x36\x4b\x9d\xc4\x54\x2b\xa0\xdb\xbc\x85",
        ),
    ),
    // Blast
    (
        81457,
        Address::new(
            *b"\x16\x31\x55\x91\x98\xa9\xe4\x74\x03\x34\x33\xb2\x95\x8d\xab\xc1\x35\xab\x64\x46",
        ),
    ),
    // BNB Chain
    (
        56,
        Address::new(
            *b"\x28\xe2\xea\x09\x08\x77\xbf\x75\x74\x05\x58\xf6\xbf\xb3\x6a\x5f\xfe\xe9\xe9\xdf",
        ),
    ),
    // Celo
    (
        42220,
        Address::new(
            *b"\x28\x8d\xc8\x41\xa5\x2f\xca\x27\x07\xc6\x94\x7b\x3a\x77\x7c\x5e\x56\xcd\x87\xbc",
        ),
    ),
    // Ink (CREATE2-shared with Arbitrum One)
    (
        57073,
        Address::new(
            *b"\x36\x0e\x68\xfa\xcc\xca\x8c\xa4\x95\xc1\xb7\x59\xfd\x9e\xee\x46\x6d\xb9\xfb\x32",
        ),
    ),
    // Unichain
    (
        130,
        Address::new(
            *b"\x1f\x98\x40\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04",
        ),
    ),
    // World Chain
    (
        480,
        Address::new(
            *b"\xb1\x86\x0d\x52\x91\x82\xac\x3b\xc1\xf5\x1f\xa2\xab\xd5\x66\x62\xb7\xd1\x3f\x33",
        ),
    ),
    // Zora
    (
        7777777,
        Address::new(
            *b"\x05\x75\x33\x8e\x4c\x17\x00\x6a\xe1\x81\xb4\x79\x00\xa8\x44\x04\x24\x7c\xa3\x0f",
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

/// Look up the V4 PoolManager (singleton flash-accounting contract) address
/// for `chain_id`. Returns `None` for chains without a known V4 deployment so
/// callers don't fabricate a `pool.address` for an untrusted chain.
#[must_use]
pub fn v4_pool_manager_address(chain_id: u64) -> Option<Address> {
    V4_POOL_MANAGER_ADDRESSES
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
        // 0x10 (V4_SWAP) and 0x21 (EXECUTE_SUB_PLAN) both have a registered
        // `(bytes, bytes[])` schema. Feeding a 1-byte malformed `inputs[i]`
        // exercises the AbiDecode failure path: Tier B keeps the opcode name
        // but reports `args = None`. T-B1.3 added a `(bytes data)` schema for
        // 0x11/0x12/0x14 so they now decode cleanly given a well-formed blob;
        // this test deliberately uses opcodes that still surface as decode
        // failures so the label-only fallback remains exercised.
        let commands = vec![0x10, 0x21]; // V4_SWAP, EXECUTE_SUB_PLAN
        let inputs = vec![vec![0x00], vec![0x01]];
        let steps = dispatch(&commands, &inputs, &UNISWAP_UR_TABLE);
        assert_eq!(steps[0].name, "V4_SWAP");
        assert_eq!(steps[1].name, "EXECUTE_SUB_PLAN");
        assert!(steps[0].args.is_none());
        assert!(steps[1].args.is_none());
    }

    /// After T-B1.3, `0x11 V3_POSITION_MANAGER_PERMIT`,
    /// `0x12 V3_POSITION_MANAGER_CALL`, and `0x14 V4_POSITION_MANAGER_CALL`
    /// each carry a single `(bytes data)` arg. A well-formed input MUST
    /// decode cleanly and yield `args.len() == 1` with the inner blob.
    #[test]
    fn position_manager_opcodes_decode_bytes_data() {
        // Synthetic NPM calldata: 4-byte selector + 32-byte zero-padded arg.
        let inner_calldata: Vec<u8> = {
            let mut v = vec![0x12, 0x34, 0x56, 0x78];
            v.extend_from_slice(&[0u8; 32]);
            v
        };
        let input = {
            // ABI-encode `(bytes data)` for the inner calldata.
            let func = Function::parse("step(bytes)").unwrap();
            let values = vec![DynSolValue::Bytes(inner_calldata.clone())];
            let raw = func.abi_encode_input(&values).unwrap();
            raw[4..].to_vec()
        };

        for opcode in [0x11u8, 0x12, 0x14] {
            let steps = dispatch(&[opcode], std::slice::from_ref(&input), &UNISWAP_UR_TABLE);
            assert_eq!(steps.len(), 1, "opcode {opcode:#04x}");
            let args = steps[0]
                .args
                .as_ref()
                .expect("position manager opcode must ABI-decode `(bytes data)`");
            assert_eq!(args.len(), 1);
            assert_eq!(args[0].name, "data");
            let alloy_dyn_abi::DynSolValue::Bytes(inner) = &args[0].value else {
                panic!("expected Bytes, got {:?}", args[0].value);
            };
            assert_eq!(inner, &inner_calldata);
        }
    }

    /// Cross-check that every Uniswap deploy-addresses entry added in T-B1.1
    /// (Avalanche / BNB / Blast / Celo / Ink / Unichain / World Chain / Zora)
    /// is recognised by `is_uniswap_universal_router`. Byte literals are
    /// re-derived from the published hex strings here so any one-byte typo in
    /// `UNISWAP_UR_ADDRESSES` is caught at compile or test time.
    #[test]
    fn is_uniswap_universal_router_recognizes_extended_deployments() {
        let extended: &[(u64, [u8; 20])] = &[
            // Avalanche — UniversalRouterV1_2_V2Support
            (
                43114,
                *b"\x4d\xae\x2f\x93\x9a\xcf\x50\x40\x8e\x13\xd5\x85\x34\xff\x8c\x27\x76\xd4\x52\x65",
            ),
            // BNB Chain — UniversalRouterV1_2_V2Support (shared CREATE2 with Avalanche)
            (
                56,
                *b"\x4d\xae\x2f\x93\x9a\xcf\x50\x40\x8e\x13\xd5\x85\x34\xff\x8c\x27\x76\xd4\x52\x65",
            ),
            // BNB Chain — UniversalRouterV2
            (
                56,
                *b"\x19\x06\xc1\xd6\x72\xb8\x8c\xd1\xb9\xac\x75\x93\x30\x1c\xa9\x90\xf9\x4e\xae\x07",
            ),
            // Blast — UniversalRouterV1_2_V2Support
            (
                81457,
                *b"\x64\x37\x70\xe2\x79\xd5\xd0\x73\x3f\x21\xd6\xdc\x03\xa8\xef\xba\xbf\x32\x55\xb4",
            ),
            // Blast — UniversalRouterV2
            (
                81457,
                *b"\xea\xbb\xcb\x3e\x8e\x41\x53\x06\x20\x7e\xf5\x14\xf6\x60\xa3\xf8\x20\x02\x5b\xe3",
            ),
            // Celo — UniversalRouterV1_2_V2Support (shared CREATE2 with Blast)
            (
                42220,
                *b"\x64\x37\x70\xe2\x79\xd5\xd0\x73\x3f\x21\xd6\xdc\x03\xa8\xef\xba\xbf\x32\x55\xb4",
            ),
            // Ink — UniversalRouterV2
            (
                57073,
                *b"\x11\x29\x08\xda\xc8\x6e\x20\xe7\x24\x1b\x09\x27\x47\x9e\xa3\xbf\x93\x5d\x1f\xa0",
            ),
            // Unichain — UniversalRouterV2
            (
                130,
                *b"\xef\x74\x0b\xf2\x3a\xca\xe2\x6f\x64\x92\xb1\x0d\xe6\x45\xd6\xb9\x8d\xc8\xea\xf3",
            ),
            // World Chain — UniversalRouter (corrected; deployments/480.md)
            (
                480,
                *b"\x03\xc4\xf6\xb5\x57\x33\xcd\xf3\xca\xa0\x7c\x01\xe5\xb8\x3d\xde\xe3\x38\x1f\x60",
            ),
            // World Chain — UniversalRouterV2
            (
                480,
                *b"\x8a\xc7\xbe\xe9\x93\xbb\x44\xda\xb5\x64\xea\x4b\xc9\xea\x67\xbf\x9e\xb5\xe7\x43",
            ),
            // Zora — UniversalRouterV2
            (
                7777777,
                *b"\x33\x15\xef\x7c\xa2\x8d\xb7\x4a\xba\xdc\x6c\x44\x57\x0e\xfd\xf0\x6b\x04\xb0\x20",
            ),
        ];
        assert_eq!(extended.len(), 11, "T-B1.1 expects exactly 11 new entries");
        for (chain_id, raw) in extended.iter().copied() {
            let addr = Address::from(raw);
            assert!(
                is_uniswap_universal_router(chain_id, &addr),
                "chain {chain_id} address {addr:?} not recognised",
            );
        }
    }

    /// Sanity-check the total deployment count after T-B1.1: the original
    /// 10 entries (mainnet 2 + Base 2 + Optimism 2 + Arbitrum 2 + Polygon 2)
    /// plus 11 new ones = 21.
    #[test]
    fn uniswap_universal_router_deployments_count() {
        let total = uniswap_universal_router_deployments().count();
        assert_eq!(total, 21, "expected 10 baseline + 11 T-B1.1 entries");
    }

    /// F-P0.2: cross-check that every V3 NonfungiblePositionManager entry
    /// added in this fix is recognized by `v3_position_manager_address`. The
    /// table was previously mainnet-only, leaving the other 12 UR-supported
    /// chains with `None` and faulting `0x11` / `0x12` recursion.
    ///
    /// Source: https://github.com/Uniswap/contracts/tree/main/deployments
    /// (`v3NFTPositionManager` field per chain).
    #[test]
    fn v3_position_manager_address_recognizes_create2_shared_chains() {
        // mainnet/Optimism/Arbitrum/Polygon all share the same CREATE2 NFPM.
        let create2_addr: [u8; 20] =
            *b"\xc3\x64\x42\xb4\xa4\x52\x2e\x87\x13\x99\xcd\x71\x7a\xbd\xd8\x47\xab\x11\xfe\x88";
        for chain_id in [1u64, 10, 137, 42161] {
            let resolved = v3_position_manager_address(chain_id);
            assert_eq!(
                resolved,
                Some(Address::from(create2_addr)),
                "chain {chain_id} should map to CREATE2-shared NFPM address",
            );
        }
    }

    #[test]
    fn v3_position_manager_address_recognizes_base() {
        let expected: [u8; 20] =
            *b"\x03\xa5\x20\xb3\x2c\x04\xbf\x3b\xee\xf7\xbe\xb7\x2e\x91\x9c\xf8\x22\xed\x34\xf1";
        assert_eq!(
            v3_position_manager_address(8453),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_bnb() {
        let expected: [u8; 20] =
            *b"\x7b\x8a\x01\xb3\x9d\x58\x27\x8b\x5d\xe7\xe4\x8c\x84\x49\xc9\xf4\xf5\x17\x06\x13";
        assert_eq!(
            v3_position_manager_address(56),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_avalanche() {
        let expected: [u8; 20] =
            *b"\x65\x5c\x40\x6e\xbf\xa1\x4e\xe2\x00\x62\x50\x92\x5e\x54\xec\x43\xad\x18\x4f\x8b";
        assert_eq!(
            v3_position_manager_address(43114),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_blast() {
        let expected: [u8; 20] =
            *b"\xb2\x18\xe4\xf7\xcf\x05\x33\xd4\x69\x6f\xdf\xc4\x19\xa0\x02\x3d\x33\x34\x5f\x28";
        assert_eq!(
            v3_position_manager_address(81457),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_celo() {
        let expected: [u8; 20] =
            *b"\x3d\x79\xed\xaa\xbc\x0e\xab\x6f\x08\xed\x88\x5c\x05\xfc\x0b\x01\x42\x90\xd9\x5a";
        assert_eq!(
            v3_position_manager_address(42220),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_zora() {
        let expected: [u8; 20] =
            *b"\xbc\x91\xe8\xdf\xa3\xff\x18\xde\x43\x85\x33\x72\xa3\xd7\xdf\xe5\x85\x13\x7d\x78";
        assert_eq!(
            v3_position_manager_address(7777777),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_ink() {
        let expected: [u8; 20] =
            *b"\xc0\x83\x6e\x5b\x05\x8b\xbe\x22\xae\x22\x66\xe1\xac\x48\x8a\x1a\x0f\xd8\xdc\xe8";
        assert_eq!(
            v3_position_manager_address(57073),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_unichain() {
        let expected: [u8; 20] =
            *b"\x94\x3e\x6e\x07\xa7\xe8\xe7\x91\xda\xfc\x44\x08\x3e\x54\x04\x1d\x74\x3c\x46\xe9";
        assert_eq!(
            v3_position_manager_address(130),
            Some(Address::from(expected))
        );
    }

    #[test]
    fn v3_position_manager_address_recognizes_world_chain() {
        let expected: [u8; 20] =
            *b"\xec\x12\xa9\xf9\xa0\x9f\x50\x55\x06\x86\x36\x37\x66\xcc\x15\x3d\x03\xc2\x7b\x5e";
        assert_eq!(
            v3_position_manager_address(480),
            Some(Address::from(expected))
        );
    }

    /// F-P0.2: ensure unknown chains still return `None` so we don't blindly
    /// recurse against untrusted targets.
    #[test]
    fn v3_position_manager_address_rejects_unknown_chain() {
        // Picked arbitrary chain ids that are NOT in the V3 NFPM allow-list.
        for chain_id in [0u64, 5, 100, 11_111] {
            assert_eq!(
                v3_position_manager_address(chain_id),
                None,
                "chain {chain_id} should not map to any V3 NFPM",
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // TB-4 — V4 PositionManager / PoolManager 13-chain address tables.
    // Byte literals are re-derived from the published hex strings here so a
    // one-byte typo in the address tables is caught at compile / test time.
    // Source: Uniswap/contracts deployments/json/<chainId>.json.
    // ─────────────────────────────────────────────────────────────────────

    /// TB-4: every V4 PositionManager entry resolves. The table was 6-chain
    /// (mainnet/Optimism/Polygon/Arbitrum/Base/Blast) before TB-4; the 7 new
    /// entries are asserted explicitly so a regression is attributed here.
    #[test]
    fn v4_position_manager_address_covers_thirteen_chains() {
        let expected: &[(u64, [u8; 20])] = &[
            // pre-TB-4 baseline 6.
            (
                1,
                *b"\xbd\x21\x65\x13\xd7\x4c\x8c\xf1\x4c\xf4\x74\x7e\x6a\xaa\x64\x20\xff\x64\xee\x9e",
            ),
            (
                10,
                *b"\x3c\x3e\xa4\xb5\x7a\x46\x24\x1e\x54\x61\x0e\x5f\x02\x2e\x5c\x45\x85\x9a\x10\x17",
            ),
            (
                137,
                *b"\x1e\xc2\xeb\xf4\xf3\x7e\x73\x63\xfd\xfe\x35\x51\x60\x24\x25\xaf\x0b\x3c\xee\xf9",
            ),
            (
                42161,
                *b"\xd8\x8f\x38\xf9\x30\xb7\x95\x2f\x2d\xb2\x43\x2c\xb0\x02\xe7\xab\xbf\x3d\xd8\x69",
            ),
            (
                8453,
                *b"\x7c\x5f\x5a\x4b\xbd\x8f\xd6\x31\x84\x57\x75\x25\x32\x61\x23\xb5\x19\x42\x9b\xdc",
            ),
            (
                81457,
                *b"\x4a\xd2\xf4\xcc\xa2\x68\x2c\xbb\x5b\x95\x0d\x66\x0d\xd4\x58\xa1\xd3\xf1\xba\xad",
            ),
            // TB-4 new 7.
            (
                56,
                *b"\x7a\x4a\x5c\x91\x9a\xe2\x54\x1a\xed\x11\x04\x1a\x1a\xee\xe6\x8f\x12\x87\xf9\x5b",
            ),
            (
                42220,
                *b"\xf7\x96\x5f\x39\x81\xe4\xd5\xbc\x38\x3b\xfb\xcb\x61\x50\x17\x63\xe9\x06\x8c\xa9",
            ),
            (
                43114,
                *b"\xb7\x4b\x1f\x14\xd2\x75\x4a\xcf\xcb\xbe\x1a\x22\x10\x23\xa5\xcf\x50\xab\x8a\xcd",
            ),
            (
                57073,
                *b"\x1b\x35\xd1\x3a\x2e\x25\x28\xf1\x92\x63\x7f\x14\xb0\x5f\x0d\xc0\xe7\xde\xb5\x66",
            ),
            (
                130,
                *b"\x45\x29\xa0\x1c\x7a\x04\x10\x16\x7c\x57\x40\xc4\x87\xa8\xde\x60\x23\x26\x17\xbf",
            ),
            (
                480,
                *b"\xc5\x85\xe0\xf5\x04\x61\x3b\x5f\xbf\x87\x4f\x21\xaf\x14\xc6\x52\x60\xfb\x41\xfa",
            ),
            (
                7777777,
                *b"\xf6\x6c\x7b\x99\xe2\x04\x0f\x0d\x9b\x32\x6b\x3b\x7c\x15\x2e\x96\x63\x54\x3d\x63",
            ),
        ];
        assert_eq!(expected.len(), 13, "TB-4 expects all 13 chains covered");
        for (chain_id, raw) in expected.iter().copied() {
            assert_eq!(
                v4_position_manager_address(chain_id),
                Some(Address::from(raw)),
                "chain {chain_id} V4 PositionManager address mismatch",
            );
        }
    }

    /// TB-4: every V4 PoolManager entry resolves. The table is brand-new in
    /// TB-4 — all 13 chains asserted.
    #[test]
    fn v4_pool_manager_address_covers_thirteen_chains() {
        let expected: &[(u64, [u8; 20])] = &[
            (
                1,
                *b"\x00\x00\x00\x00\x00\x04\x44\x4c\x5d\xc7\x5c\xb3\x58\x38\x0d\x2e\x3d\xe0\x8a\x90",
            ),
            (
                8453,
                *b"\x49\x85\x81\xff\x71\x89\x22\xc3\xf8\xe6\xa2\x44\x95\x6a\xf0\x99\xb2\x65\x2b\x2b",
            ),
            (
                10,
                *b"\x9a\x13\xf9\x8c\xb9\x87\x69\x4c\x9f\x08\x6b\x1f\x5e\xb9\x90\xee\xa8\x26\x4e\xc3",
            ),
            (
                42161,
                *b"\x36\x0e\x68\xfa\xcc\xca\x8c\xa4\x95\xc1\xb7\x59\xfd\x9e\xee\x46\x6d\xb9\xfb\x32",
            ),
            (
                137,
                *b"\x67\x36\x67\x82\x80\x58\x70\x06\x01\x51\x38\x3f\x4b\xbf\xf9\xda\xb5\x3e\x5c\xd6",
            ),
            (
                43114,
                *b"\x06\x38\x0c\x0e\x09\x12\x31\x2b\x51\x50\x36\x4b\x9d\xc4\x54\x2b\xa0\xdb\xbc\x85",
            ),
            (
                81457,
                *b"\x16\x31\x55\x91\x98\xa9\xe4\x74\x03\x34\x33\xb2\x95\x8d\xab\xc1\x35\xab\x64\x46",
            ),
            (
                56,
                *b"\x28\xe2\xea\x09\x08\x77\xbf\x75\x74\x05\x58\xf6\xbf\xb3\x6a\x5f\xfe\xe9\xe9\xdf",
            ),
            (
                42220,
                *b"\x28\x8d\xc8\x41\xa5\x2f\xca\x27\x07\xc6\x94\x7b\x3a\x77\x7c\x5e\x56\xcd\x87\xbc",
            ),
            (
                57073,
                *b"\x36\x0e\x68\xfa\xcc\xca\x8c\xa4\x95\xc1\xb7\x59\xfd\x9e\xee\x46\x6d\xb9\xfb\x32",
            ),
            (
                130,
                *b"\x1f\x98\x40\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x04",
            ),
            (
                480,
                *b"\xb1\x86\x0d\x52\x91\x82\xac\x3b\xc1\xf5\x1f\xa2\xab\xd5\x66\x62\xb7\xd1\x3f\x33",
            ),
            (
                7777777,
                *b"\x05\x75\x33\x8e\x4c\x17\x00\x6a\xe1\x81\xb4\x79\x00\xa8\x44\x04\x24\x7c\xa3\x0f",
            ),
        ];
        assert_eq!(expected.len(), 13, "TB-4 expects all 13 chains covered");
        for (chain_id, raw) in expected.iter().copied() {
            assert_eq!(
                v4_pool_manager_address(chain_id),
                Some(Address::from(raw)),
                "chain {chain_id} V4 PoolManager address mismatch",
            );
        }
    }

    /// TB-4: Arbitrum One (42161) and Ink (57073) share the same CREATE2 V4
    /// PoolManager address. This colocation is intentional — assert it so a
    /// future table edit that diverges them is flagged.
    #[test]
    fn v4_pool_manager_address_arbitrum_ink_share_create2() {
        let shared: [u8; 20] =
            *b"\x36\x0e\x68\xfa\xcc\xca\x8c\xa4\x95\xc1\xb7\x59\xfd\x9e\xee\x46\x6d\xb9\xfb\x32";
        assert_eq!(v4_pool_manager_address(42161), Some(Address::from(shared)));
        assert_eq!(v4_pool_manager_address(57073), Some(Address::from(shared)));
    }

    /// TB-4: unknown chains return `None` for both V4 tables so callers never
    /// fabricate an address for an untrusted chain.
    #[test]
    fn v4_address_tables_reject_unknown_chain() {
        for chain_id in [0u64, 5, 100, 11_111, 999_999] {
            assert_eq!(
                v4_position_manager_address(chain_id),
                None,
                "chain {chain_id} should not map to any V4 PositionManager",
            );
            assert_eq!(
                v4_pool_manager_address(chain_id),
                None,
                "chain {chain_id} should not map to any V4 PoolManager",
            );
        }
    }
}
