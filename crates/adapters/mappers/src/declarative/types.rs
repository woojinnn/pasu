//! Bundle JSON types — 1:1 with ADAPTER_LOADER_ARCHITECTURE.md §4.1, §5.1.
//!
//! Phase 0 scope: serde-able structs/enums only. No interpreter, no Mapper impl.
//! These types are wire format definitions that future phases (1, 4, 5, ...)
//! will execute via DeclarativeMapper.
//!
//! Notes:
//!  - Spec §5.1 BNF lists a `concat` BuiltinFn, while §5.3.1 has the actual
//!    signature `concat_bytes(a, b)`. We adopt **concat_bytes** (snake_case)
//!    to match the executable signature. This is review finding M-3.
//!  - Strategy enum is fully defined in Phase 0 so all 4 variants parse via
//!    serde. Execution for `OpcodeStreamDispatch` / `EnumTaggedDispatch` /
//!    `MulticallRecurse` is filled in by later phases (Phase 5 / 향후 / Phase 4).
//!  - `ValueExpr` is a serde `untagged` enum. Variant ordering matters: we
//!    list `Literal` first (matches `{ "literal": ... }`), then `Transform`
//!    (matches `{ "fn": ..., "args": [...] }`), then `FromArg` (matches
//!    `{ "from": "..." }` with optional `via` / `kind`).
//!  - `BTreeMap` over `HashMap` keeps `fields` ordered for stable serde
//!    output and deterministic test snapshots.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level bundle
// ---------------------------------------------------------------------------

/// Bundle JSON top-level (§4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterFunctionBundle {
    /// Bundle type tag. Must be `"adapter_function"` for Phase 0 / Phase 1.
    #[serde(rename = "type")]
    pub bundle_type: BundleType,

    /// Bundle id (`publisher/name@version` — semver). Example:
    /// `"uniswap/v2/swapExactTokensForTokens@1.0.0"`.
    pub id: String,

    /// Publisher identity (e.g. ENS name).
    pub publisher: String,

    /// Match criteria — which calldata this bundle handles.
    #[serde(rename = "match")]
    pub match_: BundleMatch,

    /// Outer ABI fragment (alloy parses raw bytes via this ABI).
    pub abi_fragment: AbiFragment,

    /// Emit rule (4 strategies — see [`EmitRule`]).
    pub emit: EmitRule,

    /// Dependency declaration (Tier B imperatives, host capabilities,
    /// minimum extension version).
    pub requires: Requires,
}

/// Bundle type tag. Phase 0 only handles `adapter_function`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleType {
    AdapterFunction,
}

/// Match criteria identifying a callsite. registry v2 schema:
/// `chain_to_addresses` (chain → addresses map) + `selector`. The cartesian
/// `chain_ids × to` shape of v1 is retained via `#[serde(default)]` for
/// backward compatibility (test fixtures, legacy seed bundles). Iteration
/// must go through [`BundleMatch::entries`] so consumers do not need to
/// branch on which shape parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleMatch {
    /// v2 — chain id → contract addresses (case-insensitive hex). Empty
    /// (`{}`) when the bundle uses the v1 legacy shape below.
    #[serde(default)]
    pub chain_to_addresses: BTreeMap<u64, Vec<String>>,

    /// v1 legacy — chain ids the bundle applies to. Empty when the bundle
    /// uses the v2 shape above. Cartesian with [`Self::to`].
    #[serde(default)]
    pub chain_ids: Vec<u64>,

    /// v1 legacy — contract addresses the bundle applies to. Empty when the
    /// bundle uses the v2 shape above. Cartesian with [`Self::chain_ids`].
    #[serde(default)]
    pub to: Vec<String>,

    /// 4-byte function selector as `"0x" + 8 hex chars`.
    pub selector: String,
}

