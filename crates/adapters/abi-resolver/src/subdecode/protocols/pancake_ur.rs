//! PancakeSwap (Infinity) Universal Router opcode table (opcode-dispatched).
//!
//! Source: `pancakeswap/infinity-universal-router @ main`:
//! - `src/libraries/Commands.sol`  — opcode constants (mask `0x3f`)
//! - `src/base/Dispatcher.sol`     — input ABI shapes
//!
//! Pancake UR is a fork of the original Uniswap UR. The intersection 0x00-0x0e
//! is identical (with the same input ABIs as Uniswap **v1** — no
//! `minHopPriceX36` trailing array on V2/V3 swaps), and the table stays
//! lock-step with Uniswap on common opcodes. Pancake-specific differences,
//! verified 1:1 against `Commands.sol` (opcode constants) + `Dispatcher.sol`
//! (decode shapes) at `pancakeswap/infinity-universal-router @ main`:
//!
//! - `mask = 0x3f` (Uniswap is `0x7f`) — opcodes only use the low 6 bits;
//! - `0x07` is a placeholder (Uniswap uses it for `PAY_PORTION_FULL_PRECISION`);
//! - `0x10` `INFI_SWAP` — Pancake Infinity action-stream entrypoint
//!   `(bytes actions, bytes[] params)`; same outer shape as Uniswap UR
//!   `V4_SWAP`, but the inner stream dispatches against `PANCAKE_INFI_TABLE`
//!   (Pancake Infinity 6-field PoolKey/PathKey — not Uniswap V4 5-field);
//! - `0x11 / 0x12` are **placeholders** in `Commands.sol`. Uniswap uses these
//!   for V3 `POSITION_MANAGER_PERMIT/CALL`; Pancake's `Dispatcher.sol` reverts
//!   `InvalidCommandType`. No entry — omission causes a Tier B fallback to
//!   `UNKNOWN` so authors notice the divergence vs Uniswap;
//! - `0x13 / 0x14` are `INFI_CL_INITIALIZE_POOL` / `INFI_BIN_INITIALIZE_POOL`
//!   `(PoolKey, uint160 sqrtPriceX96)` / `(PoolKey, uint24 activeId)`. The
//!   dispatcher forwards via `abi.encodeCall(ICLPoolManager.initialize, ...)`
//!   to the immutable `clPoolManager` / `binPoolManager` stored on the UR —
//!   cross-target callkey extraction is NOT needed (target is UR-self storage,
//!   not a per-chain registry lookup);
//! - `0x15 — 0x20` are placeholders — `Dispatcher.sol` reverts;
//! - `0x22 / 0x23` are Pancake stable-swap opcodes — `Dispatcher.sol` decodes
//!   `(address recipient, uint256 amountIn/Out, uint256 amount{Out,In}{Min,Max},
//!   address[] path, uint256[] flag, bool payerIsUser)` (the comment in
//!   Dispatcher.sol says `bytes path, bytes flag` but the actual decode uses
//!   `inputs.toAddressArray(3)` + `inputs.toUintArray(4)` — D004).
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

    // ---- 0x10..=0x14 — PancakeSwap Infinity entrypoints ----
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
    // 0x11, 0x12 — Commands.sol marks these `COMMAND_PLACEHOLDER`; Pancake
    // `Dispatcher.sol` falls through and reverts `InvalidCommandType`. We
    // deliberately omit entries so Tier B fallback labels them `UNKNOWN`
    // (mirrors how 0x07 is handled). Uniswap UR uses 0x11/0x12 for
    // `V3_POSITION_MANAGER_PERMIT/CALL` — that mismatch was the root cause of
    // the prior copy-paste defect in this file.

    // INFI_CL_INITIALIZE_POOL — `(PoolKey, uint160 sqrtPriceX96)`. The
    // dispatcher wraps via `abi.encodeCall(ICLPoolManager.initialize, ...)`,
    // but the *inputs* slot the UR consumes from the command stream is the
    // raw `(PoolKey, uint160)` pair — that is what we decode here. PoolKey
    // is the 6-field Pancake Infinity variant
    // (currency0, currency1, hooks, poolManager, fee, parameters).
    OpcodeEntry {
        opcode: 0x13,
        name: "INFI_CL_INITIALIZE_POOL",
        input_signatures: &[],
        input_json_abi: Some(INFI_CL_INITIALIZE_POOL_JSON),
    },
    // INFI_BIN_INITIALIZE_POOL — `(PoolKey, uint24 activeId)` with the same
    // 6-field PoolKey. Dispatcher wraps via
    // `abi.encodeCall(IBinPoolManager.initialize, ...)`.
    OpcodeEntry {
        opcode: 0x14,
        name: "INFI_BIN_INITIALIZE_POOL",
        input_signatures: &[],
        input_json_abi: Some(INFI_BIN_INITIALIZE_POOL_JSON),
    },
    // 0x15 — 0x20 are placeholders per Commands.sol; Dispatcher.sol reverts
    // `InvalidCommandType`. No entries — Tier B falls back to UNKNOWN.

    // ---- 0x21 EXECUTE_SUB_PLAN — same shape as Uniswap UR ----
    OpcodeEntry {
        opcode: 0x21,
        name: "EXECUTE_SUB_PLAN",
        input_signatures: &["(bytes commands, bytes[] inputs)"],
        input_json_abi: None,
    },

    // ---- 0x22..=0x23 — Pancake stable-swap (Uniswap leaves these placeholders) ----
    // Verified against `pancakeswap/infinity-universal-router @ main`
    // Dispatcher.sol (D004 — comment vs actual decode):
    // - Dispatcher.sol inline comment says
    //   `abi.decode(inputs, (address, uint256, uint256, bytes, bytes, bool))`
    //   but the *actual* decode uses
    //   `inputs.toAddressArray(3)` + `inputs.toUintArray(4)` →
    //   `address[] path` + `uint256[] flag` (NOT bytes).
    // - `path` carries the StableSwap token-hop address list (length N+1 for
    //   N pools); `flag` carries one uint per pool encoding 2pool/3pool route
    //   metadata.
    OpcodeEntry {
        opcode: 0x22,
        name: "STABLE_SWAP_EXACT_IN",
        input_signatures: &[
            "(address recipient, uint256 amountIn, uint256 amountOutMin, address[] path, uint256[] flag, bool payerIsUser)",
        ],
        input_json_abi: None,
    },
    OpcodeEntry {
        opcode: 0x23,
        name: "STABLE_SWAP_EXACT_OUT",
        input_signatures: &[
            "(address recipient, uint256 amountOut, uint256 amountInMax, address[] path, uint256[] flag, bool payerIsUser)",
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

/// Allowlist of PancakeSwap **Infinity-aware** Universal Router deployments
/// (chain_id, address). Verified against `pancakeswap/infinity-universal-router
/// @ main` `deploy-addresses/{bsc,base}-mainnet.json`.
///
/// **Not included** (Phase B deferred per PANCAKE_PHASE_0_2_REPORT § 0.2):
/// - Ethereum mainnet legacy NFT-aware UR `0x65b382...` (constructor takes
///   `seaportV1_5` / `seaportV1_4` / `openseaConduit` / `x2y2` / `looksRareV2`
///   — opcode 0x10-0x1c map to NFT marketplaces, not Infinity opcodes). Adding
///   a separate `OpcodeTable` for legacy UR is a follow-up.
/// - BSC legacy NFT-aware UR `0x1a0a18AC4...` — same caveat.
const PANCAKE_UR_ADDRESSES: &[(u64, Address)] = &[
    // BSC mainnet — Infinity-aware UR (deploy-addresses/bsc-mainnet.json).
    (
        56,
        Address::new(
            *b"\xd9\xc5\x00\xdf\xf8\x16\xa1\xda\x21\xa4\x8a\x73\x2d\x34\x98\xbf\x09\xdc\x9a\xeb",
        ),
    ),
    // Base mainnet — same address (deploy-addresses/base-mainnet.json).
    (
        8453,
        Address::new(
            *b"\xd9\xc5\x00\xdf\xf8\x16\xa1\xda\x21\xa4\x8a\x73\x2d\x34\x98\xbf\x09\xdc\x9a\xeb",
        ),
    ),
];

// ---------------------------------------------------------------------------
// Per-chain PositionManager / NFPM address resolvers
// ---------------------------------------------------------------------------
//
// Pancake UR's INFI_INIT opcodes (0x13/0x14) forward to *self-stored*
// immutable pool managers (no cross-target callkey extraction needed), but
// the standalone PositionManager dispatcher entries (Phase 3 Batch 3) need a
// per-chain address registry for their bundles' `chain_to_addresses` field.
// The V3 NFPM is also referenced by Smart Router fall-through pathways. Each
// returns `None` for chains we have not verified an Exact-Match deployment
// for (1차 출처: BscScan / BaseScan Verified pages + GitHub deploy-addresses).

/// PancakeSwap V3 NonfungiblePositionManager address (CREATE2-deterministic
/// across BSC + Ethereum + Base — `pancakeswap/pancake-v3-contracts`
/// `deploy-addresses` confirms identical bytes on all three chains).
#[must_use]
pub fn pancake_v3_position_manager_address(chain_id: u64) -> Option<Address> {
    match chain_id {
        // BSC (56), Ethereum (1), Base (8453) — same CREATE2 address.
        1 | 56 | 8453 => Some(Address::new(
            *b"\x46\xA1\x5B\x0b\x27\x31\x1c\xed\xF1\x72\xAB\x29\xE4\xf4\x76\x6f\xbE\x7F\x43\x64",
        )),
        _ => None,
    }
}

/// PancakeSwap Infinity CL PositionManager address (BSC + Base only —
/// Infinity is not deployed on Ethereum).
#[must_use]
pub fn pancake_infinity_cl_position_manager_address(chain_id: u64) -> Option<Address> {
    match chain_id {
        56 | 8453 => Some(Address::new(
            *b"\x55\xf4\xc8\xab\xA7\x1A\x1e\x92\x3e\xdC\x30\x3e\xb4\xfE\xfF\x14\x60\x8c\xC2\x26",
        )),
        _ => None,
    }
}

/// PancakeSwap Infinity Bin PositionManager address (BSC + Base only).
#[must_use]
pub fn pancake_infinity_bin_position_manager_address(chain_id: u64) -> Option<Address> {
    match chain_id {
        56 | 8453 => Some(Address::new(
            *b"\x3D\x31\x1D\x62\x83\xDd\x8a\xB9\x0b\xb0\x03\x18\x35\xC8\xe6\x06\x34\x9e\x28\x50",
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::opcode_stream::dispatch;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    #[test]
    fn pancake_bsc_infinity_ur_recognised() {
        // BSC mainnet Infinity-aware UR.
        let addr =
            Address::from_slice(&hex::decode("d9C500DfF816a1Da21A48A732d3498Bf09dc9AEB").unwrap());
        assert!(is_pancake_universal_router(56, &addr));
        // Same address on Base.
        assert!(is_pancake_universal_router(8453, &addr));
        // NOT registered on Ethereum (Infinity UR isn't deployed there; the
        // legacy NFT-aware UR `0x65b382...` is Phase B deferred).
        assert!(!is_pancake_universal_router(1, &addr));
    }

    #[test]
    fn legacy_eth_nft_aware_ur_not_recognised() {
        // The previous in-file allowlist mistakenly included this address as
        // an "Infinity-aware UR". It is actually the legacy NFT-aware UR
        // (constructor takes seaport / x2y2 / looksRareV2). Phase B deferred.
        let addr =
            Address::from_slice(&hex::decode("65b382653f7C31bC0Af67f188122035461ec9C76").unwrap());
        assert!(!is_pancake_universal_router(1, &addr));
    }

    #[test]
    fn unknown_address_is_not_pancake() {
        let addr = Address::from([0x42; 20]);
        assert!(!is_pancake_universal_router(56, &addr));
    }

    #[test]
    fn mask_extracts_six_bit_opcode() {
        // 0x80 | 0x0b = allowRevert + WRAP_ETH; mask 0x3f → 0x0b.
        let raw = 0x80u8 | 0x0b;
        assert_eq!(raw & PANCAKE_UR_MASK, 0x0b);
        assert_eq!(raw & PANCAKE_UR_ALLOW_REVERT, PANCAKE_UR_ALLOW_REVERT);
    }

    #[test]
    fn pancake_v3_pm_address_is_create2_deterministic_across_chains() {
        let bsc = pancake_v3_position_manager_address(56).unwrap();
        let eth = pancake_v3_position_manager_address(1).unwrap();
        let base = pancake_v3_position_manager_address(8453).unwrap();
        assert_eq!(bsc, eth);
        assert_eq!(bsc, base);
        assert!(pancake_v3_position_manager_address(42161).is_none());
    }

    #[test]
    fn pancake_infinity_pm_addresses_bsc_and_base_only() {
        // Infinity PMs are deployed on BSC + Base, not Ethereum.
        assert!(pancake_infinity_cl_position_manager_address(56).is_some());
        assert!(pancake_infinity_cl_position_manager_address(8453).is_some());
        assert!(pancake_infinity_cl_position_manager_address(1).is_none());
        assert!(pancake_infinity_bin_position_manager_address(56).is_some());
        assert!(pancake_infinity_bin_position_manager_address(8453).is_some());
        assert!(pancake_infinity_bin_position_manager_address(1).is_none());
    }

    #[test]
    fn infi_cl_initialize_pool_decodes_pool_key_and_sqrt_price() {
        // Construct `(PoolKey(6 fields), uint160 sqrtPriceX96)`.
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(alloy_primitives::Address::ZERO),
            DynSolValue::Address(alloy_primitives::Address::from([0xaa; 20])),
            DynSolValue::Address(alloy_primitives::Address::ZERO),
            DynSolValue::Address(alloy_primitives::Address::from([0xbb; 20])),
            DynSolValue::Uint(U256::from(2_500u64), 24),
            DynSolValue::FixedBytes(alloy_primitives::B256::ZERO, 32),
        ]);
        let sqrt_price = DynSolValue::Uint(U256::from(79228162514264337593543950336u128), 160);
        let f = Function::parse("step((address,address,address,address,uint24,bytes32),uint160)")
            .unwrap();
        let payload = f.abi_encode_input(&[pool_key, sqrt_price]).unwrap()[4..].to_vec();
        let steps = dispatch(&[0x13], &[payload], &PANCAKE_UR_TABLE);
        assert_eq!(steps[0].name, "INFI_CL_INITIALIZE_POOL");
        let args = steps[0].args.as_ref().unwrap();
        assert_eq!(args[0].name, "poolKey");
        assert_eq!(args[1].name, "sqrtPriceX96");
        // PoolKey component names: 6 fields including poolManager + parameters.
        let comp_names: Vec<&str> = args[0].components.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            comp_names,
            [
                "currency0",
                "currency1",
                "hooks",
                "poolManager",
                "fee",
                "parameters"
            ]
        );
    }

    #[test]
    fn infi_bin_initialize_pool_decodes_active_id_uint24() {
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(alloy_primitives::Address::ZERO),
            DynSolValue::Address(alloy_primitives::Address::from([0xcc; 20])),
            DynSolValue::Address(alloy_primitives::Address::ZERO),
            DynSolValue::Address(alloy_primitives::Address::from([0xdd; 20])),
            DynSolValue::Uint(U256::from(5_000u64), 24),
            DynSolValue::FixedBytes(alloy_primitives::B256::ZERO, 32),
        ]);
        let active_id = DynSolValue::Uint(U256::from(8388608u64), 24);
        let f = Function::parse("step((address,address,address,address,uint24,bytes32),uint24)")
            .unwrap();
        let payload = f.abi_encode_input(&[pool_key, active_id]).unwrap()[4..].to_vec();
        let steps = dispatch(&[0x14], &[payload], &PANCAKE_UR_TABLE);
        assert_eq!(steps[0].name, "INFI_BIN_INITIALIZE_POOL");
        let args = steps[0].args.as_ref().unwrap();
        assert_eq!(args[1].name, "activeId");
    }

    #[test]
    fn stable_swap_exact_in_decodes_address_array_path_and_uint_array_flag() {
        // Dispatcher.sol actual decode is `address[] path` + `uint256[] flag`,
        // not `bytes path` + `bytes flag` (D004 fix).
        let f = Function::parse("step(address,uint256,uint256,address[],uint256[],bool)").unwrap();
        let payload = f
            .abi_encode_input(&[
                DynSolValue::Address(alloy_primitives::Address::from([0x11; 20])),
                DynSolValue::Uint(U256::from(1_000u64), 256),
                DynSolValue::Uint(U256::from(900u64), 256),
                DynSolValue::Array(vec![
                    DynSolValue::Address(alloy_primitives::Address::from([0xaa; 20])),
                    DynSolValue::Address(alloy_primitives::Address::from([0xbb; 20])),
                    DynSolValue::Address(alloy_primitives::Address::from([0xcc; 20])),
                ]),
                DynSolValue::Array(vec![
                    DynSolValue::Uint(U256::from(2u64), 256),
                    DynSolValue::Uint(U256::from(3u64), 256),
                ]),
                DynSolValue::Bool(true),
            ])
            .unwrap()[4..]
            .to_vec();
        let steps = dispatch(&[0x22], &[payload], &PANCAKE_UR_TABLE);
        assert_eq!(steps[0].name, "STABLE_SWAP_EXACT_IN");
        let args = steps[0].args.as_ref().unwrap();
        assert_eq!(args[3].name, "path");
        assert!(
            matches!(args[3].value, DynSolValue::Array(_)),
            "path must decode as Array (address[]), got {:?}",
            args[3].value
        );
        assert_eq!(args[4].name, "flag");
        assert!(
            matches!(args[4].value, DynSolValue::Array(_)),
            "flag must decode as Array (uint256[]), got {:?}",
            args[4].value
        );
    }

    #[test]
    fn opcodes_0x11_and_0x12_fall_through_unknown() {
        // Pancake `Commands.sol` places COMMAND_PLACEHOLDER at 0x11/0x12 —
        // Dispatcher.sol reverts. We must not silently dispatch as a Uniswap-
        // shaped V3 PositionManager opcode (the prior copy-paste bug).
        // dispatch() takes one-byte-per-step. With no entry the step's name
        // falls back to "UNKNOWN" (Tier B convention).
        let steps = dispatch(&[0x11], &[Vec::new()], &PANCAKE_UR_TABLE);
        assert_eq!(steps[0].name, "UNKNOWN");
        let steps = dispatch(&[0x12], &[Vec::new()], &PANCAKE_UR_TABLE);
        assert_eq!(steps[0].name, "UNKNOWN");
    }
}
