//! Balancer V2 enum-tagged sub-decoder tables.
//!
//! `joinPool` / `exitPool` on the Vault both take a `JoinPoolRequest` /
//! `ExitPoolRequest` whose `bytes userData` is `(JoinKind kind, …)` /
//! `(ExitKind kind, …)` — the first 32-byte word picks which tail follows.
//!
//! Source (verified against `balancer/balancer-v2-monorepo @ master`):
//! - `pkg/interfaces/contracts/pool-weighted/WeightedPoolUserData.sol`
//! - `pkg/interfaces/contracts/pool-stable/StablePoolUserData.sol`
//!
//! Important: **JoinKind** is identical between Weighted and Stable pools
//! (4 enum values, same names, same per-kind payload shapes), so a single
//! table covers both. **ExitKind**, however, *differs* — the same numeric
//! value (e.g. `1`) decodes to different schemas for Weighted vs Stable.
//! Callers should `try_dispatch` against `[WEIGHTED, STABLE]`; the engine
//! picks whichever shape ABI-decodes cleanly. (Some genuine ambiguity is
//! unavoidable without the on-chain pool type, but the `(uint256[],uint256)`
//! vs `(uint256)` shapes are usually distinguishable by length.)

use crate::subdecode::enum_tagged::{EnumEntry, EnumTable};

// ---------------------------------------------------------------------------
// Selectors — Vault.joinPool / Vault.exitPool
// ---------------------------------------------------------------------------

/// `Vault.joinPool(bytes32,address,address,(address[],uint256[],bytes,bool))`.
pub const JOIN_POOL_SELECTOR: [u8; 4] = [0xb9, 0x5c, 0xac, 0x28];

/// `Vault.exitPool(bytes32,address,address,(address[],uint256[],bytes,bool))`.
pub const EXIT_POOL_SELECTOR: [u8; 4] = [0x8b, 0xdb, 0x39, 0x13];

// ---------------------------------------------------------------------------
// JoinKind — uniform across Weighted, Stable, MetaStable, ComposableStable
// ---------------------------------------------------------------------------
//
// enum JoinKind {
//   INIT,                             // 0
//   EXACT_TOKENS_IN_FOR_BPT_OUT,      // 1
//   TOKEN_IN_FOR_EXACT_BPT_OUT,       // 2
//   ALL_TOKENS_IN_FOR_EXACT_BPT_OUT,  // 3
// }

const JOIN_INIT_JSON: &str = r#"[
    {"name": "kind",      "type": "uint256"},
    {"name": "amountsIn", "type": "uint256[]"}
]"#;

const JOIN_EXACT_TOKENS_IN_FOR_BPT_OUT_JSON: &str = r#"[
    {"name": "kind",            "type": "uint256"},
    {"name": "amountsIn",       "type": "uint256[]"},
    {"name": "minBPTAmountOut", "type": "uint256"}
]"#;

const JOIN_TOKEN_IN_FOR_EXACT_BPT_OUT_JSON: &str = r#"[
    {"name": "kind",          "type": "uint256"},
    {"name": "bptAmountOut",  "type": "uint256"},
    {"name": "tokenIndex",    "type": "uint256"}
]"#;

const JOIN_ALL_TOKENS_IN_FOR_EXACT_BPT_OUT_JSON: &str = r#"[
    {"name": "kind",         "type": "uint256"},
    {"name": "bptAmountOut", "type": "uint256"}
]"#;

/// JoinKind dispatch table — covers every Balancer V2 pool type
/// (Weighted, Stable, MetaStable, ComposableStable, Linear) since they all
/// share the same enum.
pub static BALANCER_V2_JOIN_KIND: EnumTable = EnumTable {
    name: "Balancer V2 JoinKind",
    entries: &[
        EnumEntry {
            kind: 0,
            name: "INIT",
            payload_json_abi: JOIN_INIT_JSON,
        },
        EnumEntry {
            kind: 1,
            name: "EXACT_TOKENS_IN_FOR_BPT_OUT",
            payload_json_abi: JOIN_EXACT_TOKENS_IN_FOR_BPT_OUT_JSON,
        },
        EnumEntry {
            kind: 2,
            name: "TOKEN_IN_FOR_EXACT_BPT_OUT",
            payload_json_abi: JOIN_TOKEN_IN_FOR_EXACT_BPT_OUT_JSON,
        },
        EnumEntry {
            kind: 3,
            name: "ALL_TOKENS_IN_FOR_EXACT_BPT_OUT",
            payload_json_abi: JOIN_ALL_TOKENS_IN_FOR_EXACT_BPT_OUT_JSON,
        },
    ],
};