impl BundleMatch {
    /// Iterate `(chain_id, address)` pairs regardless of which shape
    /// (v2 `chain_to_addresses` or v1 `chain_ids × to`) the bundle uses.
    /// v2 takes precedence when both are present; legacy cartesian is the
    /// fallback. Returns owned pairs so the caller can mutate addresses
    /// (lowercase) without conflicting with the borrow.
    pub fn entries(&self) -> Vec<(u64, String)> {
        if !self.chain_to_addresses.is_empty() {
            return self
                .chain_to_addresses
                .iter()
                .flat_map(|(c, addrs)| addrs.iter().map(move |a| (*c, a.clone())))
                .collect();
        }
        // v1 cartesian fallback
        self.chain_ids
            .iter()
            .flat_map(|c| self.to.iter().map(move |a| (*c, a.clone())))
            .collect()
    }
}

/// Outer ABI fragment. `abi` is opaque `serde_json::Value` because alloy
/// consumes it through `alloy_json_abi::Function` at runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbiFragment {
    pub function_name: String,
    pub abi: serde_json::Value,
}

/// Dependency declaration.
///
/// Capabilities are split into two tiers (Phase 7B):
///  - `adapter_capabilities` — adapter-layer responsibilities resolved at
///    static lookup time (e.g. `"token_metadata"` — symbol / decimals look-up
///    via the registry-side static token endpoint).
///  - `host_capabilities` — host-layer dynamic enrichment requiring live
///    RPC / oracle calls at evaluation time (e.g. `"host:oracle"`).
///
/// `#[serde(default)]` on both fields keeps backward compatibility with
/// older bundles that may omit either key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requires {
    /// Tier B imperative dependencies (e.g. `"universal-router-dispatcher@^1.0"`).
    /// Each entry MUST be statically embedded in the extension.
    pub imperative: Vec<String>,

    /// Adapter-layer capabilities — resolved at static lookup time
    /// (e.g. `"token_metadata"`).
    #[serde(default)]
    pub adapter_capabilities: Vec<String>,

    /// Host-layer capabilities — dynamic RPC enrichment only
    /// (e.g. `"host:oracle"`). Phase 7B narrows this field's meaning to
    /// dynamic-only; static lookups moved to `adapter_capabilities`.
    #[serde(default)]
    pub host_capabilities: Vec<String>,

    /// Minimum extension version (semver requirement, e.g. `">=0.1.0"`).
    pub extension: String,
}

// ---------------------------------------------------------------------------
// Emit rule — 4 strategies
// ---------------------------------------------------------------------------

/// Emit rule. Discriminated by `strategy` tag (§5.1 BNF).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum EmitRule {
    /// Simple ABI → single ActionEnvelope. Phase 1 implementation target.
    SingleEmit {
        category: String,
        action: String,
        fields: BTreeMap<String, ValueExpr>,
    },

    /// Opcode stream dispatch (Universal Router, Pancake UR, Sushi RP, 0x Settler).
    /// Phase 5 execution.
    OpcodeStreamDispatch {
        dispatcher_id: String,
        /// `"0x" + 1-2 hex` (e.g. `"0x7f"` — opcode mask).
        mask: String,
        /// `"0x" + 1-2 hex` (e.g. `"0x80"` — allow-revert bit).
        allow_revert_bit: String,
        per_opcode_emit: BTreeMap<String, PerOpcodeEmit>,
        unknown_opcode_policy: UnknownOpcodePolicy,
    },

    /// Enum-tagged dispatch (Balancer V2 joinPool, V4 Router actions). 향후.
    EnumTaggedDispatch {
        dispatcher_id: String,
        tag_path: String,
        tag_decoder: String,
        per_variant_emit: BTreeMap<String, PerVariantEmit>,
        unknown_variant_policy: UnknownVariantPolicy,
    },

    /// Multicall recursion (Cat D). Phase 4 execution.
    MulticallRecurse {
        recurse_rule_id: String,
        max_depth: u8,
    },

    /// Array fan-out — one ABI tuple-array argument → N ActionEnvelopes,
    /// one per element. Phase 7B (Permit2 batch overloads). Generalises
    /// `single_emit`: the field tree is built once per array element with a
    /// synthetic `element` arg bound to the current row.
    ArrayEmit {
        category: String,
        action: String,
        /// `JsonPath` to the tuple-array argument, e.g. `"$.args.transferDetails"`
        /// or `"$.args.permitBatch[0]"` (the `PermitDetails[]` inside `PermitBatch`).
        array_path: String,
        /// Hard cap on element count (`DoS` guard). 1..=64.
        max_elements: u8,
        /// Optional parallel arrays — synchronised index. Maps a synthetic
        /// arg name → `JsonPath` of another array of equal length. Element i of
        /// `array_path` is bound to `element`; element i of each parallel
        /// array is bound to its key name. Used by Permit2
        /// `permitTransferFrom(batch)` where `TokenPermissions[]` and
        /// `SignatureTransferDetails[]` are index-aligned.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        parallel_paths: BTreeMap<String, String>,
        /// Per-element field map. `$.args.element[...]` resolves to the
        /// current `array_path` element; `$.args.<parallelKey>[...]` to the
        /// matching parallel element; `$.args.*`/`$.tx.*`/`$.context.*` to
        /// the outer call.
        fields: BTreeMap<String, ValueExpr>,
    },
}

