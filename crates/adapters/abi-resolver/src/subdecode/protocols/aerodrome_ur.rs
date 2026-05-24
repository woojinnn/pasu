//! Aerodrome / Velodrome Universal Router opcode table — `main` lineage
//! (opcode-dispatched).
//!
//! Source: `velodrome-finance/universal-router @ main`, HEAD
//! `540899c395004a50179bcce2774882995b3f381c`:
//! - `contracts/libraries/Commands.sol`  — opcode constants (mask `0x3f`)
//! - `contracts/base/Dispatcher.sol`     — input ABI shapes
//!
//! Aerodrome and Velodrome share one `universal-router` repo. The `main`
//! branch is the current Base deployment (`UniversalRouter` in
//! `deployment-addresses/base.json`); an older `v1` lineage with a different
//! opcode set (NFT-marketplace pass-throughs) also exists on Base but is NOT
//! covered by this table — it needs a separate `AERODROME_UR_V1_TABLE`.
//!
//! Key divergences from Uniswap UR:
//!
//! - `mask = 0x3f` (Uniswap is `0x7f`) — opcodes only use the low 6 bits.
//!   Mixing this table with `UNISWAP_UR_TABLE` would mask wrong.
//! - `V2_SWAP_*` (`0x08`/`0x09`) on `main` uses a **packed `bytes path`**
//!   plus a trailing `bool isUni` flag (`isUni=true` → Uniswap V2/V3 pools,
//!   `isUni=false` → Velodrome/Aerodrome AMM pools) — not a `Route[]` struct
//!   array as on the `v1` branch. `V3_SWAP_*` shares the same 6-tuple shape.
//! - `0x10`–`0x13` are V4 swap / V4 pool init / cross-chain bridge opcodes
//!   (the `v1` branch uses these slots for NFT marketplaces instead).
//!
//! Selectors `execute(bytes,bytes[],uint256)` (`0x3593564c`) and
//! `execute(bytes,bytes[])` (`0x24856bc3`) are identical to Uniswap UR — they
//! disambiguate by router `to` address (see [`is_aerodrome_universal_router`]).
//!
//! Placeholder opcodes (`0x0f`, `0x14`–`0x20`, `0x22`–`0x3f`) are
//! intentionally omitted: the dispatcher reverts on them, and the engine
//! falls back to `UNKNOWN(raw …)` which is the correct under-decode
//! behaviour.

use alloy_primitives::Address;

use crate::subdecode::opcode_stream::{OpcodeEntry, OpcodeTable};

/// Aerodrome UR command bytes use the high bit (`0x80`) for `allowRevert`;
/// the opcode mask is `0x3f` (`Commands.COMMAND_TYPE_MASK`).
pub const AERODROME_UR_MASK: u8 = 0x3f;
/// Bit set on a command byte when the dispatcher should tolerate revert
/// (`Commands.FLAG_ALLOW_REVERT`).
pub const AERODROME_UR_ALLOW_REVERT: u8 = 0x80;

/// Aerodrome / Velodrome UR (`main` lineage) opcode dispatch table.
pub static AERODROME_UR_MAIN_TABLE: OpcodeTable = OpcodeTable {
    mask: AERODROME_UR_MASK,
    allow_revert_bit: AERODROME_UR_ALLOW_REVERT,
    entries: ENTRIES,
};

