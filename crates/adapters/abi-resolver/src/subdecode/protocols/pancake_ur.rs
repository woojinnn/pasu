//! PancakeSwap (Infinity) Universal Router opcode table (opcode-dispatched).
//!
//! Source: `pancakeswap/infinity-universal-router @ main`:
//! - `src/libraries/Commands.sol`  — opcode constants (mask `0x3f`)
//! - `src/base/Dispatcher.sol`     — input ABI shapes
//!
//! Pancake UR is a fork of the original Uniswap UR. The intersection 0x00-0x0e
//! is identical (with the same input ABIs as Uniswap **v1** — no
//! `minHopPriceX36` trailing array on V2/V3 swaps), and the table stays
//! lock-step with Uniswap on common opcodes. Pancake-specific differences:
//!
//! - `mask = 0x3f` (Uniswap is `0x7f`) — opcodes only use the low 6 bits;
//! - `0x07` is a placeholder (Uniswap uses it for `PAY_PORTION_FULL_PRECISION`);
//! - `0x10–0x16` are PancakeSwap **Infinity** vault / position-manager opcodes
//!   (Uniswap uses these slots for V4 instead). Schemas live in `infinity-core`
//!   and are kept label-only here pending a follow-up.
//! - `0x22 / 0x23` are Pancake stable-swap opcodes — Uniswap leaves these as
//!   placeholders.
//!
//! Selectors `execute(bytes,bytes[],uint256)` (`0x3593564c`) and
//! `execute(bytes,bytes[])` (`0x24856bc3`) are identical to Uniswap UR — they
//! disambiguate by router `to` address (see [`is_pancake_universal_router`]).

use alloy_primitives::Address;

use crate::subdecode::opcode_stream::{OpcodeEntry, OpcodeTable};

/// Pancake UR command bytes use the high bit (`0x80`) for `allowRevert`,
/// matching Uniswap; the opcode mask is `0x3f`.
pub const PANCAKE_UR_MASK: u8 = 0x3f;
/// Bit set on a command byte when the dispatcher should tolerate revert.
pub const PANCAKE_UR_ALLOW_REVERT: u8 = 0x80;

/// Pancake (Infinity) UR opcode dispatch table.
pub static PANCAKE_UR_TABLE: OpcodeTable = OpcodeTable {
    mask: PANCAKE_UR_MASK,
    allow_revert_bit: PANCAKE_UR_ALLOW_REVERT,
    entries: ENTRIES,
};