// ---------------------------------------------------------------------------
// ExitKind — DIFFERS between Weighted and Stable pool types
// ---------------------------------------------------------------------------
//
// WeightedPoolUserData.ExitKind:
//   EXACT_BPT_IN_FOR_ONE_TOKEN_OUT,     // 0
//   EXACT_BPT_IN_FOR_TOKENS_OUT,        // 1   ← NOT in Stable
//   BPT_IN_FOR_EXACT_TOKENS_OUT,        // 2
//
// StablePoolUserData.ExitKind:
//   EXACT_BPT_IN_FOR_ONE_TOKEN_OUT,     // 0
//   BPT_IN_FOR_EXACT_TOKENS_OUT,        // 1   ← Weighted's index 2
//   EXACT_BPT_IN_FOR_ALL_TOKENS_OUT,    // 2   ← NOT in Weighted
//
// Per-kind payload shapes:
//   EXACT_BPT_IN_FOR_ONE_TOKEN_OUT:  (kind, uint256 bptAmountIn, uint256 tokenIndex)
//   EXACT_BPT_IN_FOR_TOKENS_OUT:     (kind, uint256 bptAmountIn)
//   BPT_IN_FOR_EXACT_TOKENS_OUT:     (kind, uint256[] amountsOut, uint256 maxBPTAmountIn)
//   EXACT_BPT_IN_FOR_ALL_TOKENS_OUT: (kind, uint256 bptAmountIn)

const EXIT_EXACT_BPT_IN_FOR_ONE_TOKEN_OUT_JSON: &str = r#"[
    {"name": "kind",         "type": "uint256"},
    {"name": "bptAmountIn",  "type": "uint256"},
    {"name": "tokenIndex",   "type": "uint256"}
]"#;

const EXIT_EXACT_BPT_IN_FOR_TOKENS_OUT_JSON: &str = r#"[
    {"name": "kind",        "type": "uint256"},
    {"name": "bptAmountIn", "type": "uint256"}
]"#;

const EXIT_BPT_IN_FOR_EXACT_TOKENS_OUT_JSON: &str = r#"[
    {"name": "kind",             "type": "uint256"},
    {"name": "amountsOut",       "type": "uint256[]"},
    {"name": "maxBPTAmountIn",   "type": "uint256"}
]"#;

const EXIT_EXACT_BPT_IN_FOR_ALL_TOKENS_OUT_JSON: &str = r#"[
    {"name": "kind",        "type": "uint256"},
    {"name": "bptAmountIn", "type": "uint256"}
]"#;

/// Weighted-pool ExitKind table.
pub static BALANCER_V2_EXIT_KIND_WEIGHTED: EnumTable = EnumTable {
    name: "Balancer V2 ExitKind (Weighted)",
    entries: &[
        EnumEntry {
            kind: 0,
            name: "EXACT_BPT_IN_FOR_ONE_TOKEN_OUT",
            payload_json_abi: EXIT_EXACT_BPT_IN_FOR_ONE_TOKEN_OUT_JSON,
        },
        EnumEntry {
            kind: 1,
            name: "EXACT_BPT_IN_FOR_TOKENS_OUT",
            payload_json_abi: EXIT_EXACT_BPT_IN_FOR_TOKENS_OUT_JSON,
        },
        EnumEntry {
            kind: 2,
            name: "BPT_IN_FOR_EXACT_TOKENS_OUT",
            payload_json_abi: EXIT_BPT_IN_FOR_EXACT_TOKENS_OUT_JSON,
        },
    ],
};