const ENTRIES: &[OpcodeEntry] = &[
    // ---- 0x00..=0x01 — V3 / CL swaps (packed `bytes path` + `isUni`) ----
    // The `bytes path` is packed (token ++ poolParam ++ token …); `isUni`
    // selects Uniswap V3 vs Velodrome/Aerodrome Slipstream pools. Inner
    // path unfolding is a Tier A BuiltinFn concern, not handled here.
    OpcodeEntry {
        opcode: 0x00,
        name: "V3_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser, bool isUni)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x01,
        name: "V3_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bool payerIsUser, bool isUni)",
        ],
        input_json_abi: None,
    },
    // ---- 0x02..=0x07 — Permit2 transfer / payments ----
    OpcodeEntry {
        opcode: 0x02,
        name: "PERMIT2_TRANSFER_FROM",
        input_signatures: &["(address token, address recipient, uint160 amount)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x03,
        name: "PERMIT2_PERMIT_BATCH",
        input_signatures: &[],
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
    OpcodeEntry {
        opcode: 0x07,
        name: "TRANSFER_FROM",
        input_signatures: &["(address token, address recipient, uint256 value)"],
        input_json_abi: None,
    },
    // ---- 0x08..=0x09 — V2 swaps (packed `bytes path` + `isUni`) ----
    // `main` lineage uses packed bytes (UniV2 = bare address sequence /
    // VeloV2 = token ++ stable(1B) ++ token), NOT a `Route[]` struct array.
    OpcodeEntry {
        opcode: 0x08,
        name: "V2_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser, bool isUni)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x09,
        name: "V2_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bool payerIsUser, bool isUni)",
        ],
        input_json_abi: None,
    },
    // ---- 0x0a..=0x0e — Permit2 single / wrap / balance check ----
    OpcodeEntry {
        opcode: 0x0a,
        name: "PERMIT2_PERMIT",
        input_signatures: &[],
        input_json_abi: Some(PERMIT2_PERMIT_JSON),
    },
    OpcodeEntry {
        opcode: 0x0b,
        name: "WRAP_ETH",
        input_signatures: &["(address recipient, uint256 amount)"],
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
        input_signatures: &[],
        input_json_abi: Some(PERMIT2_TRANSFER_FROM_BATCH_JSON),
    },
    OpcodeEntry {
        opcode: 0x0e,
        name: "BALANCE_CHECK_ERC20",
        input_signatures: &["(address owner, address token, uint256 minBalance)"],
        input_json_abi: None,
    },
    // 0x0f — placeholder (dispatcher reverts); omitted → UNKNOWN fallback.

    // ---- 0x10..=0x13 — V4 swap / V4 pool init / cross-chain bridge ----
    OpcodeEntry {
        opcode: 0x10,
        name: "V4_SWAP",
        // Top-level shape `(bytes actions, bytes[] params)` — same as
        // Uniswap UR's V4_SWAP. The inner V4 action stream is dispatched by
        // a V4 router opcode table (cross-table recursion), not here.
        input_signatures: &["(bytes actions, bytes[] params)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x11,
        name: "V4_INITIALIZE_POOL",
        input_signatures: &[
            "((address,address,uint24,int24,address) poolKey, uint160 sqrtPriceX96)",
        ],
        input_json_abi: Some(V4_INITIALIZE_POOL_JSON),
    },
    OpcodeEntry {
        opcode: 0x12,
        name: "BRIDGE_TOKEN",
        input_signatures: &[
            "(uint8 bridgeType, address recipient, address token, address bridge, uint256 amount, uint256 msgFee, uint32 domain, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x13,
        name: "EXECUTE_CROSS_CHAIN",
        input_signatures: &[
            "(uint32 domain, address icaRouter, bytes32 remoteRouter, bytes32 ism, bytes32 commitment, uint256 msgFee, address hook, bytes hookMetadata)",
        ],
        input_json_abi: None,
    },
    // 0x14..=0x20 — placeholders (dispatcher reverts); omitted.

    // ---- 0x21 EXECUTE_SUB_PLAN — nested `(bytes,bytes[])` self-recursion ----
    OpcodeEntry {
        opcode: 0x21,
        name: "EXECUTE_SUB_PLAN",
        input_signatures: &["(bytes commands, bytes[] inputs)"],
        input_json_abi: None,
    },
    // 0x22..=0x3f — placeholders (dispatcher reverts); omitted.
];

// JSON-ABI literals — Permit2 + V4 PoolKey types are vendor-neutral, copied
// verbatim from `universal_router.rs` (mirrors how `pancake_ur.rs` does it).

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

/// Returns true when `(chain_id, target)` matches a known Aerodrome /
/// Velodrome `main`-lineage UR deployment that we trust to use the
/// [`AERODROME_UR_MAIN_TABLE`] dispatch shape.
///
/// New addresses can be added here as they're verified against upstream
/// `velodrome-finance/universal-router`.
#[must_use]
pub fn is_aerodrome_universal_router(chain_id: u64, target: &Address) -> bool {
    AERODROME_UR_ADDRESSES
        .iter()
        .any(|(chain, addr)| *chain == chain_id && addr == target)
}

/// Iterator over known Aerodrome UR deployments as `(chain_id, address)`
/// pairs. Mirrors `universal_router::uniswap_universal_router_deployments()`.
pub fn aerodrome_universal_router_deployments() -> impl Iterator<Item = (u64, Address)> {
    AERODROME_UR_ADDRESSES.iter().copied()
}

/// Allowlist of Aerodrome / Velodrome `main`-lineage Universal Router
/// deployments — `(chain_id, address)`.
///
/// Source: `velodrome-finance/universal-router @ main`
/// `deployment-addresses/base.json` (`0xC5b6786D…`) plus a BaseScan-verified
/// `main`-lineage deployment (`0xcAF22ce3…`).
const AERODROME_UR_ADDRESSES: &[(u64, Address)] = &[
    // Base — `deployment-addresses/base.json` `UniversalRouter` (current).
    (
        8453,
        Address::new(
            *b"\xc5\xb6\x78\x6d\x7b\x64\x76\x7d\x77\x58\x77\xb0\xb6\xa3\x19\xad\x94\x6b\x11\xb5",
        ),
    ),
    // Base — BaseScan-verified `main`-lineage UniversalRouter deployment.
    (
        8453,
        Address::new(
            *b"\xca\xf2\x2c\xe3\x12\x98\xcf\x2b\xf1\xd1\x52\x86\x2f\x80\x21\x64\x78\xad\x7c\x67",
        ),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::opcode_stream::dispatch;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    #[test]
    fn mask_extracts_six_bit_opcode() {
        // 0x80 | 0x0b = allowRevert + WRAP_ETH; mask 0x3f → 0x0b.
        let raw = 0x80u8 | 0x0b;
        assert_eq!(raw & AERODROME_UR_MASK, 0x0b);
        assert_eq!(raw & AERODROME_UR_ALLOW_REVERT, AERODROME_UR_ALLOW_REVERT);
    }

    #[test]
    fn aerodrome_address_recognised() {
        // `deployment-addresses/base.json` UniversalRouter.
        let a =
            Address::from_slice(&hex::decode("C5b6786D7B64767D775877b0B6A319AD946B11B5").unwrap());
        assert!(is_aerodrome_universal_router(8453, &a));
        // Same address on a different chain → not recognised.
        assert!(!is_aerodrome_universal_router(1, &a));

        // BaseScan-verified `main`-lineage deployment.
        let b =
            Address::from_slice(&hex::decode("cAF22ce31298CF2BF1D152862F80216478ad7c67").unwrap());
        assert!(is_aerodrome_universal_router(8453, &b));
        assert!(!is_aerodrome_universal_router(10, &b));
    }

    #[test]
    fn unknown_address_not_aerodrome() {
        let addr = Address::from([0x42; 20]);
        assert!(!is_aerodrome_universal_router(8453, &addr));
    }

    #[test]
    fn aerodrome_ur_deployments_count() {
        assert_eq!(aerodrome_universal_router_deployments().count(), 2);
    }

    /// Smoke-test that the table dispatches a couple of opcodes against
    /// `AERODROME_UR_MAIN_TABLE`: `0x0b WRAP_ETH` decodes its
    /// `(address,uint256)` input and `0x0c UNWRAP_WETH` likewise.
    #[test]
    fn dispatch_decodes_wrap_then_unwrap() {
        // Build an ABI-encoded `(address,uint256)` blob the way `inputs[i]`
        // looks in real UR calldata (synthetic 4-byte selector sliced off).
        let encode = |addr: [u8; 20], amount: u128| -> Vec<u8> {
            let func = Function::parse("step(address,uint256)").unwrap();
            let values = vec![
                DynSolValue::Address(Address::from(addr)),
                DynSolValue::Uint(U256::from(amount), 256),
            ];
            let raw = func.abi_encode_input(&values).unwrap();
            raw[4..].to_vec()
        };

        let commands = vec![0x0b, 0x0c];
        let inputs = vec![encode([0xaa; 20], 1_000_000), encode([0xbb; 20], 2_000_000)];
        let steps = dispatch(&commands, &inputs, &AERODROME_UR_MAIN_TABLE);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "WRAP_ETH");
        assert_eq!(steps[0].opcode, 0x0b);
        assert!(steps[0].args.is_some());
        assert!(steps[0].error.is_none());
        assert_eq!(steps[1].name, "UNWRAP_WETH");
        assert!(steps[1].args.is_some());
    }

    /// A placeholder opcode (`0x14`, intentionally not in the table) must
    /// fall through to the `UNKNOWN` label — the intended under-decode
    /// behaviour for dispatcher-revert slots.
    #[test]
    fn placeholder_opcode_is_unknown() {
        let steps = dispatch(&[0x14], &[vec![]], &AERODROME_UR_MAIN_TABLE);
        assert_eq!(steps[0].name, "UNKNOWN");
    }
}