/// Per-opcode emit rule (inside `OpcodeStreamDispatch.per_opcode_emit`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerOpcodeEmit {
    pub name: String,
    pub category: String,
    pub action: String,
    pub fields: BTreeMap<String, ValueExpr>,
}

/// Per-variant emit rule (inside `EnumTaggedDispatch.per_variant_emit`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerVariantEmit {
    pub name: String,
    pub category: String,
    pub action: String,
    pub fields: BTreeMap<String, ValueExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownOpcodePolicy {
    Deny,
    Warn,
    IgnoreStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownVariantPolicy {
    Deny,
    Warn,
}

// ---------------------------------------------------------------------------
// ValueExpr — Literal | FromArg | Transform (untagged union per §5.1 BNF)
// ---------------------------------------------------------------------------

/// Per spec §5.1:
///
/// ```text
/// ValueExpr := Literal | FromArg | Transform
/// Literal   := { literal: TypedValue }
/// FromArg   := { from: JsonPath, via?: HostCapability, kind?: AmountKind }
/// Transform := { fn: WhitelistedFn, args: [ValueExpr; max_4] }
/// ```
///
/// Variant order in the enum is significant for serde `untagged`: the first
/// variant that successfully deserializes wins. We order:
///   1. `Literal`   — distinguished by presence of `"literal"` key
///   2. `Transform` — distinguished by presence of `"fn"` key
///   3. `FromArg`   — distinguished by presence of `"from"` key
///
/// This ordering avoids ambiguity because each variant's required field is
/// unique to that variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueExpr {
    Literal {
        literal: serde_json::Value,
    },
    Transform {
        #[serde(rename = "fn")]
        function: BuiltinFn,
        args: Vec<ValueExpr>,
    },
    FromArg {
        from: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        via: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        kind: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// BuiltinFn — WhitelistedFn = BuiltinFn ∪ TierBBackedFn
// ---------------------------------------------------------------------------

/// All callable functions (`WhitelistedFn` in §5.1 BNF). For Phase 0 we keep
/// builtin and TierB-backed in one flat enum — they are distinguished by
/// implementation (interpreter built-in vs Tier B static), not by wire format.
///
/// Phase 1 uses only [`BuiltinFn::SelectAddress`]. Other variants parse but
/// are not executed; calling them at map time will `unimplemented!()` until
/// the respective phase fills them in.
///
/// Note: spec §5.1 BNF lists `"concat"`, but §5.3.1 has signature
/// `concat_bytes(a, b)`. We use **`concat_bytes`** (snake_case) to match
/// the executable signature. See review finding M-3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinFn {
    // ---- Phase 1: used by V2 swap ----
    /// Pick element of `address[]` by index (idx = -1 = last).
    SelectAddress,

    // ---- Builtin (§5.3.1) — parsed in Phase 0, execution by later phases ----
    /// `div(num: U256, denom: u32) -> U256` with /0 + overflow checks.
    Div,
    /// `mul(num: U256, factor: u32) -> U256` with overflow check.
    Mul,
    /// `concat_bytes(a, b) -> Vec<u8>` bounded by MAX_CONCAT (4096).
    /// Spec BNF says `concat` — we adopt `concat_bytes` to match §5.3.1.
    ConcatBytes,
    /// `select_uint(arr, idx) -> U256`.
    SelectUint,
    /// `select_bytes(arr, idx) -> Bytes`.
    SelectBytes,
    /// `literal_address(s) -> AddressRef`.
    LiteralAddress,
    /// `addr_eq(a, b) -> bool`.
    AddrEq,
    /// `selector_eq(a, b) -> bool`.
    SelectorEq,
    /// `chain_id() -> u64`.
    ChainId,
    /// `now() -> u64`.
    Now,

    // ---- TierB-backed (§5.3.2) — DSL wrapper for Tier B static helpers ----
    /// `unfold_packed(bytes, format)`. Backend: `subdecode/packed.rs`.
    UnfoldPacked,
    /// `unfold_v3_path(bytes, select)`. Backend:
    /// `subdecode/protocols/uniswap_v3.rs::decode_v3_path`. Phase 3.
    UnfoldV3Path,
    /// `curve_route_last_token(route: address[11], swap_params: uint256[N][5])
    /// -> AddressRef` — Curve Router NG output-token resolver. Mirrors
    /// `Router.vy::exchange` per-hop semantics: hop `i` executes while
    /// `route[2i+1] != 0`, and the emitted output token of the last executed
    /// hop is `route[2i+2]` for coin-producing swap types (1/2/3/6/7) or
    /// `route[2i+1]` for pool/helper/vault-producing swap types
    /// (4 LP_ADD, 5 LENDING_TO_LP, 8 WRAPPED_ASSET_CONVERT, 9 ERC4626_ASSET_SHARE).
    /// `swap_type` is read from `swap_params[i][2]` (both `uint256[5][5]` and
    /// `uint256[4][5]` Router NG variants encode swap_type at inner index `[2]`).
    /// Backend: [`super::builtin_fn::curve_route_last_token`]. Phase 12.3 +
    /// Phase 13 P1-5 + F3 / F-route1.B Phase C (V3 round).
    CurveRouteLastToken,
    /// `select_from_literal_array(array, idx) -> Value` — pick an element
    /// from a bundle-embedded literal array (typically pool `coins[]`) by a
    /// caller-supplied i/j index. Backend:
    /// [`super::builtin_fn::select_from_literal_array`]. Phase 12.7 (P0-2).
    ///
    /// Used by Curve V1/V2/NG `exchange` + `remove_liquidity_one_coin`
    /// bundles to resolve `coins[i]` / `coins[j]` instead of hardcoding the
    /// first/second token of the pool — the previous bundles silently
    /// mislabelled inputs and outputs whenever the user passed any
    /// `(i, j) != (0, 1)`.
    SelectFromLiteralArray,
    /// `unfold_slipstream_path(bytes, select [, hop_index])`. Phase 8
    /// (Aerodrome CL). Slipstream packed path encodes a **signed**
    /// `int24 tickSpacing` between tokens instead of Uniswap V3's
    /// `uint24 fee` — sign-extension applied on decode. Backend lives in
    /// `declarative::builtin_fn::unfold_slipstream_path`.
    UnfoldSlipstreamPath,
    /// `unfold_velo_v2_path(bytes, select)`. Phase 2 (Aerodrome
    /// Universal Router `V2_SWAP` — opcodes `0x08` / `0x09`). The UR
    /// `main` build packs the V2 swap path as 20-byte `token` segments
    /// whose stride depends on an `isUni` flag: the UniV2 layout is
    /// `token ++ token ++ …` (`len = 20*N`), the VeloV2 layout
    /// interleaves a 1-byte `stable` flag (`len = 20 + 21*N`). Both
    /// layouts are invariant at the endpoints — the path always starts
    /// and ends on a 20-byte token — so this built-in extracts
    /// `path[0..20]` / `path[len-20..len]` without parsing the stable
    /// byte or the stride. `select ∈ {first_token, last_token}`.
    /// Backend lives in `declarative::builtin_fn::unfold_velo_v2_path`.
    UnfoldVeloV2Path,
    /// `map_recipient(addr) -> Address` — resolve a Uniswap action recipient
    /// sentinel: `0x..01` (`MSG_SENDER`) → `ctx.from`, `0x..02`
    /// (`ADDRESS_THIS`) → `ctx.to`; any other address passes through.
    /// TierB-backed — wraps `protocols::universal_router::common::map_recipient`.
    /// `VERIFICATION_UNISWAP_REALTX` finding F3 — UR opcode recipients were
    /// emitted as raw sentinel literals on the declarative path.
    MapRecipient,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// V2 swap fixture (the canonical §4.1 example) must round-trip through
    /// serde without losing semantic info. Field check below also asserts
    /// the variant matching of `EmitRule` / `ValueExpr` is correct.
    #[test]
    fn roundtrip_v2_swap_bundle() {
        let json = include_str!("../../tests/fixtures/uniswap-v2-swap-exact-tokens.json");

        let bundle: AdapterFunctionBundle = serde_json::from_str(json).expect("fixture parses");

        // Top-level identity.
        assert_eq!(bundle.bundle_type, BundleType::AdapterFunction);
        assert_eq!(bundle.id, "uniswap/v2/swapExactTokensForTokens@1.0.0");
        assert_eq!(bundle.publisher, "uniswap.eth");

        // match
        assert_eq!(bundle.match_.chain_ids, vec![1, 8453, 10, 42161]);
        assert_eq!(bundle.match_.selector, "0x38ed1739");

        // abi
        assert_eq!(
            bundle.abi_fragment.function_name,
            "swapExactTokensForTokens"
        );

        // emit — must be SingleEmit
        let fields = match &bundle.emit {
            EmitRule::SingleEmit {
                category,
                action,
                fields,
            } => {
                assert_eq!(category, "dex");
                assert_eq!(action, "swap");
                fields
            }
            _ => panic!("expected SingleEmit, got {:?}", bundle.emit),
        };

        // Spot-check key field shapes.
        match fields.get("inputToken.asset.kind") {
            Some(ValueExpr::Literal { literal }) => {
                assert_eq!(literal, &serde_json::json!("erc20"));
            }
            other => panic!("inputToken.asset.kind: expected Literal, got {other:?}"),
        }

        match fields.get("inputToken.asset.address") {
            Some(ValueExpr::Transform { function, args }) => {
                assert_eq!(*function, BuiltinFn::SelectAddress);
                assert_eq!(args.len(), 2);
                match &args[0] {
                    ValueExpr::FromArg { from, via, kind } => {
                        assert_eq!(from, "$.args.path");
                        assert!(via.is_none());
                        assert!(kind.is_none());
                    }
                    other => panic!("args[0]: expected FromArg, got {other:?}"),
                }
                match &args[1] {
                    ValueExpr::Literal { literal } => {
                        assert_eq!(literal, &serde_json::json!(0));
                    }
                    other => panic!("args[1]: expected Literal, got {other:?}"),
                }
            }
            other => panic!("inputToken.asset.address: expected Transform, got {other:?}"),
        }

        match fields.get("outputToken.asset.address") {
            Some(ValueExpr::Transform { function, args }) => {
                assert_eq!(*function, BuiltinFn::SelectAddress);
                // idx = -1 (last element)
                match &args[1] {
                    ValueExpr::Literal { literal } => {
                        assert_eq!(literal, &serde_json::json!(-1));
                    }
                    other => panic!("outputToken idx: expected Literal(-1), got {other:?}"),
                }
            }
            other => panic!("outputToken.asset.address: expected Transform, got {other:?}"),
        }

        match fields.get("inputToken.amount.value") {
            Some(ValueExpr::FromArg { from, .. }) => {
                assert_eq!(from, "$.args.amountIn");
            }
            other => panic!("inputToken.amount.value: expected FromArg, got {other:?}"),
        }

        // requires
        assert_eq!(bundle.requires.imperative, Vec::<String>::new());
        // Phase 7B: token_metadata is now an adapter capability.
        assert_eq!(bundle.requires.adapter_capabilities, vec!["token_metadata"]);
        assert_eq!(bundle.requires.host_capabilities, Vec::<String>::new());
        assert_eq!(bundle.requires.extension, ">=0.1.0");

        // Round-trip: re-serialize and parse again must equal the first parse.
        // We don't compare to the raw fixture text because field order /
        // whitespace may differ; semantic equality is what matters.
        let serialized = serde_json::to_string(&bundle).expect("serializes");
        let bundle2: AdapterFunctionBundle =
            serde_json::from_str(&serialized).expect("round-trip parses");
        assert_eq!(bundle, bundle2);
    }

    /// All 5 strategies must parse — even though only `SingleEmit` is wired
    /// for execution in Phase 1. Confirms the dispatch tag works in serde.
    /// Phase 7B added `array_emit` as the fifth variant.
    #[test]
    fn all_five_strategies_parse() {
        let cases = [
            (
                r#"{"strategy":"single_emit","category":"x","action":"y","fields":{}}"#,
                "single_emit",
            ),
            (
                r#"{"strategy":"opcode_stream_dispatch","dispatcher_id":"universal_router","mask":"0x7f","allow_revert_bit":"0x80","per_opcode_emit":{},"unknown_opcode_policy":"deny"}"#,
                "opcode_stream_dispatch",
            ),
            (
                r#"{"strategy":"enum_tagged_dispatch","dispatcher_id":"balancer_v2","tag_path":"$.args.userData","tag_decoder":"uint256_at_offset_0","per_variant_emit":{},"unknown_variant_policy":"deny"}"#,
                "enum_tagged_dispatch",
            ),
            (
                r#"{"strategy":"multicall_recurse","recurse_rule_id":"self_array_bytes_last_arg","max_depth":3}"#,
                "multicall_recurse",
            ),
            (
                r#"{"strategy":"array_emit","category":"misc","action":"permit","array_path":"$.args.transferDetails","max_elements":64,"fields":{}}"#,
                "array_emit",
            ),
        ];

        for (json, label) in cases {
            let parsed: EmitRule =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("{label} did not parse: {e}"));
            let reserialized = serde_json::to_string(&parsed).expect("serializes");
            let reparsed: EmitRule = serde_json::from_str(&reserialized)
                .unwrap_or_else(|e| panic!("{label} round-trip failed: {e}"));
            assert_eq!(parsed, reparsed, "{label} round-trip mismatch");
        }
    }

    /// `array_emit` parses with the right field shapes. `parallel_paths` is
    /// `#[serde(default)]` so an omitted key yields an empty map; supplying
    /// it round-trips. Phase 7B.
    #[test]
    fn array_emit_strategy_parses() {
        // parallel_paths omitted → defaults to empty.
        let no_parallel: EmitRule = serde_json::from_str(
            r#"{"strategy":"array_emit","category":"misc","action":"transfer","array_path":"$.args.transferDetails","max_elements":32,"fields":{"from":{"from":"$.args.element[0]"}}}"#,
        )
        .expect("array_emit (no parallel) parses");
        match &no_parallel {
            EmitRule::ArrayEmit {
                category,
                action,
                array_path,
                max_elements,
                parallel_paths,
                fields,
            } => {
                assert_eq!(category, "misc");
                assert_eq!(action, "transfer");
                assert_eq!(array_path, "$.args.transferDetails");
                assert_eq!(*max_elements, 32);
                assert!(parallel_paths.is_empty());
                assert_eq!(fields.len(), 1);
            }
            other => panic!("expected ArrayEmit, got {other:?}"),
        }

        // parallel_paths present → captured.
        let with_parallel: EmitRule = serde_json::from_str(
            r#"{"strategy":"array_emit","category":"misc","action":"permit","array_path":"$.args.permit[0]","max_elements":64,"parallel_paths":{"td":"$.args.transferDetails"},"fields":{}}"#,
        )
        .expect("array_emit (parallel) parses");
        match &with_parallel {
            EmitRule::ArrayEmit { parallel_paths, .. } => {
                assert_eq!(
                    parallel_paths.get("td").map(String::as_str),
                    Some("$.args.transferDetails")
                );
            }
            other => panic!("expected ArrayEmit, got {other:?}"),
        }

        // Round-trip: re-serialize then re-parse must equal.
        let reserialized = serde_json::to_string(&with_parallel).expect("serializes");
        let reparsed: EmitRule = serde_json::from_str(&reserialized).expect("round-trip parses");
        assert_eq!(with_parallel, reparsed);
    }

    /// ValueExpr untagged dispatch — each shape parses to the right variant.
    #[test]
    fn value_expr_untagged_dispatch() {
        let literal: ValueExpr = serde_json::from_str(r#"{"literal":"erc20"}"#).unwrap();
        assert!(matches!(literal, ValueExpr::Literal { .. }));

        let from_arg: ValueExpr = serde_json::from_str(r#"{"from":"$.args.x"}"#).unwrap();
        assert!(matches!(from_arg, ValueExpr::FromArg { .. }));

        let from_arg_full: ValueExpr = serde_json::from_str(
            r#"{"from":"$.args.x","via":"host:token_metadata","kind":"exact"}"#,
        )
        .unwrap();
        match from_arg_full {
            ValueExpr::FromArg { from, via, kind } => {
                assert_eq!(from, "$.args.x");
                assert_eq!(via.as_deref(), Some("host:token_metadata"));
                assert_eq!(kind.as_deref(), Some("exact"));
            }
            _ => panic!("expected FromArg"),
        }

        let transform: ValueExpr = serde_json::from_str(
            r#"{"fn":"select_address","args":[{"from":"$.args.path"},{"literal":0}]}"#,
        )
        .unwrap();
        match transform {
            ValueExpr::Transform { function, args } => {
                assert_eq!(function, BuiltinFn::SelectAddress);
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected Transform"),
        }
    }

    /// Phase 2 (B3) — the wire string `"unfold_velo_v2_path"` must
    /// deserialize to [`BuiltinFn::UnfoldVeloV2Path`]. This proves serde's
    /// `snake_case` rename rule renders the `V2` segment as `v2` (digits
    /// stay attached to the preceding letter), so a Phase 3 Tier A bundle
    /// `{ "fn": "unfold_velo_v2_path", ... }` parses correctly. Also
    /// round-trips back to the same wire string.
    #[test]
    fn builtin_fn_unfold_velo_v2_path_serde_roundtrip() {
        let parsed: BuiltinFn = serde_json::from_str(r#""unfold_velo_v2_path""#).expect("parses");
        assert_eq!(parsed, BuiltinFn::UnfoldVeloV2Path);

        let serialized = serde_json::to_string(&BuiltinFn::UnfoldVeloV2Path).expect("serializes");
        assert_eq!(serialized, r#""unfold_velo_v2_path""#);

        // Embedded in a `Transform` ValueExpr — the shape a Phase 3 bundle
        // uses: `{"fn":"unfold_velo_v2_path","args":[{from},{literal}]}`.
        let transform: ValueExpr = serde_json::from_str(
            r#"{"fn":"unfold_velo_v2_path","args":[{"from":"$.args.path"},{"literal":"first_token"}]}"#,
        )
        .expect("transform parses");
        match transform {
            ValueExpr::Transform { function, args } => {
                assert_eq!(function, BuiltinFn::UnfoldVeloV2Path);
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected Transform, got {other:?}"),
        }
    }
}