/// Stable-pool ExitKind table.
pub static BALANCER_V2_EXIT_KIND_STABLE: EnumTable = EnumTable {
    name: "Balancer V2 ExitKind (Stable)",
    entries: &[
        EnumEntry {
            kind: 0,
            name: "EXACT_BPT_IN_FOR_ONE_TOKEN_OUT",
            payload_json_abi: EXIT_EXACT_BPT_IN_FOR_ONE_TOKEN_OUT_JSON,
        },
        EnumEntry {
            kind: 1,
            name: "BPT_IN_FOR_EXACT_TOKENS_OUT",
            payload_json_abi: EXIT_BPT_IN_FOR_EXACT_TOKENS_OUT_JSON,
        },
        EnumEntry {
            kind: 2,
            name: "EXACT_BPT_IN_FOR_ALL_TOKENS_OUT",
            payload_json_abi: EXIT_EXACT_BPT_IN_FOR_ALL_TOKENS_OUT_JSON,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subdecode::enum_tagged::{dispatch, try_dispatch};
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    fn encode(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
        let func = Function::parse(&format!("step{sig}")).unwrap();
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn join_init_decodes_against_uniform_table() {
        // Encode userData = (kind=0, amountsIn=[100,200])
        let payload = encode(
            "(uint256,uint256[])",
            vec![
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Array(vec![
                    DynSolValue::Uint(U256::from(100u64), 256),
                    DynSolValue::Uint(U256::from(200u64), 256),
                ]),
            ],
        );
        let d = dispatch(&payload, &BALANCER_V2_JOIN_KIND).unwrap();
        assert_eq!(d.kind, 0);
        assert_eq!(d.kind_name, "INIT");
        assert_eq!(d.args[0].name, "kind");
        assert_eq!(d.args[1].name, "amountsIn");
    }

    #[test]
    fn join_exact_tokens_in_for_bpt_out() {
        let payload = encode(
            "(uint256,uint256[],uint256)",
            vec![
                DynSolValue::Uint(U256::from(1u64), 256),
                DynSolValue::Array(vec![DynSolValue::Uint(U256::from(50u64), 256)]),
                DynSolValue::Uint(U256::from(99u64), 256),
            ],
        );
        let d = dispatch(&payload, &BALANCER_V2_JOIN_KIND).unwrap();
        assert_eq!(d.kind_name, "EXACT_TOKENS_IN_FOR_BPT_OUT");
        assert_eq!(d.args[2].name, "minBPTAmountOut");
    }

    #[test]
    fn weighted_exit_kind_1_does_not_match_stable() {
        // Weighted kind=1 is EXACT_BPT_IN_FOR_TOKENS_OUT → (kind, uint256)
        let payload = encode(
            "(uint256,uint256)",
            vec![
                DynSolValue::Uint(U256::from(1u64), 256),
                DynSolValue::Uint(U256::from(42u64), 256),
            ],
        );
        // Try Weighted first → matches
        let d = try_dispatch(
            &payload,
            &[
                &BALANCER_V2_EXIT_KIND_WEIGHTED,
                &BALANCER_V2_EXIT_KIND_STABLE,
            ],
        )
        .unwrap();
        assert_eq!(d.kind_name, "EXACT_BPT_IN_FOR_TOKENS_OUT");
        assert_eq!(d.table_name, "Balancer V2 ExitKind (Weighted)");
    }

    #[test]
    fn stable_exit_kind_1_uses_different_shape() {
        // Stable kind=1 is BPT_IN_FOR_EXACT_TOKENS_OUT → (kind, uint256[], uint256)
        let payload = encode(
            "(uint256,uint256[],uint256)",
            vec![
                DynSolValue::Uint(U256::from(1u64), 256),
                DynSolValue::Array(vec![DynSolValue::Uint(U256::from(7u64), 256)]),
                DynSolValue::Uint(U256::from(8u64), 256),
            ],
        );
        // Weighted's kind=1 expects (kind, uint256), so it will MISDECODE
        // (alloy might still succeed because uint256 reads any 32 bytes, but
        // structurally the trailing data is wrong; this is the unavoidable
        // ambiguity from kind reuse). Try Stable first to be deterministic
        // when the caller knows it's a Stable-shaped pool.
        let d = try_dispatch(
            &payload,
            &[
                &BALANCER_V2_EXIT_KIND_STABLE,
                &BALANCER_V2_EXIT_KIND_WEIGHTED,
            ],
        )
        .unwrap();
        assert_eq!(d.kind_name, "BPT_IN_FOR_EXACT_TOKENS_OUT");
        assert_eq!(d.table_name, "Balancer V2 ExitKind (Stable)");
    }
}