const ENTRIES: &[OpcodeEntry] = &[
    // ---- 0x00..=0x06 — payments / V3 swaps / Permit2 transfers ----
    OpcodeEntry {
        opcode: 0x00,
        name: "V3_SWAP_EXACT_IN",
        // Pancake UR uses Uniswap v1 shape (no trailing minHopPriceX36).
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x01,
        name: "V3_SWAP_EXACT_OUT",
        input_signatures: &[
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
    // 0x07 — explicit placeholder in Pancake (vs PAY_PORTION_FULL_PRECISION
    // in Uniswap). The dispatcher reverts; we pass through as UNKNOWN. By
    // omitting an entry we also avoid mislabelling an Uniswap-style opcode.

    // ---- 0x08..=0x0e — V2 swaps / Permit2 single / wrap / balance check ----
    OpcodeEntry {
        opcode: 0x08,
        name: "V2_SWAP_EXACT_IN",
        // Pancake's Dispatcher source comments say `(address, uint256, uint256,
        // bytes, bool)` — but the implementation actually decodes `address[]
        // path` like Uniswap. We list both shapes; the `address[]` form is the
        // common case and the `bytes` fallback is harmless since alloy will
        // reject mismatches.
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, address[] path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x09,
        name: "V2_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, address[] path, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x0a,
        name: "PERMIT2_PERMIT",
        input_signatures: &[],
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
        input_signatures: &[],
        input_json_abi: Some(PERMIT2_TRANSFER_FROM_BATCH_JSON),
    },
    OpcodeEntry {
        opcode: 0x0e,
        name: "BALANCE_CHECK_ERC20",
        input_signatures: &["(address owner, address token, uint256 minBalance)"],
        input_json_abi: None,
    },

    // ---- 0x10..=0x16 — PancakeSwap Infinity vault / position-manager ----
    OpcodeEntry {
        opcode: 0x10,
        name: "INFI_SWAP",
        // Top-level shape `(bytes actions, bytes[] params)` matches Uniswap
        // UR's V4_SWAP. Inner action-stream dispatch is handled by the
        // orchestrator against `PANCAKE_INFI_TABLE` (see
        // `crate::subdecode::protocols::pancake_infinity`).
        input_signatures: &["(bytes actions, bytes[] params)"],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x11,
        name: "V3_POSITION_MANAGER_PERMIT",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x12,
        name: "V3_POSITION_MANAGER_CALL",
        input_signatures: &[],
        input_json_abi: None,
    },
    // INFI_CL_INITIALIZE_POOL — `(PoolKey, uint160 sqrtPriceX96)` where
    // PoolKey is the 6-field Pancake Infinity variant.
    OpcodeEntry {
        opcode: 0x13,
        name: "INFI_CL_INITIALIZE_POOL",
        input_signatures: &[],
        input_json_abi: Some(INFI_CL_INITIALIZE_POOL_JSON),
    },
    OpcodeEntry {
        opcode: 0x14,
        name: "INFI_BIN_INITIALIZE_POOL",
        input_signatures: &[],
        input_json_abi: Some(INFI_BIN_INITIALIZE_POOL_JSON),
    },
    OpcodeEntry {
        opcode: 0x15,
        name: "INFI_CL_POSITION_CALL",
        input_signatures: &[],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x16,
        name: "INFI_BIN_POSITION_CALL",
        input_signatures: &[],
        input_json_abi: None,
    },

    // ---- 0x21 EXECUTE_SUB_PLAN — same shape as Uniswap UR ----
    OpcodeEntry {
        opcode: 0x21,
        name: "EXECUTE_SUB_PLAN",
        input_signatures: &["(bytes commands, bytes[] inputs)"],
        input_json_abi: None,
    },

    // ---- 0x22..=0x23 — Pancake stable-swap (Uniswap leaves these placeholders) ----
    // Verified against `pancakeswap/infinity-universal-router @ main`
    // Dispatcher.sol: input is `(address recipient, uint256 amountIn/Out,
    // uint256 amountOutMin/InMax, bytes path, bytes flag, bool payerIsUser)`.
    // `path` and `flag` are both bytes (path encodes the StableSwap path,
    // flag encodes 2pool/3pool route metadata).
    OpcodeEntry {
        opcode: 0x22,
        name: "STABLE_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bytes flag, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x23,
        name: "STABLE_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, bytes path, bytes flag, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
];

// Pancake Infinity PoolKey: 6 fields (currency0, currency1, hooks,
// poolManager, fee, parameters). Different from Uniswap V4 PoolKey.
const INFI_CL_INITIALIZE_POOL_JSON: &str = r#"[
    { "name": "poolKey", "type": "tuple", "components": [
        { "name": "currency0",   "type": "address" },
        { "name": "currency1",   "type": "address" },
        { "name": "hooks",       "type": "address" },
        { "name": "poolManager", "type": "address" },
        { "name": "fee",         "type": "uint24" },
        { "name": "parameters",  "type": "bytes32" }
    ]},
    { "name": "sqrtPriceX96", "type": "uint160" }
]"#;

const INFI_BIN_INITIALIZE_POOL_JSON: &str = r#"[
    { "name": "poolKey", "type": "tuple", "components": [
        { "name": "currency0",   "type": "address" },
        { "name": "currency1",   "type": "address" },
        { "name": "hooks",       "type": "address" },
        { "name": "poolManager", "type": "address" },
        { "name": "fee",         "type": "uint24" },
        { "name": "parameters",  "type": "bytes32" }
    ]},
    { "name": "activeId", "type": "uint24" }
]"#;

// JSON-ABI literals (mirror Uniswap's — Permit2 structs are vendor-neutral).
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

const PERMIT2_TRANSFER_FROM_BATCH_JSON: &str = r#"[
    { "name": "transferDetails", "type": "tuple[]", "components": [
        { "name": "from",   "type": "address" },
        { "name": "to",     "type": "address" },
        { "name": "amount", "type": "uint160" },
        { "name": "token",  "type": "address" }
    ]}
]"#;

/// Returns true when `(chain_id, target)` matches a known PancakeSwap UR
/// deployment that we trust to use the [`PANCAKE_UR_TABLE`] dispatch shape.
///
/// New Pancake UR addresses can be added here as they're verified against
/// upstream `pancakeswap/infinity-universal-router`.
#[must_use]
pub fn is_pancake_universal_router(chain_id: u64, target: &Address) -> bool {
    PANCAKE_UR_ADDRESSES
        .iter()
        .any(|(chain, addr)| *chain == chain_id && addr == target)
}

/// Iterator over known PancakeSwap UR deployments as `(chain_id, address)`
/// pairs. Mirrors `universal_router::uniswap_universal_router_deployments()`.
pub fn pancake_universal_router_deployments() -> impl Iterator<Item = (u64, Address)> {
    PANCAKE_UR_ADDRESSES.iter().copied()
}

/// Allowlist of PancakeSwap Universal Router deployments (chain_id, address).
const PANCAKE_UR_ADDRESSES: &[(u64, Address)] = &[
    // Ethereum mainnet — Pancake (Infinity) UR address observed in
    // production frontends.
    (
        1,
        Address::new(
            *b"\x65\xb3\x82\x65\x3f\x7c\x31\xbc\x0a\xf6\x7f\x18\x81\x22\x03\x54\x61\xec\x9c\x76",
        ),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pancake_address_recognised() {
        let addr =
            Address::from_slice(&hex::decode("65b382653f7C31bC0Af67f188122035461ec9C76").unwrap());
        assert!(is_pancake_universal_router(1, &addr));
        // Different chain → not recognised.
        assert!(!is_pancake_universal_router(56, &addr));
    }

    #[test]
    fn unknown_address_is_not_pancake() {
        let addr = Address::from([0x42; 20]);
        assert!(!is_pancake_universal_router(1, &addr));
    }

    #[test]
    fn mask_extracts_six_bit_opcode() {
        // 0x80 | 0x0b = allowRevert + WRAP_ETH; mask 0x3f → 0x0b.
        let raw = 0x80u8 | 0x0b;
        assert_eq!(raw & PANCAKE_UR_MASK, 0x0b);
        assert_eq!(raw & PANCAKE_UR_ALLOW_REVERT, PANCAKE_UR_ALLOW_REVERT);
    }
}
