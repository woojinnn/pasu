//! `#[wasm_bindgen]` JSON-string exports for the v3 declarative adapter pipeline.
//!
//! v3 surface (registry-v2, PDF FSM `Action` tree):
//!   * `declarative_install_v3_json(bundle_json: String) -> String` —
//!     stores the raw v3 manifest (`type: "adapter_action"`,
//!     `schema_version: "3"`, hierarchical `emit.body`) in
//!     `DECLARATIVE_V3_STATE` and registers the `(chain_id, to, selector)`
//!     callkey → bundle_id bridge.
//!
//!   * `declarative_route_request_v3_json(input_json: String) -> String` —
//!     orchestrator entry. Looks the callkey up in the bridge, decodes the
//!     raw calldata against the bundle's `abi_fragment.abi`, runs the
//!     emit-rule against the decoded args via `mappers::declarative::
//!     action_builder`, and returns the resulting `Vec<Action>` (PDF FSM
//!     `policy_transition::action::Action`).
//!
//! Wire shape (input/output) is documented inline next to each export.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use mappers::declarative::action_builder::{
    build_action_body, build_array_emit, substitute_placeholders,
    UnknownOpcodePolicy as V3UnknownOpcodePolicy, V3MapContext,
};
use mappers::declarative::args_json::args_to_json;
use mappers::declarative::types::BundleMatch;
use wasm_bindgen::prelude::*;

use crate::dto::{
    DeclarativeInstallResultDto, DeclarativeRouteRequestV3InputDto,
    DeclarativeRouteRequestV3ResultDto, DeclarativeRouteTypedDataV3InputDto, EngineErrorDto,
    Envelope,
};
use crate::exports::check_input_size;

// v3 action tree imports. Namespaced under `v3_action` for readability.
use policy_state::live_field::{DataSource, LiveField, OracleProvider};
use policy_state::primitives::{
    Address as V3Address, ChainId as V3ChainId, Time as V3Time, U256 as V3U256,
};
use policy_transition::action as v3_action;

/// Reserved selector key for **bare native transfers** (B.3). A tx with EMPTY
/// calldata (`"0x"` / absent) and `value > 0` has NO 4-byte function selector,
/// so it cannot be keyed by a real selector. Such a call is keyed under this
/// sentinel — the all-zero 4-byte word, which still satisfies the `"0x" + 8 hex`
/// `SELECTOR_RE` that build-index and the SW bundle parser enforce, so no
/// validation has to relax. It can never collide with a genuine dispatch: a
/// real selector requires ≥4 calldata bytes, but the route only substitutes
/// this sentinel when calldata is EMPTY. A manifest opts in by declaring
/// `match.selector = "0x00000000"` (e.g. the HyperLiquid HYPE system address's
/// payable `receive()`).
const NATIVE_TRANSFER_SELECTOR: &str = "0x00000000";

/// Bridge key: `(chain_id, to_lowercase, selector_lowercase)`.
/// `to` is normalised to lowercase hex (no checksum) and `selector` to
/// lowercase `"0x" + 8 hex` so the lookup is case-insensitive — the spec lets
/// bundles carry checksummed addresses and the orchestrator side has no
/// reason to roundtrip the case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BridgeKey {
    chain_id: u64,
    to: String,
    selector: String,
}

/// Selector-only bridge key: `(chain_id, selector_lowercase)`.
///
/// Used by the **address-agnostic** route tier (standard NFT
/// `setApprovalForAll`, selector `0xa22cb465`): the security meaning of granting
/// an operator control of an entire collection is identical for every contract,
/// so the route must decode it regardless of whether the collection address was
/// pre-registered as a per-address [`BridgeKey`]. Manifests opt in via
/// `match.address_agnostic`; the route consults this map ONLY after the exact
/// per-address `bridge` lookup misses, so existing routing stays byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SelectorKey {
    chain_id: u64,
    selector: String,
}

/// Typed-data bridge key:
/// `(chain_id, verifying_contract_lowercase, primary_type, witness_type?)`.
///
/// Parallel to [`BridgeKey`] but for the off-chain EIP-712 path. `verifying_contract`
/// is normalised to lowercase hex (no checksum) so the lookup is case-insensitive
/// — manifests may carry checksummed addresses while the orchestrator side sends
/// whatever the wallet surfaced. `primary_type` is the EIP-712 `primaryType` string
/// (e.g. `"PermitSingle"`, `"PermitWitnessTransferFrom"`,
/// `"HyperliquidTransaction:UsdSend"`) — kept verbatim (NOT lowered) since it is the
/// exact discriminator the wallet's `eth_signTypedData` payload carries.
///
/// `witness_type` (T1) is the OPTIONAL 4th component. Permit2
/// `permitWitnessTransferFrom` witnesses (UniswapX intent orders etc.) ALL share
/// the `(chain_id, Permit2, "PermitWitnessTransferFrom")` triple — the actual
/// order type is the EIP-712 `witness` field's type. `witness_type` carries that
/// struct's type name (kept VERBATIM, like `primary_type`) to disambiguate.
/// `None` for every non-witness payload, so the key matches exactly as it did
/// pre-T1 (backward compatible: `None`-on-both manifest+input hashes/compares
/// identically to the old 3-tuple).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TypedDataBridgeKey {
    chain_id: u64,
    verifying_contract: String,
    primary_type: String,
    witness_type: Option<String>,
}

// ───────────────────────────────────────────────────────────────────────────
// M2 — v3 declarative state
// ───────────────────────────────────────────────────────────────────────────
//
// v3 (PDF FSM hierarchical `ActionBody`) state:
//
// * `bridge`  — `(chain_id, to_lower, selector_lower) -> bundle_id`. The
//   bundle_id is the canonical registry id (e.g. `"uniswap/v2-router-02/
//   swapExactTokensForTokens@1.0.0"`), used as decoder_id in the v3 path.
// * `bundles` — `bundle_id -> raw manifest JSON`. We keep the manifest
//   untyped here because the v3 `emit.body` / `emit.live_inputs` /
//   `emit.per_opcode_body` shapes are templates the action_builder consumes
//   directly. Typed `AdapterFunctionBundle` parsing would discard the v3
//   fields entirely (they are not part of EmitRule). The v3 install therefore
//   only validates the structural envelope (`id`, `match`, `abi_fragment`,
//   `emit.strategy`) and trusts the action_builder + serde_json::from_value
//   to surface schema errors at route time.

#[derive(Default)]
struct DeclarativeV3State {
    /// `(chain_id, to_lower, selector_lower)` → `bundle_id`. Populated by
    /// [`declarative_install_v3_json`] via [`BundleMatch::entries`] — the
    /// dual-schema (`chain_to_addresses` / `chain_ids × to`) split is
    /// invisible here.
    bridge: HashMap<BridgeKey, String>,
    /// `bundle_id` → raw manifest JSON. Stored as `serde_json::Value` (not
    /// the strongly-typed `AdapterFunctionBundle`) because the v3 templates
    /// (`emit.body`, `emit.live_inputs`, `emit.per_opcode_body`) are not
    /// modelled in `EmitRule` — the action_builder consumes them as-is.
    bundles: HashMap<String, serde_json::Value>,
    /// `(chain_id, verifying_contract_lower, primary_type)` → `bundle_id`.
    /// Parallel off-chain EIP-712 routing table. Populated by
    /// [`declarative_install_v3_json`] only for manifests carrying a
    /// `match.typed_data` block (Phase A.1) — calldata-only manifests leave it
    /// empty. [`declarative_route_typed_data_v3_json`] resolves a wallet
    /// `eth_signTypedData` payload through it.
    typed_data_bridge: HashMap<TypedDataBridgeKey, String>,
    /// `(chain_id, selector_lower)` → `bundle_id`. Populated ONLY for manifests
    /// with `match.address_agnostic` (standard NFT `setApprovalForAll`).
    /// [`declarative_route_request_v3_json`] consults it as a FALLBACK after the
    /// per-address [`Self::bridge`] misses — it never shadows an exact
    /// per-address hit, so existing per-address routing is unchanged.
    selector_bridge: HashMap<SelectorKey, String>,
}

thread_local! {
    /// v3 install table. Single instance per WASM module lifetime (one per
    /// SW lifetime in the extension).
    static DECLARATIVE_V3_STATE: RefCell<DeclarativeV3State> = RefCell::new(DeclarativeV3State::default());
}

// ───────────────────────────────────────────────────────────────────────────
// M2 — `declarative_install_v3_json`
// ───────────────────────────────────────────────────────────────────────────
//
// Stores the raw manifest in `DECLARATIVE_V3_STATE` so
// [`declarative_route_request_v3_json`] can route
// against the v3 `emit.body` / `emit.live_inputs` / `emit.per_opcode_body`
// templates.
//
// The v3 install validates only the structural envelope:
//   * `bundle.id`         — required, non-empty string. Used as decoder_id.
//   * `bundle.match`      — parsed via `BundleMatch` so supported address
//                           layouts yield `(chain_id, address)` pairs.
//   * `bundle.match.selector` — required (carried inside `BundleMatch`).
// `emit.strategy` / `emit.body` / `emit.per_opcode_body` are NOT validated
// at install — they flow through `action_builder` at route time, which
// surfaces precise serde errors keyed to the field that failed.

/// Install (or replace) a v3 declarative bundle.
///
/// Input JSON shape: the full v3 manifest with `emit.strategy` ∈
/// {`single_emit`, `opcode_stream_dispatch`} and a hierarchical
/// `emit.body` (and optional `emit.live_inputs` / `emit.per_opcode_body`).
///
/// Output:
/// ```json
/// { "ok": true, "data": { "decoder_id": "<bundle_id>", "bundle_id": "<bundle_id>" } }
/// ```
/// or `{ "ok": false, "error": { "kind": "...", "message": "..." } }`.
///
/// v3 does not mint a separate `declarative.<path>` decoder id — the bundle_id
/// itself is the canonical key (it already disambiguates publisher / contract /
/// function / version, matching how the registry indexes manifests). Both
/// `decoder_id` and `bundle_id` are populated to the same value to preserve the
/// existing [`DeclarativeInstallResultDto`] wire shape.
#[wasm_bindgen]
pub fn declarative_install_v3_json(bundle_json: String) -> String {
    let result = (|| -> Result<DeclarativeInstallResultDto, EngineErrorDto> {
        check_input_size(&bundle_json, "declarative_install_v3_json")?;
        let bundle_value: serde_json::Value =
            serde_json::from_str(&bundle_json).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_bundle_json",
                    format!("invalid bundle json: {error}"),
                )
            })?;

        let bundle_id = bundle_value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "missing_id",
                    "bundle.id missing or not a string".to_string(),
                )
            })?
            .to_owned();

        let match_value = bundle_value.get("match").ok_or_else(|| {
            EngineErrorDto::new("invalid_match", "bundle.match missing".to_string())
        })?;
        let bundle_match: BundleMatch =
            serde_json::from_value(match_value.clone()).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_match",
                    format!("bundle.match parse failed: {error}"),
                )
            })?;

        let selector = bundle_match.selector.to_ascii_lowercase();

        // Phase A.1 — off-chain EIP-712 typed-data bridge. Manifests carrying
        // a `match.typed_data` block additionally register a
        // `(chain_id, verifying_contract, primary_type)` → bundle_id mapping so
        // [`declarative_route_typed_data_v3_json`] can route an off-chain
        // signature payload to the same emit-rule the calldata path uses.
        // calldata-only manifests omit `typed_data` and skip this entirely.
        // `witness_type` (T1) is read here too — the optional 4th key
        // component. Kept verbatim (NOT lowercased), like `primary_type`. A
        // manifest with no `witness_type` yields `None`, matching the pre-T1
        // 3-tuple key shape exactly.
        let typed_data_route: Option<(String, String, Option<String>)> = match_value
            .get("typed_data")
            .and_then(serde_json::Value::as_object)
            .and_then(|td| {
                let vc = td
                    .get("verifying_contract")
                    .and_then(serde_json::Value::as_str)?
                    .to_ascii_lowercase();
                let pt = td
                    .get("primary_type")
                    .and_then(serde_json::Value::as_str)?
                    .to_owned();
                let wt = td
                    .get("witness_type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                Some((vc, pt, wt))
            });

        DECLARATIVE_V3_STATE.with(|state| {
            let mut state = state.borrow_mut();
            for (chain_id, to) in bundle_match.entries() {
                let key = BridgeKey {
                    chain_id,
                    to: to.to_ascii_lowercase(),
                    selector: selector.clone(),
                };
                state.bridge.entry(key).or_insert_with(|| bundle_id.clone());
            }
            if let Some((ref vc, ref pt, ref wt)) = typed_data_route {
                for (chain_id, _to) in bundle_match.entries() {
                    let key = TypedDataBridgeKey {
                        chain_id,
                        verifying_contract: vc.clone(),
                        primary_type: pt.clone(),
                        witness_type: wt.clone(),
                    };
                    state
                        .typed_data_bridge
                        .entry(key)
                        .or_insert_with(|| bundle_id.clone());
                }
            }
            // Address-agnostic (selector-only) tier — standard NFT
            // `setApprovalForAll`. `bundle_match.entries()` yielded nothing (no
            // `chain_to_addresses` / `to`), so the per-address `bridge` loop
            // above registered no callkeys; instead key the bundle by
            // `(chain_id, selector)` for every declared chain so the route can
            // decode the call on ANY collection address (the `to` flows from the
            // live tx via the body's `$to`).
            if bundle_match.address_agnostic {
                for &chain_id in &bundle_match.chain_ids {
                    let key = SelectorKey {
                        chain_id,
                        selector: selector.clone(),
                    };
                    state
                        .selector_bridge
                        .entry(key)
                        .or_insert_with(|| bundle_id.clone());
                }
            }
            state.bundles.insert(bundle_id.clone(), bundle_value);
        });

        Ok(DeclarativeInstallResultDto {
            decoder_id: bundle_id.clone(),
            bundle_id,
        })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// v3 `declarative_route_request_v3_json`
// ───────────────────────────────────────────────────────────────────────────
//
// v3 route entry. Looks up the callkey through the bridge populated by
// `declarative_install_v3_json`, decodes the calldata against the bundle's
// ABI, then runs the manifest's emit-rule via `action_builder` to produce
// the hierarchical `policy_transition::action::Action` tree (PDF FSM spec).
//
// The active route entry emits the ActionBody tree directly; no legacy
// declarative route is installed in parallel.

/// v3 orchestrator entry emitting the PDF FSM `Action` tree.
///
/// Input JSON shape (see [`DeclarativeRouteRequestV3InputDto`]):
/// ```json
/// {
///   "chain_id": 1,
///   "to":       "0x7a25...",
///   "selector": "0x38ed1739",
///   "calldata": "0x38ed1739...",
///   "value":      "0",
///   "gas_limit":  "200000",
///   "gas_price":  "20000000000",
///   "submitter":  "0xaaaa...",
///   "submitted_at": 1700000000,
///   "nonce": 42,
///   "block_timestamp": 1700000010
/// }
/// ```
///
/// Output:
/// ```json
/// { "ok": true, "data": { "actions": [<Action>], "decoder_id": "" } }
/// ```
/// or `{ "ok": false, "error": { "kind": "...", "message": "..." } }`.
///
/// The active implementation decodes calldata through the installed
/// registry-v2 manifest and emit-rule. It may still emit `Unknown` when a
/// manifest explicitly falls back that way, but that is not the happy path.
#[wasm_bindgen]
pub fn declarative_route_request_v3_json(input_json: String) -> String {
    let result = (|| -> Result<DeclarativeRouteRequestV3ResultDto, EngineErrorDto> {
        check_input_size(&input_json, "declarative_route_request_v3_json")?;
        let input: DeclarativeRouteRequestV3InputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        // ── Parse + normalise ──────────────────────────────────────────────
        let submitter = parse_v3_address(&input.submitter, "submitter")?;
        let target = parse_v3_address(&input.to, "to")?;
        let value = parse_v3_u256(&input.value, "value")?;
        let gas_limit = parse_v3_u256(&input.gas_limit, "gas_limit")?;
        let gas_price = parse_v3_u256(&input.gas_price, "gas_price")?;

        let chain = V3ChainId::new(format!("eip155:{}", input.chain_id));
        let submitted_at = V3Time::from_unix(input.submitted_at);

        // ── Build ActionMeta (OnchainTx nature) ────────────────────────────
        //
        // Phase 4B wraps `gas_price` in a stub `LiveField` whose source =
        // Pyth `gas/eip155:<chain_id>`. The Sync Orchestrator is not wired
        // into this entry yet — `synced_at` collapses to `submitted_at` and
        // `ttl`/`confidence` are left at default. Phase 5+ replaces this
        // stub with a proper LiveField sourced from the Sync layer.
        let gas_price_live = LiveField::new(
            gas_price,
            DataSource::OracleFeed {
                provider: OracleProvider::Pyth,
                feed_id: format!("gas/eip155:{}", input.chain_id),
            },
            submitted_at,
        );

        let meta = v3_action::ActionMeta {
            submitted_at,
            submitter,
            nature: v3_action::ActionNature::OnchainTx {
                chain: chain.clone(),
                nonce: input.nonce,
                gas_limit,
                gas_price: gas_price_live,
                value,
            },
        };

        // ── Build ActionBody (M2 — v3 manifest lookup + action_builder) ────
        //
        // Pipeline:
        //   1. Look the callkey up in `DECLARATIVE_V3_STATE.bridge` — miss
        //      surfaces a `no_declarative_v3_mapper` error so the SW caller
        //      can fail closed or surface the gap.
        //   2. Decode the raw calldata against the manifest's
        //      `abi_fragment.abi`.
        //   3. Build a [`V3MapContext`] from the request + the decoded args
        //      (`args_to_json` keeps the stable decoded-args JSON shape).
        //   4. Dispatch on `emit.strategy`:
        //        * `single_emit`            → [`build_action_body`]
        //        * `opcode_stream_dispatch` → [`build_multicall_from_opcode_stream`]
        //      any other strategy returns `unsupported_strategy`.
        //
        // `resolved` is only populated for static, source-grounded values the
        // route path can know locally (for example WETH, V4 PoolManager, Aave
        // WTG immutable Pool). Other `$resolved.<k>` / `$derived.<k>` values
        // still surface a precise `unresolved_placeholder` until Sync wires the
        // dynamic resolver layer.
        //
        // B.3 — selector-less (bare native transfer) routing. A tx with EMPTY
        // calldata has no 4-byte selector, so the lookup uses the reserved
        // [`NATIVE_TRANSFER_SELECTOR`] sentinel instead of `input.selector`.
        // The byte vec is decoded once here (so emptiness is authoritative,
        // not the raw string) and reused for the ABI-decode pass below. A
        // selector-bearing call (≥1 calldata byte) keeps the exact prior key
        // (`input.selector`), so existing routing is byte-identical.
        let calldata_hex = input.calldata.strip_prefix("0x").unwrap_or(&input.calldata);
        let calldata_bytes = hex::decode(calldata_hex).map_err(|error| {
            EngineErrorDto::new(
                "invalid_calldata",
                format!("calldata is not valid hex: {error}"),
            )
        })?;
        let is_native_transfer = calldata_bytes.is_empty();
        let lookup_selector = if is_native_transfer {
            NATIVE_TRANSFER_SELECTOR.to_owned()
        } else {
            input.selector.to_ascii_lowercase()
        };

        let key = BridgeKey {
            chain_id: input.chain_id,
            to: input.to.to_ascii_lowercase(),
            selector: lookup_selector.clone(),
        };

        let (bundle_id, bundle_value) = DECLARATIVE_V3_STATE
            .with(|state| {
                let state = state.borrow();
                // 1. Exact per-address callkey (unchanged priority — a registered
                //    `(chain, to, selector)` always wins). 2. Address-agnostic
                //    selector-only fallback (standard NFT `setApprovalForAll`):
                //    consulted ONLY on a per-address miss, so existing routing is
                //    byte-identical.
                let bundle_id = state.bridge.get(&key).or_else(|| {
                    state.selector_bridge.get(&SelectorKey {
                        chain_id: input.chain_id,
                        selector: lookup_selector.clone(),
                    })
                });
                bundle_id.and_then(|bundle_id| {
                    state
                        .bundles
                        .get(bundle_id)
                        .cloned()
                        .map(|b| (bundle_id.clone(), b))
                })
            })
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "no_declarative_v3_mapper",
                    format!(
                        "no v3 mapper bridged for chain_id={} to={} selector={lookup_selector}",
                        input.chain_id, input.to
                    ),
                )
            })?;

        // Decode calldata against the manifest ABI. A bare
        // native transfer has NO calldata to decode against a function ABI
        // (the byte vec is empty, and `decode_with_json_abi` requires ≥4 bytes
        // for a selector), so the args object is simply empty — the
        // native-transfer body references only `$to` / `$chain` / `$calldata` /
        // `$tx.value`, never `$args.*`.
        let args_json = if is_native_transfer {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            let abi_json = bundle_value.pointer("/abi_fragment/abi").ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing abi_fragment.abi".to_string())
            })?;
            let decoded = abi_resolver::bridge::decode_with_json_abi(abi_json, &calldata_bytes)
                .map_err(|error| {
                    EngineErrorDto::new("decode_failed", format!("calldata decode failed: {error}"))
                })?;
            args_to_json(&decoded)
        };

        let emit = bundle_value
            .get("emit")
            .ok_or_else(|| EngineErrorDto::new("invalid_bundle", "missing emit".to_string()))?;
        let strategy = emit
            .get("strategy")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing emit.strategy".to_string())
            })?
            .to_owned();

        // Pre-populate well-known chain-scoped token addresses. Dynamic
        // resolved values such as pool/factory addresses are supplied by
        // higher-level sync, but static token addresses like WETH can be
        // resolved here so `$resolved.weth` placeholders avoid zero-address
        // fallback.
        let mut resolved = BTreeMap::new();
        let weth_address: Option<&'static str> = match input.chain_id {
            1 => Some("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            8453 | 10 => Some("0x4200000000000000000000000000000000000006"),
            42161 => Some("0x82af49447d8a07e3bd95bd0d56f35241523fbab1"),
            _ => None,
        };
        if let Some(addr) = weth_address {
            resolved.insert(
                "weth".to_owned(),
                serde_json::Value::String(addr.to_owned()),
            );
        }

        // B.1.c — Uniswap V4 singleton `PoolManager` per chain. It is the
        // `IPoolManager` immutable wired into the PositionManager at deploy
        // and is NEVER in `modifyLiquidities` calldata, but is fixed per chain,
        // so it is pre-populated here (mirroring the static WETH injection) for
        // the V4 manifest's `$resolved.pool_manager` venue field. Addresses are
        // the verified V4 deployments (docs UNISWAP_B1_SOURCE_RESEARCH.md §2;
        // mainnet + Base explorer "Exact Match", OP/ARB docs-table sourced).
        let v4_pool_manager: Option<&'static str> = match input.chain_id {
            1 => Some("0x000000000004444c5dc75cb358380d2e3de08a90"),
            8453 => Some("0x498581ff718922c3f8e6a244956af099b2652b2b"),
            10 => Some("0x9a13f98cb987694c9f086b1f5eb990eea8264ec3"),
            42161 => Some("0x360e68faccca8ca495c1b759fd9eee466db9fb32"),
            _ => None,
        };
        if let Some(addr) = v4_pool_manager {
            resolved.insert(
                "pool_manager".to_owned(),
                serde_json::Value::String(addr.to_owned()),
            );
        }

        // Aave WrappedTokenGatewayV3 keeps the legacy `pool` calldata argument,
        // but current verified deployments ignore it and call immutable POOL.
        // Resolve by known gateway target instead of trusting user calldata.
        if let Some(pool) = aave_weth_gateway_pool(input.chain_id, &key.to) {
            resolved.insert(
                "pool".to_owned(),
                serde_json::Value::String(pool.to_owned()),
            );
        }
        if let Some(asset) = compound_v3_base_asset(input.chain_id, &key.to) {
            resolved.insert(
                "compound_v3_base_asset".to_owned(),
                serde_json::Value::String(asset.to_owned()),
            );
        }
        if let Some(asset) = compound_v2_underlying(input.chain_id, &key.to) {
            resolved.insert(
                "compound_v2_underlying".to_owned(),
                serde_json::Value::String(asset.to_owned()),
            );
        }

        // Tier-B synthetic derivations the declarative grammar cannot express
        // (it can index/slice but not hash). Morpho Blue's `market_id` =
        // keccak(MarketParams); inject it as `$derived.morpho_market_id` so the
        // single_emit `LendingVenue::MorphoBlue.market_id` field resolves. A
        // no-op for every non-Morpho call (shape-gated on a `marketParams`
        // 5-tuple). The single_emit analogue of `maybe_inject_v4_pool_id`.
        let mut derived = BTreeMap::new();
        maybe_inject_morpho_market_id(&args_json, &mut derived);
        maybe_inject_metamorpho_underlying(&args_json, &mut derived);
        maybe_inject_uniswap_v3_path(&args_json, &mut derived);

        let ctx = V3MapContext {
            chain: chain.clone(),
            tx_to: target,
            tx_from: submitter,
            value,
            submitted_at,
            args_json: &args_json,
            // Raw tx calldata hex — referenced by the bare `$calldata`
            // placeholder so an `Unknown` body preserves the full calldata.
            raw_calldata: &input.calldata,
            resolved,
            derived,
            inputs: None,
        };

        // D-C (generalized): a manifest's `emit.reenter_callback_arg` names the
        // `bytes` arg carrying an `abi.encode(Call[])` re-entry callback the
        // `multicall_call_array` caller recurses into — manifest-driven, so the
        // engine has NO per-protocol callback-selector list.
        let reenter_callback = emit
            .get("reenter_callback_arg")
            .and_then(serde_json::Value::as_str)
            .and_then(|arg| args_json.get(arg))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        // `reenter_only` is a pure re-entry trampoline (e.g. a Morpho flash loan):
        // it has NO own body — the whole intent is in the callback the caller
        // recurses into. Emit no action; surface only the callback.
        if strategy == "reenter_only" {
            return Ok(DeclarativeRouteRequestV3ResultDto {
                actions: vec![],
                decoder_id: bundle_id,
                reenter_callback,
            });
        }

        let body = match strategy.as_str() {
            "single_emit" => {
                let body_template = emit.get("body").ok_or_else(|| {
                    EngineErrorDto::new("invalid_bundle", "missing emit.body".to_string())
                })?;
                let live_inputs_template = emit.get("live_inputs");
                build_action_body(&ctx, body_template, live_inputs_template).map_err(|error| {
                    EngineErrorDto::new("build_action_body_failed", error.to_string())
                })?
            }
            "opcode_stream_dispatch" => {
                let per_opcode_body = emit
                    .get("per_opcode_body")
                    .and_then(serde_json::Value::as_object)
                    .ok_or_else(|| {
                        EngineErrorDto::new(
                            "invalid_bundle",
                            "missing emit.per_opcode_body".to_string(),
                        )
                    })?;
                let mask = parse_hex_u8(
                    emit.get("mask")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("0xff"),
                    "emit.mask",
                )?;
                let allow_revert_bit = parse_hex_u8(
                    emit.get("allow_revert_bit")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("0x00"),
                    "emit.allow_revert_bit",
                )?;
                let unknown_policy = match emit
                    .get("unknown_opcode_policy")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("warn")
                {
                    "deny" => V3UnknownOpcodePolicy::Deny,
                    "skip" => V3UnknownOpcodePolicy::Skip,
                    _ => V3UnknownOpcodePolicy::Warn,
                };

                // Commands + inputs come from one of two shapes:
                //
                //   * Universal Router: top-level ABI args `commands` (bytes) +
                //     `inputs` (bytes[]). The default — used when the manifest
                //     omits `emit.unlock_data_source`.
                //   * Uniswap V4 `modifyLiquidities`: ONE `unlockData` bytes
                //     arg that is itself `abi.encode(bytes actions, bytes[]
                //     params)`. When `emit.unlock_data_source` is present we
                //     resolve that `$args.<name>` placeholder to the bytes hex
                //     and abi-decode it into `(actions, params[])` — the SAME
                //     `(commands, inputs[])` shape, just nested one decode deep.
                //     Everything downstream (per-opcode `inputs_abi` decode +
                //     `build_multicall_from_opcode_stream`) is identical.
                //
                // `unlock_decoded` owns the V4 path's decoded `bytes[]` so the
                // `inputs_array` borrow below outlives the match.
                let unlock_decoded: Option<Vec<serde_json::Value>>;
                let (commands_bytes, inputs_array): (Vec<u8>, &Vec<serde_json::Value>) =
                    if let Some(src) = emit
                        .get("unlock_data_source")
                        .and_then(serde_json::Value::as_str)
                    {
                        let (actions, params) = decode_v4_unlock_data(&ctx, src)
                            .map_err(|error| EngineErrorDto::new("invalid_unlock_data", error))?;
                        unlock_decoded = Some(params);
                        (actions, unlock_decoded.as_ref().expect("just set"))
                    } else {
                        let commands_str = args_json
                            .get("commands")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| {
                                EngineErrorDto::new(
                                    "invalid_args",
                                    "missing args.commands".to_string(),
                                )
                            })?;
                        let commands_bytes =
                            hex::decode(commands_str.strip_prefix("0x").unwrap_or(commands_str))
                                .map_err(|error| {
                                    EngineErrorDto::new(
                                        "invalid_commands",
                                        format!("commands not hex: {error}"),
                                    )
                                })?;
                        let inputs_array = args_json
                            .get("inputs")
                            .and_then(serde_json::Value::as_array)
                            .ok_or_else(|| {
                                EngineErrorDto::new(
                                    "invalid_args",
                                    "missing args.inputs".to_string(),
                                )
                            })?;
                        (commands_bytes, inputs_array)
                    };

                // Per-opcode `inputs_abi` ABI-decode pass (factored into
                // [`decode_stream_inputs`] so the recursive
                // [`dispatch_opcode_stream`] inner pass reuses the exact same
                // decode logic — see B.1.c.2). The v3 manifest attaches the
                // Solidity tuple signature next to each opcode's `body` /
                // `nested` template; decoding here yields a JSON value the
                // action_builder's `$inputs.<path>` placeholder walker can
                // consume. Best-effort: missing `inputs_abi`, parse failure,
                // or decode failure all degrade to `Value::Null`, which the
                // action_builder's `$inputs.<x>` walker surfaces as a clear
                // `UnresolvedPlaceholder` error rather than a silent bogus
                // default — that is the M5 manual-e2e contract.
                let decoded_inputs_array =
                    decode_stream_inputs(per_opcode_body, &commands_bytes, inputs_array, mask)?;

                // B.1.c.2 — recursive opcode-stream dispatch. The helper loops
                // the masked command bytes and, per opcode, EITHER builds a
                // leaf `body` (flat — byte-identical to the prior
                // `build_multicall_from_opcode_stream` path) OR, when the entry
                // carries `nested`, abi-decodes that opcode's inner action
                // stream and RECURSES one level deeper (UR `V4_SWAP` 0x10 =
                // `(bytes actions, bytes[] params)`; UR `EXECUTE_SUB_PLAN`
                // 0x21 = `(bytes commands, bytes[] inputs)`), producing a child
                // `ActionBody::Multicall`. `depth`/`max_depth` (default 3) is a
                // fail-loud infinite-recursion backstop.
                let max_depth = emit
                    .get("max_depth")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(3u32, |v| u32::try_from(v).unwrap_or(u32::MAX));

                dispatch_opcode_stream(
                    &ctx,
                    per_opcode_body,
                    &commands_bytes,
                    &decoded_inputs_array,
                    mask,
                    allow_revert_bit,
                    unknown_policy,
                    0,
                    max_depth,
                )?
            }
            // Phase A.2 — homogeneous-array fan-out. `emit.array_source` is a
            // `$args.<path>` placeholder resolving to a JSON array; each
            // element becomes the `$inputs` of a per-item `emit.body` build.
            // Covers calldata batch shapes (Permit2 `permitBatch` /
            // `transferFromBatch`, Balancer `batchSwap`).
            "array_emit" => {
                let array_source = emit
                    .get("array_source")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        EngineErrorDto::new(
                            "invalid_bundle",
                            "missing emit.array_source".to_string(),
                        )
                    })?;
                let per_item_body = emit.get("body").ok_or_else(|| {
                    EngineErrorDto::new("invalid_bundle", "missing emit.body".to_string())
                })?;
                let parallel_sources = emit.get("parallel_sources");
                let per_item_live_inputs = emit.get("live_inputs");
                build_array_emit(
                    &ctx,
                    array_source,
                    parallel_sources,
                    per_item_body,
                    per_item_live_inputs,
                )
                .map_err(|error| {
                    EngineErrorDto::new("build_array_emit_failed", error.to_string())
                })?
            }
            // A-redux.2 — `tagged_dispatch` (HyperLiquid CoreWriter mechanism).
            // ONE action is encoded as `data[0]=version ‖ data[tag..tag+sz]=
            // action_id (uintN BE) ‖ data[tag+sz..]=abi.encode(args)`. Resolve
            // `bytes_source` → bytes, assert the version byte, read the action
            // id, look up `per_action_body["<decimal id>"]`, abi-decode the
            // trailing args with that action's `inputs_abi` into the ctx
            // `inputs`, and build that ONE action's `body` (NOT a Multicall).
            "tagged_dispatch" => build_tagged_dispatch(&ctx, emit)?,
            // Bulker-class parallel-array tagged dispatch (Compound v3
            // `Bulker.invoke(bytes32[] actions, bytes[] data)`): the two arrays
            // are index-aligned; each `data[i]` is an action-specific ABI tuple
            // keyed by the `actions[i]` tag (a `bytes32` ASCII action-name for
            // Compound). Fans out to a `Multicall` of per-element bodies —
            // generalises `tagged_dispatch` (ONE tagged action) to an ARRAY and
            // `array_emit` (one homogeneous body per element) to a
            // PER-ELEMENT-TAGGED body. Inline decode (per-tag `inputs_abi` in
            // this one manifest) → no child callkeys, unlike `multicall_recurse`.
            "parallel_tagged_dispatch" => {
                build_parallel_tagged_dispatch(&ctx, emit, input.chain_id)?
            }
            // Cat D — `multicall_recurse` (self-multicall: NFPM / SwapRouter02 /
            // V4 PositionManager `multicall(bytes[])`). The inner sub-calls are
            // the single `bytes[]` argument, each ABI-encoded calldata targeting
            // the SAME contract. We resolve + decode + build EACH inner leg by
            // re-entering this very entrypoint (so it transparently handles every
            // strategy — single_emit, opcode_stream_dispatch, even nested
            // multicall), then wrap the flattened inner bodies in one
            // `ActionBody::Multicall`.
            "multicall_recurse" => build_multicall_recurse_body(
                input.chain_id,
                &input.to,
                &input.submitter,
                input.submitted_at,
                &args_json,
                emit,
            )?,
            // Cat D' — `multicall_call_array` (PER-LEG-TO: Bundler3
            // `multicall(Call[])`, Call = (address to, bytes data, uint256 value,
            // bool skipRevert, bytes32 callbackHash)). Unlike `multicall_recurse`
            // (same-`to`, `bytes[]` legs), each leg carries its OWN target `to`, so
            // we re-route the leg's `data` THERE (e.g. Bundler3 → GeneralAdapter1)
            // and wrap the mapped legs in one `ActionBody::Multicall`.
            "multicall_call_array" => build_multicall_call_array_body(
                input.chain_id,
                &input.submitter,
                input.submitted_at,
                &args_json,
                emit,
            )?,
            // composite_emit (Li.Fi `swapAndStartBridgeTokensViaX`): an ordered list
            // of sub-emits, each a `single_emit` or `array_emit`, flattened into ONE
            // heterogeneous `Multicall`. Models a function whose calldata carries BOTH
            // an array of swap legs (`SwapData[]` → amm::Swap × N) AND a singular
            // bridge leg (`BridgeData` → bridge::send), decoded IN-PLACE from this one
            // call's own params — NO per-child re-routing to other callkeys (unlike
            // `multicall_recurse` / `multicall_call_array`). Each part reuses the exact
            // `build_action_body` / `build_array_emit` builders; an `array_emit` part's
            // inner Multicall is flattened into the parent so children stay a flat
            // heterogeneous list the orchestrator evaluates per-child.
            "composite_emit" => {
                let parts = emit
                    .get("parts")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| {
                        EngineErrorDto::new("invalid_bundle", "missing emit.parts".to_string())
                    })?;
                let mut actions = Vec::new();
                for part in parts {
                    let part_strategy = part
                        .get("strategy")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            EngineErrorDto::new(
                                "invalid_bundle",
                                "composite_emit part missing strategy".to_string(),
                            )
                        })?;
                    let body_template = part.get("body").ok_or_else(|| {
                        EngineErrorDto::new(
                            "invalid_bundle",
                            "composite_emit part missing body".to_string(),
                        )
                    })?;
                    let live_inputs_template = part.get("live_inputs");
                    match part_strategy {
                        "single_emit" => {
                            let b = build_action_body(&ctx, body_template, live_inputs_template)
                                .map_err(|error| {
                                    EngineErrorDto::new(
                                        "build_action_body_failed",
                                        error.to_string(),
                                    )
                                })?;
                            actions.push(b);
                        }
                        "array_emit" => {
                            let array_source = part
                                .get("array_source")
                                .and_then(serde_json::Value::as_str)
                                .ok_or_else(|| {
                                    EngineErrorDto::new(
                                        "invalid_bundle",
                                        "composite_emit array_emit part missing array_source"
                                            .to_string(),
                                    )
                                })?;
                            let parallel_sources = part.get("parallel_sources");
                            let m = build_array_emit(
                                &ctx,
                                array_source,
                                parallel_sources,
                                body_template,
                                live_inputs_template,
                            )
                            .map_err(|error| {
                                EngineErrorDto::new("build_array_emit_failed", error.to_string())
                            })?;
                            if let v3_action::ActionBody::Multicall { actions: inner } = m {
                                actions.extend(inner);
                            } else {
                                actions.push(m);
                            }
                        }
                        other => {
                            return Err(EngineErrorDto::new(
                                "unsupported_strategy",
                                format!("composite_emit part strategy unsupported: {other}"),
                            ));
                        }
                    }
                }
                v3_action::ActionBody::Multicall { actions }
            }
            other => {
                return Err(EngineErrorDto::new(
                    "unsupported_strategy",
                    format!("unsupported emit.strategy: {other}"),
                ));
            }
        };

        // N2 (catch-all): a decoded body that is an EMPTY Multicall — e.g. an
        // `array_emit` over a length-0 dynamic array (Permit2 `lockdown([])`,
        // `permitBatch([])`, …) — must NOT pass through. An empty Multicall has
        // domain "multicall", survives the orchestrator's realActions filter, and
        // aggregates to PASS (no children, no policy) — a fail-open. Surface it as
        // Unknown so it warn-closes. (A strictly-empty batch is an on-chain no-op,
        // so warn-closing costs the user nothing.) `dispatch_opcode_stream` already
        // returns Unknown for an empty stream; this is the single chokepoint for
        // every other strategy.
        let body = match body {
            v3_action::ActionBody::Multicall { ref actions } if actions.is_empty() => unknown_leg(
                ctx.tx_to,
                ctx.chain.clone(),
                ctx.raw_calldata.to_string(),
                ctx.value,
            ),
            other => other,
        };

        let action = v3_action::Action { meta, body };

        Ok(DeclarativeRouteRequestV3ResultDto {
            actions: vec![action],
            decoder_id: bundle_id,
            reenter_callback,
        })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

fn aave_weth_gateway_pool(chain_id: u64, target: &str) -> Option<&'static str> {
    match (chain_id, target) {
        (1, "0xd01607c3c5ecaba394d8be377a08590149325722") => {
            Some("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2")
        }
        (10, "0x5f2508cae9923b02316254026cd43d7902866725") => {
            Some("0x794a61358d6845594f94dc1db02a252b5b4814ad")
        }
        (8453, "0xa0d9c1e9e48ca30c8d8c3b5d69ff5dc1f6dffc24") => {
            Some("0xa238dd80c259a72e81d7e4664a9801593f98d1c5")
        }
        (42161, "0x5283beced7adf6d003225c13896e536f2d4264ff") => {
            Some("0x794a61358d6845594f94dc1db02a252b5b4814ad")
        }
        _ => None,
    }
}

/// Static `$resolved.compound_v2_underlying` — the ERC-20 underlying a Compound
/// v2 cToken market lends/borrows (cETH → the native sentinel, since `CEther`
/// has no `underlying()`). Keyed by `(chain_id, cToken)`; mainnet-only because
/// Compound v2 never multi-chained. Every underlying was verified on-chain via
/// each market's `underlying()` view. The `compound_v2` analogue of
/// [`compound_v3_base_asset`]; a no-op for every non-cToken target.
fn compound_v2_underlying(chain_id: u64, target: &str) -> Option<&'static str> {
    let target = target.to_ascii_lowercase();
    match (chain_id, target.as_str()) {
        // cBAT → BAT
        (1, "0x6c8c6b02e7b2be14d4fa6022dfd6d75921d90e4e") => {
            Some("0x0d8775f648430679a709e98d2b0cb6250d2887ef")
        }
        // cDAI → DAI
        (1, "0x5d3a536e4d6dbd6114cc1ead35777bab948e3643") => {
            Some("0x6b175474e89094c44da98b954eedeac495271d0f")
        }
        // cREP → REP
        (1, "0x158079ee67fce2f58472a96584a73c7ab9ac95c1") => {
            Some("0x1985365e9f78359a9b6ad760e32412f4a445e862")
        }
        // cUSDC → USDC
        (1, "0x39aa39c021dfbae8fac545936693ac917d5e7563") => {
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        }
        // cUSDT → USDT
        (1, "0xf650c3d88d12db855b8bf7d11be6c55a4e07dcc9") => {
            Some("0xdac17f958d2ee523a2206206994597c13d831ec7")
        }
        // cWBTC (legacy) → WBTC
        (1, "0xc11b1268c1a384e55c48c2391d8d480264a3a7f4") => {
            Some("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599")
        }
        // cZRX → ZRX
        (1, "0xb3319f5d18bc0d84dd1b4825dcde5d5f7266d407") => {
            Some("0xe41d2489571d322189246dafa5ebde1f4699f498")
        }
        // cSAI → SAI
        (1, "0xf5dce57282a584d2746faf1593d3121fcac444dc") => {
            Some("0x89d24a6b4ccb1b6faa2625fe562bdd9a23260359")
        }
        // cUNI → UNI
        (1, "0x35a18000230da775cac24873d00ff85bccded550") => {
            Some("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984")
        }
        // cCOMP → COMP
        (1, "0x70e36f6bf80a52b3b46b3af8e106cc0ed743e8e4") => {
            Some("0xc00e94cb662c3520282e6f5717214004a7f26888")
        }
        // cWBTC2 → WBTC
        (1, "0xccf4429db6322d5c611ee964527d42e5d685dd6a") => {
            Some("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599")
        }
        // cTUSD → TUSD
        (1, "0x12392f67bdf24fae0af363c24ac620a2f67dad86") => {
            Some("0x0000000000085d4780b73119b644ae5ecd22b376")
        }
        // cLINK → LINK
        (1, "0xface851a4921ce59e912d19329929ce6da6eb0c7") => {
            Some("0x514910771af9ca656af840dff83e8264ecf986ca")
        }
        // cMKR → MKR
        (1, "0x95b4ef2869ebd94beb4eee400a99824bf5dc325b") => {
            Some("0x9f8f72aa9304c8b593d555f12ef6589cc3a579a2")
        }
        // cSUSHI → SUSHI
        (1, "0x4b0181102a0112a2ef11abee5563bb4a3176c9d7") => {
            Some("0x6b3595068778dd592e39a122f4f5a5cf09c90fe2")
        }
        // cAAVE → AAVE
        (1, "0xe65cdb6479bac1e22340e4e755fae7e509ecd06c") => {
            Some("0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9")
        }
        // cYFI → YFI
        (1, "0x80a2ae356fc9ef4305676f7a3e2ed04e12c33946") => {
            Some("0x0bc529c00c6401aef6d220be8c6ea1667f6ad93e")
        }
        // cUSDP → USDP (Pax Dollar)
        (1, "0x041171993284df560249b57358f931d9eb7b925d") => {
            Some("0x8e870d67f660d95d5be530380d0ec0bd388289e1")
        }
        // cFEI → FEI
        (1, "0x7713dd9ca933848f6819f38b8352d9a15ea73f67") => {
            Some("0x956f47f50a910163d8bf957cf5846d573e7f87ca")
        }
        // cETH (CEther) → native ETH sentinel
        (1, "0x4ddc2d193948926d02f9b1fe9e1daa0718270ed5") => {
            Some("0x0000000000000000000000000000000000000000")
        }
        _ => None,
    }
}

fn compound_v3_base_asset(chain_id: u64, target: &str) -> Option<&'static str> {
    let target = target.to_ascii_lowercase();
    match (chain_id, target.as_str()) {
        (1, "0xc3d688b66703497daa19211eedff47f25384cdc3") => {
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        }
        (1, "0xa17581a9e3356d9a858b789d68b4d866e593ae94") => {
            Some("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
        }
        (1, "0x5d409e56d886231adaf00c8775665ad0f9897b56") => {
            Some("0xdc035d45d973e3ec169d2276ddab16f1e407384f")
        }
        (1, "0x3afdc9bca9213a35503b077a6072f3d0d5ab0840") => {
            Some("0xdac17f958d2ee523a2206206994597c13d831ec7")
        }
        (1, "0xe85dc543813b8c2cfeaac371517b925a166a9293") => {
            Some("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599")
        }
        (1, "0x3d0bb1ccab520a66e607822fc55bc921738fafe3") => {
            Some("0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0")
        }
        (137, "0xf25212e676d1f7f89cd72ffee66158f541246445") => {
            Some("0x2791bca1f2de4661ed88a30c99a7a9449aa84174")
        }
        (137, "0xaeb318360f27748acb200ce616e389a6c9409a07") => {
            Some("0xc2132d05d31c914a87c6611c10748aeb04b58e8f")
        }
        (5000, "0x606174f62cd968d8e684c645080fa694c1d7786e") => {
            Some("0x5d3a1ff2b6bab83b63cd9ad0787074081a52ef34")
        }
        (59144, "0x8d38a3d6b3c3b7d96d6536da7eef94a9d7dbc991") => {
            Some("0x176211869ca2b568f2a7d4ee941e073a821ee1ff")
        }
        (59144, "0x60f2058379716a64a7a5d29219397e79bc552194") => {
            Some("0xe5d7c2a44ffddf6b295a15c148167daaaf5cf34f")
        }
        (534352, "0xb2f97c1bd3bf02f5e74d13f02e3e26f93d77ce44") => {
            Some("0x06efdbff2a14a7c8e15944d1f4a48f9f95f663a4")
        }
        (2020, "0x4006ed4097ee51c09a04c3b0951d28ccf19e6dfe") => {
            Some("0xc99a6a985ed2cac1ef41640596c5a5f9f4e19ef5")
        }
        (2020, "0xc0afdbd1ceb621ef576ba969ce9d4cef78dbc0c0") => {
            Some("0xe514d9deb7966c8be0ca922de8a064264ea6bcd4")
        }
        (130, "0x2c7118c4c88b9841fcf839074c26ae8f035f2921") => {
            Some("0x078d782b760474a361dda0af3839290b0ef57ad6")
        }
        (130, "0x6c987dde50db1dcdd32cd4175778c2a291978e2a") => {
            Some("0x4200000000000000000000000000000000000006")
        }
        (8453, "0x784efeb622244d2348d4f2522f8860b96fbece89") => {
            Some("0x940181a94a35a4569e4529a3cdfb74e38fd98631")
        }
        (8453, "0x9c4ec768c28520b50860ea7a15bd7213a9ff58bf") => {
            Some("0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca")
        }
        (8453, "0xb125e6687d4313864e53df431d5425969c15eb2f") => {
            Some("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913")
        }
        (8453, "0x2c776041ccfe903071af44aa147368a9c8eea518") => {
            Some("0x820c137fa70c8691f0e44dc420a5e53c168921dc")
        }
        (8453, "0x46e6b214b524310239732d51387075e0e70970bf") => {
            Some("0x4200000000000000000000000000000000000006")
        }
        (10, "0x2e44e174f7d53f0212823acc11c01a11d58c5bcb") => {
            Some("0x0b2c639c533813f4aa9d7837caf62653d097ff85")
        }
        (10, "0x995e394b8b2437ac8ce61ee0bc610d617962b214") => {
            Some("0x94b008aa00579c1307b0ef2c499ad98a8ce58e58")
        }
        (10, "0xe36a30d249f7761327fd973001a32010b521b6fd") => {
            Some("0x4200000000000000000000000000000000000006")
        }
        (42161, "0xa5edbdd9646f8dff606d7448e414884c7d905dca") => {
            Some("0xff970a61a04b1ca14834a43f5de4533ebddb5cc8")
        }
        (42161, "0x9c4ec768c28520b50860ea7a15bd7213a9ff58bf") => {
            Some("0xaf88d065e77c8cc2239327c5edb3a432268e5831")
        }
        (42161, "0xd98be00b5d27fc98112bde293e487f8d4ca57d07") => {
            Some("0xfd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb9")
        }
        (42161, "0x6f7d514bbd4aff3bcd1140b7344b32f063dee486") => {
            Some("0x82af49447d8a07e3bd95bd0d56f35241523fbab1")
        }
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Phase A.1 — `declarative_route_typed_data_v3_json`
// ───────────────────────────────────────────────────────────────────────────
//
// Off-chain EIP-712 parallel to `declarative_route_request_v3_json`. Instead
// of a calldata `(chain_id, to, selector)` callkey, it resolves a wallet
// `eth_signTypedData` payload through the `typed_data_bridge`
// `(chain_id, verifying_contract, primary_type)` populated at install time,
// reshapes the EIP-712 `message` into `args_json` via the ABI-derived wrap
// rule, then reuses the SAME `build_action_body` emit-rule engine the calldata
// path uses. The resulting body is wrapped in an `OffchainSig` meta nature.
//
// Scope (Phase A.1): `single_emit` only. `opcode_stream_dispatch` is a
// calldata-stream construct (Universal Router `execute` etc.) with no off-chain
// signature analogue, so it is rejected with `unsupported_strategy_for_typed_data`.

/// Phase A.1 — v3 off-chain typed-data route entry emitting the PDF FSM
/// `Action` tree with an `OffchainSig` nature.
///
/// Input JSON shape (see [`DeclarativeRouteTypedDataV3InputDto`]):
/// ```json
/// {
///   "chain_id": 1,
///   "verifying_contract": "0x000000000022d473030f116ddee9f6b43ac78ba3",
///   "primary_type": "PermitSingle",
///   "domain_name": "Permit2",
///   "message": { "details": { ... }, "spender": "0x...", "sigDeadline": "..." },
///   "submitter": "0xaaaa...",
///   "submitted_at": 1700000000
/// }
/// ```
///
/// Output:
/// ```json
/// { "ok": true, "data": { "actions": [<Action>], "decoder_id": "<bundle_id>" } }
/// ```
/// or `{ "ok": false, "error": { "kind": "...", "message": "..." } }`.
#[wasm_bindgen]
pub fn declarative_route_typed_data_v3_json(input_json: String) -> String {
    let result = (|| -> Result<DeclarativeRouteRequestV3ResultDto, EngineErrorDto> {
        check_input_size(&input_json, "declarative_route_typed_data_v3_json")?;
        let input: DeclarativeRouteTypedDataV3InputDto = serde_json::from_str(&input_json)
            .map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        // ── Parse + normalise ──────────────────────────────────────────────
        let submitter = parse_v3_address(&input.submitter, "submitter")?;
        let verifying_contract = parse_v3_address(&input.verifying_contract, "verifying_contract")?;
        let chain = V3ChainId::new(format!("eip155:{}", input.chain_id));
        let submitted_at = V3Time::from_unix(input.submitted_at);

        // ── Typed-data bridge lookup ────────────────────────────────────────
        //
        // `verifying_contract` is lowercased to match the install-time
        // normalisation; `primary_type` AND `witness_type` (T1) are kept
        // verbatim (they are the exact EIP-712 discriminators). `witness_type`
        // is `None` for non-witness payloads, matching the install-side key
        // exactly — so the pre-T1 3-tuple lookup is byte-for-byte unchanged. A
        // miss surfaces `no_typed_data_mapper` so the SW caller can surface the gap.
        let key = TypedDataBridgeKey {
            chain_id: input.chain_id,
            verifying_contract: input.verifying_contract.to_ascii_lowercase(),
            primary_type: input.primary_type.clone(),
            witness_type: input.witness_type.clone(),
        };

        let (bundle_id, bundle_value) = DECLARATIVE_V3_STATE
            .with(|state| {
                let state = state.borrow();
                state.typed_data_bridge.get(&key).and_then(|bundle_id| {
                    state
                        .bundles
                        .get(bundle_id)
                        .cloned()
                        .map(|b| (bundle_id.clone(), b))
                })
            })
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "no_typed_data_mapper",
                    format!(
                        "no typed-data mapper bridged for chain_id={} verifying_contract={} primary_type={} witness_type={:?}",
                        input.chain_id, input.verifying_contract, input.primary_type, input.witness_type
                    ),
                )
            })?;

        let emit = bundle_value
            .get("emit")
            .ok_or_else(|| EngineErrorDto::new("invalid_bundle", "missing emit".to_string()))?;
        let strategy = emit
            .get("strategy")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing emit.strategy".to_string())
            })?;

        // Typed-data supports single_emit OR array_emit (Phase A.2 — Permit2
        // PermitBatch is an off-chain array sig). opcode_stream_dispatch is a
        // calldata-stream construct (Universal Router `execute` etc.) with no
        // off-chain signature analogue, so it stays rejected.
        if strategy != "single_emit" && strategy != "array_emit" {
            return Err(EngineErrorDto::new(
                "unsupported_strategy_for_typed_data",
                format!(
                    "emit.strategy {strategy:?} is calldata-only; typed-data routing supports single_emit / array_emit only"
                ),
            ));
        }

        // ── Build args_json from the EIP-712 message (WRAP RULE) ───────────
        let args_json = build_typed_data_args_json(
            bundle_value.pointer("/abi_fragment/abi"),
            &input.primary_type,
            &input.message,
            emit.get("body"),
        );

        // ── V3MapContext (same resolved/derived population as calldata) ─────
        // Static WETH-by-chain pre-populate mirrors the calldata route path so
        // `$resolved.weth` substitutes the correct address.
        let mut resolved = BTreeMap::new();
        let weth_address: Option<&'static str> = match input.chain_id {
            1 => Some("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            8453 | 10 => Some("0x4200000000000000000000000000000000000006"),
            42161 => Some("0x82af49447d8a07e3bd95bd0d56f35241523fbab1"),
            _ => None,
        };
        if let Some(addr) = weth_address {
            resolved.insert(
                "weth".to_owned(),
                serde_json::Value::String(addr.to_owned()),
            );
        }
        if let Some(asset) = compound_v3_base_asset(input.chain_id, &key.verifying_contract) {
            resolved.insert(
                "compound_v3_base_asset".to_owned(),
                serde_json::Value::String(asset.to_owned()),
            );
        }

        let ctx = V3MapContext {
            chain: chain.clone(),
            tx_to: verifying_contract,
            tx_from: submitter,
            value: V3U256::ZERO,
            submitted_at,
            args_json: &args_json,
            // Off-chain EIP-712 sig has NO calldata — `$calldata` resolves to
            // an empty string here (typed-data manifests do not reference it).
            raw_calldata: "",
            resolved,
            derived: BTreeMap::new(),
            inputs: None,
        };

        // Strategy dispatch (gated above to single_emit / array_emit). For
        // `single_emit` the whole message → one body. For `array_emit` an
        // `emit.array_source` ($args.<root>.<arrayField>) homogeneous array
        // fans out to a per-item-body `Multicall` (Permit2 PermitBatch shape).
        let body = if strategy == "array_emit" {
            let array_source = emit
                .get("array_source")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    EngineErrorDto::new("invalid_bundle", "missing emit.array_source".to_string())
                })?;
            let per_item_body = emit.get("body").ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing emit.body".to_string())
            })?;
            let parallel_sources = emit.get("parallel_sources");
            let per_item_live_inputs = emit.get("live_inputs");
            build_array_emit(
                &ctx,
                array_source,
                parallel_sources,
                per_item_body,
                per_item_live_inputs,
            )
            .map_err(|error| EngineErrorDto::new("build_array_emit_failed", error.to_string()))?
        } else {
            let body_template = emit.get("body").ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing emit.body".to_string())
            })?;
            let live_inputs_template = emit.get("live_inputs");
            build_action_body(&ctx, body_template, live_inputs_template).map_err(|error| {
                EngineErrorDto::new("build_action_body_failed", error.to_string())
            })?
        };

        // ── ActionMeta (OffchainSig nature) ─────────────────────────────────
        //
        // `deadline` is best-effort from the message: `sigDeadline` (Permit2 /
        // UniswapX convention) wins, else `deadline` (EIP-2612), else 0. The
        // domain carries the wallet-supplied name + the bound chain +
        // verifying_contract; `version` / `salt` are not part of the route
        // input wire (the manifest's `match.typed_data` would carry the full
        // domain if a downstream consumer needs it).
        let deadline_secs = message_u64(&input.message, "sigDeadline")
            .or_else(|| message_u64(&input.message, "deadline"))
            .unwrap_or(0);

        let meta = v3_action::ActionMeta {
            submitted_at,
            submitter,
            nature: v3_action::ActionNature::OffchainSig {
                domain: v3_action::Eip712Domain {
                    name: input.domain_name.unwrap_or_default(),
                    version: None,
                    chain_id: Some(input.chain_id),
                    verifying_contract: Some(verifying_contract),
                    salt: None,
                },
                deadline: V3Time::from_unix(deadline_secs),
                nonce_key: None,
            },
        };

        let action = v3_action::Action { meta, body };

        Ok(DeclarativeRouteRequestV3ResultDto {
            actions: vec![action],
            decoder_id: bundle_id,
            // Typed-data signatures carry no calldata, hence no re-entry callback.
            reenter_callback: None,
        })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

/// Reshape an EIP-712 `message` object into the `args_json` the manifest's
/// `$args.<path>` placeholders expect, per the Phase A.1 WRAP RULE.
///
/// Two manifest conventions exist:
///   * **Nested** (Permit2 `PermitSingle`, UniswapX, HyperLiquid): the ABI
///     payload is a single tuple param whose content IS the EIP-712 message,
///     so `$args.<root>.<...>` placeholders need `args_json = { <root>: message }`.
///   * **Flat** (EIP-2612 `permit`): the ABI payload is multiple scalar params
///     matching the message keys directly, so `$args.spender` /  `$args.value`
///     resolve against the message as-is — `args_json = message`.
///
/// The rule is computed from `abi_fragment.abi.inputs`, filtering out the
/// signature-machinery params (`owner` / `signature` / `v` / `r` / `s`):
///   * exactly one remaining param whose type starts with `tuple` → wrap under
///     that param's name;
///   * otherwise → flat (no wrap).
///
/// Fallback (inputs missing / empty / unreadable): wrap under the
/// `primary_type` lower-camel-cased — the most common single-tuple shape.
///
/// # Positional reshape (Permit2)
///
/// A nested manifest that **reuses the calldata emit** (Permit2
/// `permitSingle` / `permitBatch`) indexes the wrapped tuple POSITIONALLY —
/// `$args.<root>[0][0]` — because the calldata path abi-decodes tuples into
/// positional arrays ([`bridge::decode_with_json_abi`] renders every tuple as a
/// JSON array). The EIP-712 `message` is a NAMED object, so a bare wrap leaves
/// `$args.<root>[0]` indexing an object → `build_action_body` errors
/// (`indexed access on non-array`). When the emit indexes the root positionally
/// we therefore reshape the named message into the SAME positional layout via
/// the ABI `components` ordering (authoritative — independent of message key
/// order). Named-path manifests (UniswapX / HyperLiquid / Pendle,
/// `$args.<root>.<field>`) keep the named object untouched, so this is a no-op
/// for them.
fn build_typed_data_args_json(
    abi: Option<&serde_json::Value>,
    primary_type: &str,
    message: &serde_json::Value,
    emit_body: Option<&serde_json::Value>,
) -> serde_json::Value {
    // Payload params = ABI inputs minus the signature machinery.
    let payload: Vec<&serde_json::Value> = abi
        .and_then(|abi| abi.get("inputs"))
        .and_then(serde_json::Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter(|i| {
                    !matches!(
                        i.get("name").and_then(serde_json::Value::as_str),
                        Some("owner" | "signature" | "v" | "r" | "s")
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Single tuple payload → wrap under its param name.
    if let [only] = payload.as_slice() {
        let ty = only
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let name = only
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if ty.starts_with("tuple") {
            if emit_indexes_root_positionally(emit_body, name) {
                return serde_json::json!({ name: reshape_named_to_positional(message, only) });
            }
            return serde_json::json!({ name: message.clone() });
        }
        // Single non-tuple param → flat (no wrap).
        return message.clone();
    }

    // Multiple scalars → flat, no wrap.
    if !payload.is_empty() {
        return message.clone();
    }

    // Fallback: inputs missing / empty / unreadable → wrap under the
    // lower-camel-cased primary_type (the dominant single-tuple shape).
    serde_json::json!({ primary_type_to_lower_camel(primary_type): message.clone() })
}

/// True when the emit body indexes the wrapped tuple `root` positionally
/// (`$args.<root>[…`), the Permit2 convention inherited from the shared calldata
/// emit. Named-path manifests reference `$args.<root>.<field>` and return false.
fn emit_indexes_root_positionally(emit_body: Option<&serde_json::Value>, root: &str) -> bool {
    if root.is_empty() {
        return false;
    }
    emit_body.is_some_and(|body| {
        serde_json::to_string(body)
            .unwrap_or_default()
            .contains(&format!("$args.{root}["))
    })
}

/// Recursively reshape a NAMED EIP-712 value into the POSITIONAL layout the
/// calldata-shared emit paths index into, driven by the ABI type node. Tuples
/// become arrays ordered by `components`; `tuple[]` / `tuple[N]` become arrays
/// of reshaped tuples; scalars (and scalar arrays) pass through unchanged. A
/// value already in array form is recursed element-wise, so a positional input
/// is idempotent.
fn reshape_named_to_positional(
    value: &serde_json::Value,
    node: &serde_json::Value,
) -> serde_json::Value {
    let ty = node
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let components = node.get("components").and_then(serde_json::Value::as_array);

    // `tuple[]` / `tuple[N]` — an array of tuples.
    if ty.starts_with("tuple") && ty.ends_with(']') {
        let Some(arr) = value.as_array() else {
            return value.clone();
        };
        return serde_json::Value::Array(
            arr.iter().map(|el| reshape_tuple(el, components)).collect(),
        );
    }
    // `tuple` — a single tuple.
    if ty == "tuple" {
        return reshape_tuple(value, components);
    }
    // Scalar / scalar array — pass through.
    value.clone()
}

/// Reshape one tuple value into a positional array ordered by `components`.
/// A named object is ordered by component name; an already-positional array is
/// recursed element-wise by index. Each field recurses through
/// [`reshape_named_to_positional`] so nested tuples flatten too.
fn reshape_tuple(
    value: &serde_json::Value,
    components: Option<&Vec<serde_json::Value>>,
) -> serde_json::Value {
    let Some(components) = components else {
        return value.clone();
    };
    if let Some(obj) = value.as_object() {
        return serde_json::Value::Array(
            components
                .iter()
                .map(|c| {
                    let cname = c
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    obj.get(cname).map_or(serde_json::Value::Null, |v| {
                        reshape_named_to_positional(v, c)
                    })
                })
                .collect(),
        );
    }
    if let Some(arr) = value.as_array() {
        return serde_json::Value::Array(
            components
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    arr.get(i).map_or(serde_json::Value::Null, |v| {
                        reshape_named_to_positional(v, c)
                    })
                })
                .collect(),
        );
    }
    value.clone()
}

/// Lower-camel-case an EIP-712 `primaryType` for the wrap-rule fallback.
///
/// Handles the EIP-712 colon-suffix convention (`"HyperliquidTransaction:UsdSend"`
/// → root type `"UsdSend"` → `"usdSend"`): the substring after the last `:`
/// is taken, then its leading character is lowercased. No `:` present →
/// the whole string's leading char is lowercased (`"PermitSingle"` →
/// `"permitSingle"`).
fn primary_type_to_lower_camel(primary_type: &str) -> String {
    let root = primary_type.rsplit(':').next().unwrap_or(primary_type);
    let mut chars = root.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Best-effort extract a `u64` from a message field that may be a JSON number
/// or a decimal string (wallets serialize EIP-712 uints either way).
fn message_u64(message: &serde_json::Value, field: &str) -> Option<u64> {
    let v = message.get(field)?;
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.parse::<u64>().ok())
}

/// Parse a "0x"-prefixed 40-hex string into an [`Address`](V3Address).
/// Wraps the alloy parser to produce a uniform `EngineErrorDto` shape.
fn parse_v3_address(raw: &str, field: &str) -> Result<V3Address, EngineErrorDto> {
    raw.parse::<V3Address>().map_err(|error| {
        EngineErrorDto::new(
            "invalid_input_json",
            format!("invalid {field} address {raw:?}: {error}"),
        )
    })
}

/// Parse a base-10 decimal string into a [`U256`](V3U256). Empty input
/// behaves like the explicit serde default (`"0"`).
fn parse_v3_u256(raw: &str, field: &str) -> Result<V3U256, EngineErrorDto> {
    if raw.is_empty() {
        return Ok(V3U256::ZERO);
    }
    V3U256::from_str_radix(raw, 10).map_err(|error| {
        EngineErrorDto::new(
            "invalid_input_json",
            format!("invalid {field} decimal {raw:?}: {error}"),
        )
    })
}

/// B.1.c.2 — per-opcode `inputs_abi` ABI-decode pass for one opcode-stream
/// level.
///
/// For each `inputs_array[i]` (a `bytes` hex string), look up the opcode (from
/// `commands_bytes[i] & mask`) in `per_opcode_body`, decode the raw bytes
/// against that entry's `inputs_abi` tuple signature via
/// [`decode_inputs_abi_tuple`], and splice a synthetic V4 `pool_id` when an
/// inline `PoolKey` is present ([`maybe_inject_v4_pool_id`]). Missing /
/// unparseable / undecodable `inputs_abi` degrades to `Value::Null` (the
/// action_builder then surfaces a precise `UnresolvedPlaceholder`).
///
/// This is the SAME decode that the outer `opcode_stream_dispatch` arm and the
/// recursive [`dispatch_opcode_stream`] inner pass both run — factored out so
/// the flat path stays byte-identical (DD5) and the inner pass is guaranteed
/// to decode inner params identically (DD3).
fn decode_stream_inputs(
    per_opcode_body: &serde_json::Map<String, serde_json::Value>,
    commands_bytes: &[u8],
    inputs_array: &[serde_json::Value],
    mask: u8,
) -> Result<Vec<serde_json::Value>, EngineErrorDto> {
    let mut decoded_inputs_array = Vec::with_capacity(inputs_array.len());
    for (i, input_hex) in inputs_array.iter().enumerate() {
        let input_hex_str = input_hex.as_str().ok_or_else(|| {
            EngineErrorDto::new("invalid_inputs", format!("inputs[{i}] not string"))
        })?;
        let input_bytes = hex::decode(input_hex_str.strip_prefix("0x").unwrap_or(input_hex_str))
            .map_err(|error| {
                EngineErrorDto::new("invalid_inputs_hex", format!("inputs[{i}]: {error}"))
            })?;

        let opcode_byte = *commands_bytes.get(i).ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_commands",
                format!("commands shorter than inputs at {i}"),
            )
        })?;
        let opcode_id = opcode_byte & mask;
        let opcode_key = format!("0x{opcode_id:02x}");

        let mut decoded_input = per_opcode_body
            .get(&opcode_key)
            .and_then(|entry| decode_inputs_for_opcode_entry(entry, &input_bytes))
            .unwrap_or(serde_json::Value::Null);
        maybe_normalize_v4_swap_params(&mut decoded_input, &opcode_key);
        // B.1.c — Uniswap V4 MINT_POSITION carries an INLINE PoolKey
        // (head-flattened currency0/currency1/fee/tickSpacing/hooks). The
        // manifest references `$inputs.pool_id`, but the manifest can't hash,
        // so compute pool_id = keccak256(abi.encode(poolKey)) in Rust and
        // splice it in. Gated on the 5 PoolKey field names being present, so
        // this is a no-op for non-MINT inputs.
        maybe_inject_v4_pool_id(&mut decoded_input);
        decoded_inputs_array.push(decoded_input);
    }
    Ok(decoded_inputs_array)
}

fn decode_inputs_for_opcode_entry(
    entry: &serde_json::Value,
    input_bytes: &[u8],
) -> Option<serde_json::Value> {
    let primary = entry.get("inputs_abi").and_then(serde_json::Value::as_str);
    let alternatives = entry
        .get("inputs_abi_alternatives")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);

    primary
        .into_iter()
        .chain(alternatives)
        .find_map(|sig| decode_inputs_abi_tuple(sig, input_bytes).ok())
}

/// V4Router swap actions are ABI-decoded as a single top-level `params` tuple
/// (`abi.decode(input, (ExactInputParams))`). The declarative bodies are more
/// stable if they can read semantic top-level names, so mirror the deployed
/// periphery shapes into the old field names after decode.
fn maybe_normalize_v4_swap_params(decoded: &mut serde_json::Value, opcode_key: &str) {
    let Some(obj) = decoded.as_object_mut() else {
        return;
    };
    let Some(params) = obj
        .get("params")
        .and_then(serde_json::Value::as_array)
        .cloned()
    else {
        return;
    };

    let mut insert = |name: &str, index: usize| -> Option<()> {
        obj.insert(name.to_owned(), params.get(index)?.clone());
        Some(())
    };

    match opcode_key {
        // SWAP_EXACT_IN_SINGLE:
        // mainnet: (poolKey, zeroForOne, amountIn, amountOutMinimum, hookData)
        // post-#497: (poolKey, zeroForOne, amountIn, amountOutMinimum, minHopPriceX36, hookData)
        "0x06" => {
            let _ = insert("poolKey", 0);
            let _ = insert("zeroForOne", 1);
            let _ = insert("amountIn", 2);
            let _ = insert("amountOutMinimum", 3);
            if let Some(last) = params.last() {
                obj.insert("hookData".to_owned(), last.clone());
            }
        }
        // SWAP_EXACT_IN:
        // mainnet: (currencyIn, path, amountIn, amountOutMinimum)
        // post-#497: (currencyIn, path, minHopPriceX36, amountIn, amountOutMinimum)
        "0x07" => {
            let has_min_hop = params.len() == 5;
            let _ = insert("currencyIn", 0);
            let _ = insert("path", 1);
            if has_min_hop {
                let _ = insert("minHopPriceX36", 2);
                let _ = insert("amountIn", 3);
                let _ = insert("amountOutMinimum", 4);
            } else {
                let _ = insert("amountIn", 2);
                let _ = insert("amountOutMinimum", 3);
            }
        }
        // SWAP_EXACT_OUT_SINGLE:
        // mainnet: (poolKey, zeroForOne, amountOut, amountInMaximum, hookData)
        // post-#497: (poolKey, zeroForOne, amountOut, amountInMaximum, minHopPriceX36, hookData)
        "0x08" => {
            let _ = insert("poolKey", 0);
            let _ = insert("zeroForOne", 1);
            let _ = insert("amountOut", 2);
            let _ = insert("amountInMaximum", 3);
            if let Some(last) = params.last() {
                obj.insert("hookData".to_owned(), last.clone());
            }
        }
        // SWAP_EXACT_OUT:
        // mainnet: (currencyOut, path, amountOut, amountInMaximum)
        // post-#497: (currencyOut, path, minHopPriceX36, amountOut, amountInMaximum)
        "0x09" => {
            let has_min_hop = params.len() == 5;
            let _ = insert("currencyOut", 0);
            let _ = insert("path", 1);
            if has_min_hop {
                let _ = insert("minHopPriceX36", 2);
                let _ = insert("amountOut", 3);
                let _ = insert("amountInMaximum", 4);
            } else {
                let _ = insert("amountOut", 2);
                let _ = insert("amountInMaximum", 3);
            }
        }
        _ => {}
    }
}

/// Build an `Unknown` body for a batch that decoded to NOTHING — an opcode
/// stream whose every opcode was unmapped+dropped, or any other strategy that
/// yielded an empty `Multicall` (e.g. an `array_emit` over a length-0 array).
/// Surfacing Unknown (instead of an empty Multicall) makes the route warn-close
/// rather than PASS (an empty Multicall has no children to evaluate, so it
/// aggregates to pass — the N2 fail-open). A PARTIAL decode keeps its mapped
/// legs (the registry `warn`=skip intent for the dropped ones).
fn unknown_leg(
    target: V3Address,
    chain: V3ChainId,
    calldata: String,
    value: V3U256,
) -> v3_action::ActionBody {
    v3_action::ActionBody::Unknown {
        target,
        chain,
        calldata,
        value,
    }
}

/// B.1.c.2 — recursive opcode-stream → `ActionBody::Multicall` dispatch.
///
/// Loops the masked `commands_bytes` and, per opcode, dispatches the matching
/// `per_opcode_body["0x<hh>"]` entry by its shape:
///
///   * `body` (no `nested`) → leaf build via the mappers
///     [`build_action_body`] with a child [`V3MapContext`] whose `inputs` is
///     `decoded_inputs[i]`. This is BYTE-IDENTICAL to the prior
///     `build_multicall_from_opcode_stream` path (same child-ctx clone, same
///     `build_action_body(_, body, None)` call) — DD5.
///   * `nested` → this opcode's input is ITSELF an opcode stream. Read the
///     inner action bytes + inner param array from `decoded_inputs[i]` (keyed
///     by `inner_actions_source` / `inner_params_source`, default
///     `$inputs.actions` / `$inputs.params`), abi-decode each inner param via
///     [`decode_stream_inputs`] against the inner `per_opcode_body`, then
///     RECURSE one level deeper (`depth + 1`). The child is an
///     `ActionBody::Multicall` (nesting is natural — `Multicall { actions }`
///     accepts a child `Multicall`).
///
/// `unknown_opcode_policy` (Deny / Warn / Skip) and the `allow_revert_bit`
/// audit-only mask mirror [`build_multicall_from_opcode_stream`].
///
/// # Errors
///
/// * `max_depth_exceeded` — `depth > max_depth` (fail-loud infinite-recursion
///   backstop, DD4).
/// * `build_multicall_failed` — an inner [`build_action_body`] failed, or an
///   unknown opcode hit under [`V3UnknownOpcodePolicy::Deny`] (kind preserved
///   from the prior wrapping so existing route-test error assertions hold).
/// * `invalid_*` — malformed inner action/param bytes.
#[allow(clippy::too_many_arguments)]
fn dispatch_opcode_stream(
    ctx: &V3MapContext<'_>,
    per_opcode_body: &serde_json::Map<String, serde_json::Value>,
    commands_bytes: &[u8],
    decoded_inputs: &[serde_json::Value],
    mask: u8,
    allow_revert_bit: u8,
    unknown_policy: V3UnknownOpcodePolicy,
    depth: u32,
    max_depth: u32,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    if depth > max_depth {
        return Err(EngineErrorDto::new(
            "max_depth_exceeded",
            format!("opcode_stream recursion exceeded max_depth={max_depth} at depth={depth}"),
        ));
    }

    let mut actions = Vec::with_capacity(commands_bytes.len());

    for (i, raw_byte) in commands_bytes.iter().enumerate() {
        let opcode = raw_byte & mask;
        let _allow_revert = (raw_byte & allow_revert_bit) != 0; // audit-only.

        let opcode_key = format!("0x{opcode:02x}");
        let Some(opcode_entry) = per_opcode_body.get(&opcode_key) else {
            match unknown_policy {
                V3UnknownOpcodePolicy::Deny => {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        format!("unknown opcode 0x{opcode:02x} (policy: deny)"),
                    ));
                }
                V3UnknownOpcodePolicy::Warn => {
                    // Drop a single unmapped opcode (the registry's `warn` policy
                    // intentionally skips benign/unmodelled plumbing opcodes). The
                    // N2 fail-open it could cause — an ALL-dropped stream becoming
                    // an empty Multicall that aggregates to PASS — is closed at the
                    // function tail: an empty result surfaces as Unknown (warn).
                    eprintln!(
                        "[declarative_exports] warn: unknown opcode 0x{opcode:02x} at index {i}"
                    );
                    continue;
                }
                V3UnknownOpcodePolicy::Skip => continue,
            }
        };

        let inputs_for_this = decoded_inputs.get(i);

        if let Some(nested) = opcode_entry.get("nested") {
            // ── Nested opcode stream: recurse one level deeper. ────────────
            let child = build_nested_multicall(
                ctx,
                nested,
                inputs_for_this,
                depth,
                max_depth,
                &opcode_key,
            )?;
            actions.push(child);
            continue;
        }

        // ── Flat leaf: byte-identical to build_multicall_from_opcode_stream.
        let body_template = opcode_entry.get("body").ok_or_else(|| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("{opcode_key}.body missing"),
            )
        })?;
        let child_ctx = V3MapContext {
            chain: ctx.chain.clone(),
            tx_to: ctx.tx_to,
            tx_from: ctx.tx_from,
            value: ctx.value,
            submitted_at: ctx.submitted_at,
            args_json: ctx.args_json,
            raw_calldata: ctx.raw_calldata,
            resolved: ctx.resolved.clone(),
            // Re-derive $derived.* from THIS opcode's decoded inputs (it carries
            // its own `path`/`marketParams`/`vault`); the route-level pass only
            // saw the outer execute(commands, inputs) args. Fixes UR V3 swap
            // token_in/out == 0x0.
            derived: rederive_for_child(&ctx.derived, inputs_for_this),
            inputs: inputs_for_this,
        };
        let child_action = build_action_body(&child_ctx, body_template, None)
            .map_err(|error| EngineErrorDto::new("build_multicall_failed", error.to_string()))?;
        actions.push(child_action);
    }

    // N2: an opcode stream that decoded to NOTHING (every opcode unmapped and
    // dropped above) must NOT become an empty `Multicall` — that aggregates to
    // PASS (fail-open). Surface a single `Unknown` so it warn-closes. A PARTIAL
    // decode keeps its mapped legs (honouring `warn`=skip for the rest).
    if actions.is_empty() {
        return Ok(unknown_leg(
            ctx.tx_to,
            ctx.chain.clone(),
            ctx.raw_calldata.to_string(),
            ctx.value,
        ));
    }
    Ok(v3_action::ActionBody::Multicall { actions })
}

/// B.1.c.2 — expand ONE `nested` opcode entry into a child
/// `ActionBody::Multicall` by decoding its inner action stream and recursing
/// [`dispatch_opcode_stream`] at `depth + 1`.
///
/// `nested` shape (all but `per_opcode_body` optional):
/// ```jsonc
/// { "inner_actions_source": "$inputs.actions",   // default
///   "inner_params_source":  "$inputs.params",    // default
///   "mask": "0xff", "allow_revert_bit": "0x00",
///   "unknown_opcode_policy": "warn",
///   "per_opcode_body": { "0x06": { ... }, ... } }
/// ```
///
/// The inner action bytes + param array are read from the ALREADY-abi-decoded
/// `parent_input` (this opcode's `inputs_abi` produced e.g. `{actions, params}`
/// for `V4_SWAP` or `{commands, inputs}` for `EXECUTE_SUB_PLAN`). The source
/// placeholders name which decoded field is the action blob vs the param array
/// (`$inputs.<field>` → `<field>`).
fn build_nested_multicall(
    ctx: &V3MapContext<'_>,
    nested: &serde_json::Value,
    parent_input: Option<&serde_json::Value>,
    depth: u32,
    max_depth: u32,
    opcode_key: &str,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    let inner_per_opcode = nested
        .get("per_opcode_body")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_bundle",
                format!("{opcode_key}.nested missing per_opcode_body"),
            )
        })?;
    let inner_mask = parse_hex_u8(
        nested
            .get("mask")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0xff"),
        "nested.mask",
    )?;
    let inner_allow_revert_bit = parse_hex_u8(
        nested
            .get("allow_revert_bit")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0x00"),
        "nested.allow_revert_bit",
    )?;
    let inner_unknown_policy = match nested
        .get("unknown_opcode_policy")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("warn")
    {
        "deny" => V3UnknownOpcodePolicy::Deny,
        "skip" => V3UnknownOpcodePolicy::Skip,
        _ => V3UnknownOpcodePolicy::Warn,
    };

    // `$inputs.<field>` → `<field>`; the inner action blob + param array are
    // looked up by that field name in the already-decoded parent input.
    let actions_field = nested
        .get("inner_actions_source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("$inputs.actions")
        .strip_prefix("$inputs.")
        .unwrap_or("actions");
    let params_field = nested
        .get("inner_params_source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("$inputs.params")
        .strip_prefix("$inputs.")
        .unwrap_or("params");

    let parent_obj = parent_input
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_nested_input",
                format!("{opcode_key}.nested: parent input did not abi-decode to an object"),
            )
        })?;
    let actions_hex = parent_obj
        .get(actions_field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_nested_input",
                format!("{opcode_key}.nested: missing inner action bytes `{actions_field}`"),
            )
        })?;
    let inner_commands_bytes = hex::decode(actions_hex.strip_prefix("0x").unwrap_or(actions_hex))
        .map_err(|error| {
        EngineErrorDto::new(
            "invalid_nested_input",
            format!("{opcode_key}.nested: inner actions not hex: {error}"),
        )
    })?;
    let inner_params = parent_obj
        .get(params_field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_nested_input",
                format!("{opcode_key}.nested: missing inner param array `{params_field}`"),
            )
        })?;

    // Decode each inner param against the inner action's `inputs_abi` — the
    // SAME pass the outer level ran (DD3: inner params are abi-decoded, never
    // handed to build_action_body as raw hex).
    let inner_decoded = decode_stream_inputs(
        inner_per_opcode,
        &inner_commands_bytes,
        inner_params,
        inner_mask,
    )?;

    dispatch_opcode_stream(
        ctx,
        inner_per_opcode,
        &inner_commands_bytes,
        &inner_decoded,
        inner_mask,
        inner_allow_revert_bit,
        inner_unknown_policy,
        depth + 1,
        max_depth,
    )
}

/// Parse a `"0x" + 1-2 hex` literal into a `u8`. Used for `emit.mask` and
/// `emit.allow_revert_bit` in v3 `opcode_stream_dispatch` manifests.
fn parse_hex_u8(raw: &str, field: &str) -> Result<u8, EngineErrorDto> {
    let stripped = raw.strip_prefix("0x").unwrap_or(raw);
    u8::from_str_radix(stripped, 16).map_err(|error| {
        EngineErrorDto::new(
            "invalid_bundle",
            format!("invalid {field} hex u8 {raw:?}: {error}"),
        )
    })
}

/// A-redux.2 — `tagged_dispatch` strategy → ONE [`ActionBody`].
///
/// Decodes a self-describing single-action envelope (HyperLiquid CoreWriter
/// `sendRawAction(bytes data)`):
///
/// ```text
/// data[0]                         = version          (asserted == version_byte)
/// data[tag_offset .. +tag_size]   = action_id        (big-endian unsigned)
/// data[tag_offset + tag_size ..]  = abi.encode(args) (decoded by the matched
///                                                     action's inputs_abi)
/// ```
///
/// `emit` keys: `bytes_source` (`$args.<name>` resolving to the bytes hex),
/// `version_byte` (`"0x01"`), `tag_offset` / `tag_size` (default `1` / `3` —
/// uint24), and `per_action_body` (`{ "<decimal id>": { name?, inputs_abi,
/// body } }`, plus an optional `"default"` fallback entry).
///
/// Fail-soft (recorded, never panics): a version-byte mismatch, an action_id
/// absent from `per_action_body`, or a `bytes_source` too short to hold the
/// tag all fall through to the `"default"` body if present, else an inline
/// `Unknown` body that preserves `$calldata` so the policy layer warns/denies
/// rather than mis-classifying. (ABI-decode failure of a MATCHED action is NOT
/// swallowed — the body's `$inputs.<x>` refs then surface a precise
/// `UnresolvedPlaceholder`, mirroring the opcode-stream contract.)
fn build_tagged_dispatch(
    ctx: &V3MapContext<'_>,
    emit: &serde_json::Value,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    let per_action_body = emit
        .get("per_action_body")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.per_action_body".to_string())
        })?;

    // ── Resolve the bytes envelope ──────────────────────────────────────────
    let bytes_source = emit
        .get("bytes_source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.bytes_source".to_string())
        })?;
    let bytes_hex_val = substitute_placeholders(ctx, &serde_json::json!(bytes_source))
        .map_err(|error| EngineErrorDto::new("invalid_bytes_source", error.to_string()))?;
    let bytes_hex = bytes_hex_val.as_str().ok_or_else(|| {
        EngineErrorDto::new(
            "invalid_bytes_source",
            format!("emit.bytes_source {bytes_source:?} did not resolve to a hex string"),
        )
    })?;
    let data = hex::decode(bytes_hex.strip_prefix("0x").unwrap_or(bytes_hex)).map_err(|error| {
        EngineErrorDto::new(
            "invalid_bytes_source",
            format!("emit.bytes_source not hex: {error}"),
        )
    })?;

    let version_byte = parse_hex_u8(
        emit.get("version_byte")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0x01"),
        "emit.version_byte",
    )?;
    let tag_offset = emit
        .get("tag_offset")
        .and_then(serde_json::Value::as_u64)
        .map_or(1usize, |v| usize::try_from(v).unwrap_or(usize::MAX));
    let tag_size = emit
        .get("tag_size")
        .and_then(serde_json::Value::as_u64)
        .map_or(3usize, |v| usize::try_from(v).unwrap_or(usize::MAX));

    // Fail-soft fallback: `"default"` per-action entry, else an inline Unknown
    // body that preserves `$calldata`.
    let fallback = |ctx: &V3MapContext<'_>| -> Result<v3_action::ActionBody, EngineErrorDto> {
        if let Some(default_entry) = per_action_body.get("default") {
            let body_template = default_entry.get("body").ok_or_else(|| {
                EngineErrorDto::new(
                    "invalid_bundle",
                    "emit.per_action_body.default missing body".to_string(),
                )
            })?;
            return build_action_body(ctx, body_template, default_entry.get("live_inputs"))
                .map_err(|error| {
                    EngineErrorDto::new("build_action_body_failed", error.to_string())
                });
        }
        let unknown = serde_json::json!({
            "domain": "unknown",
            "unknown": {
                "target": "$to",
                "chain": "$chain",
                "calldata": "$calldata",
                "value": "$tx.value"
            }
        });
        build_action_body(ctx, &unknown, None)
            .map_err(|error| EngineErrorDto::new("build_action_body_failed", error.to_string()))
    };

    // ── Version byte ────────────────────────────────────────────────────────
    match data.first() {
        Some(&v) if v == version_byte => {}
        other => {
            eprintln!(
                "[declarative_exports] tagged_dispatch: version byte {other:?} != \
                 expected 0x{version_byte:02x} — fail-soft body"
            );
            return fallback(ctx);
        }
    }

    // ── action_id (big-endian unsigned, up to 8 bytes) ──────────────────────
    let tag_end = tag_offset.saturating_add(tag_size);
    let Some(tag_bytes) = data.get(tag_offset..tag_end) else {
        eprintln!(
            "[declarative_exports] tagged_dispatch: data ({} bytes) too short for tag \
             [{tag_offset}..{tag_end}] — fail-soft body",
            data.len()
        );
        return fallback(ctx);
    };
    if tag_size == 0 || tag_size > 8 {
        return Err(EngineErrorDto::new(
            "invalid_bundle",
            format!("emit.tag_size must be 1..=8, got {tag_size}"),
        ));
    }
    let mut action_id: u64 = 0;
    for b in tag_bytes {
        action_id = (action_id << 8) | u64::from(*b);
    }

    // ── Look up the per-action entry ────────────────────────────────────────
    let action_key = action_id.to_string();
    let Some(action_entry) = per_action_body.get(&action_key) else {
        eprintln!(
            "[declarative_exports] tagged_dispatch: action_id {action_id} absent from \
             per_action_body — fail-soft body"
        );
        return fallback(ctx);
    };
    let body_template = action_entry.get("body").ok_or_else(|| {
        EngineErrorDto::new(
            "invalid_bundle",
            format!("emit.per_action_body.{action_key} missing body"),
        )
    })?;

    // ── ABI-decode the trailing args into the ctx `inputs` ──────────────────
    // `decoded` owns the value so the `Some(&decoded)` borrow outlives the
    // `build_action_body` call below. A matched action whose `inputs_abi`
    // fails to decode yields `Null` (the body's `$inputs.<x>` refs then surface
    // a precise UnresolvedPlaceholder — same best-effort contract as the
    // opcode-stream path).
    let args_bytes = data.get(tag_end..).unwrap_or(&[]);
    let decoded = action_entry
        .get("inputs_abi")
        .and_then(serde_json::Value::as_str)
        .and_then(|sig| decode_inputs_abi_tuple(sig, args_bytes).ok())
        .unwrap_or(serde_json::Value::Null);

    let child_ctx = V3MapContext {
        chain: ctx.chain.clone(),
        tx_to: ctx.tx_to,
        tx_from: ctx.tx_from,
        value: ctx.value,
        submitted_at: ctx.submitted_at,
        args_json: ctx.args_json,
        raw_calldata: ctx.raw_calldata,
        resolved: ctx.resolved.clone(),
        // Defense-in-depth: today's one tagged manifest (HL core-writer) emits no
        // $derived references, so re-deriving here is inert on the body (any stray
        // injection is value-gated + unread); it pre-resolves $derived.* for a
        // future tagged body that does reference it.
        derived: rederive_for_child(&ctx.derived, Some(&decoded)),
        inputs: Some(&decoded),
    };
    build_action_body(&child_ctx, body_template, action_entry.get("live_inputs"))
        .map_err(|error| EngineErrorDto::new("build_action_body_failed", error.to_string()))
}

/// Named static `(chain, address) -> address` resolvers — the SAME helpers the
/// outer route pre-populates into `$resolved`, re-exposed so
/// [`build_parallel_tagged_dispatch`] can compute a per-element `$resolved.*`
/// keyed by a DECODED INPUT address rather than the outer `$to`. Compound v3's
/// Bulker is the motivating case: `invoke`'s `to` IS the Bulker, but each
/// action's `comet` (hence its `base_asset`) is a per-element parameter. Returns
/// `None` for an unknown resolver name or an address the resolver does not
/// cover (a no-op augmentation, never a fail).
fn resolve_named(chain_id: u64, name: &str, address: &str) -> Option<&'static str> {
    match name {
        "compound_v3_base_asset" => compound_v3_base_asset(chain_id, address),
        "compound_v2_underlying" => compound_v2_underlying(chain_id, address),
        _ => None,
    }
}

/// Derive the `per_tag` lookup key from one element's tag value under the
/// declared `tag_encoding`:
///   * `bytes32_ascii` — the tag is a `bytes32` hex string whose bytes are an
///     ASCII action name right-zero-padded (Solidity `bytes32` string literal,
///     e.g. Compound `"ACTION_SUPPLY_ASSET"`). Hex-decode, trim trailing `0x00`,
///     UTF-8. This is the legible Compound case.
///   * `bytes32_hex` — the tag is an opaque `bytes32` (e.g. a keccak selector);
///     normalise to lowercase `0x`-prefixed hex.
///   * `uint` — a small numeric enum tag; a decimal string passes through, a
///     `0x` hex string converts to decimal.
///
/// Fail-loud on a shape mismatch (the element then surfaces `invalid_tag`).
fn derive_tag_key(tag: &serde_json::Value, encoding: &str) -> Result<String, String> {
    match encoding {
        "bytes32_ascii" => {
            let s = tag
                .as_str()
                .ok_or_else(|| format!("bytes32_ascii tag must be a hex string, got {tag}"))?;
            let bytes = hex::decode(s.strip_prefix("0x").unwrap_or(s))
                .map_err(|error| format!("bytes32_ascii tag {s:?} not hex: {error}"))?;
            let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            String::from_utf8(bytes[..end].to_vec())
                .map_err(|error| format!("bytes32_ascii tag {s:?} not UTF-8: {error}"))
        }
        "bytes32_hex" => {
            let s = tag
                .as_str()
                .ok_or_else(|| format!("bytes32_hex tag must be a hex string, got {tag}"))?;
            Ok(format!(
                "0x{}",
                s.strip_prefix("0x").unwrap_or(s).to_ascii_lowercase()
            ))
        }
        "uint" => match tag {
            serde_json::Value::Number(n) => Ok(n.to_string()),
            serde_json::Value::String(s) => {
                let s = s.trim();
                if let Some(hex) = s.strip_prefix("0x") {
                    u128::from_str_radix(hex, 16)
                        .map(|v| v.to_string())
                        .map_err(|error| format!("uint tag {s:?} not hex/in-range: {error}"))
                } else {
                    Ok(s.to_owned())
                }
            }
            other => Err(format!("uint tag must be a number or string, got {other}")),
        },
        other => Err(format!(
            "unknown tag_encoding {other:?} (expected bytes32_ascii|bytes32_hex|uint)"
        )),
    }
}

/// `parallel_tagged_dispatch` — decode a parallel-array tagged batch
/// (`fn(tag[], data[])`) into a `Multicall` of per-element bodies. The
/// motivating shape is Compound v3 `Bulker.invoke(bytes32[] actions, bytes[]
/// data)` where `actions[i]` is a `bytes32` ASCII tag selecting how to
/// ABI-decode `data[i]` (`ACTION_SUPPLY_ASSET` → `(address comet, address to,
/// address asset, uint256 amount)`, …).
///
/// emit shape:
/// ```json
/// {
///   "strategy": "parallel_tagged_dispatch",
///   "actions_source": "$args.actions",
///   "data_source":    "$args.data",
///   "tag_encoding":   "bytes32_ascii",
///   "max_elements":   32,
///   "unknown_tag_policy": "warn",
///   "resolve_from_inputs": { "compound_v3_base_asset": "comet" },
///   "per_tag": {
///     "ACTION_SUPPLY_ASSET": { "inputs_abi": "(…)", "body": { … }, "live_inputs": { … } },
///     "default": { "body": { … } }
///   }
/// }
/// ```
///
/// Posture: fail-LOUD on structural faults (length mismatch, element cap,
/// non-hex data, per-element ABI-decode failure → `$inputs.*`
/// `UnresolvedPlaceholder`); fail-VISIBLE on an unknown tag (`warn` → a visible
/// `unknown` body for that element, never a silent drop; `deny` → hard error).
/// An empty batch decodes to an empty `Multicall` (an on-chain no-op).
fn build_parallel_tagged_dispatch(
    ctx: &V3MapContext<'_>,
    emit: &serde_json::Value,
    chain_id: u64,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    const DEFAULT_MAX_ELEMENTS: u64 = 32;
    const HARD_MAX_ELEMENTS: u64 = 64;

    let per_tag = emit
        .get("per_tag")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| EngineErrorDto::new("invalid_bundle", "missing emit.per_tag".to_string()))?;

    let actions_source = emit
        .get("actions_source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.actions_source".to_string())
        })?;
    let data_source = emit
        .get("data_source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.data_source".to_string())
        })?;

    let tag_encoding = emit
        .get("tag_encoding")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.tag_encoding".to_string())
        })?;

    let max_elements = emit
        .get("max_elements")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_MAX_ELEMENTS);
    if max_elements == 0 || max_elements > HARD_MAX_ELEMENTS {
        return Err(EngineErrorDto::new(
            "invalid_bundle",
            format!("emit.max_elements must be 1..={HARD_MAX_ELEMENTS}, got {max_elements}"),
        ));
    }

    let deny_unknown = matches!(
        emit.get("unknown_tag_policy")
            .and_then(serde_json::Value::as_str),
        Some("deny")
    );

    // `resolve_from_inputs`: resolver name → the `inputs_abi` field whose decoded
    // address keys that resolver (e.g. `compound_v3_base_asset` ← `comet`).
    let resolve_from_inputs: Vec<(String, String)> = emit
        .get("resolve_from_inputs")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();

    // ── Resolve the two index-aligned arrays ────────────────────────────────
    let tags_val = substitute_placeholders(ctx, &serde_json::json!(actions_source))
        .map_err(|error| EngineErrorDto::new("invalid_actions_source", error.to_string()))?;
    let tags = tags_val.as_array().ok_or_else(|| {
        EngineErrorDto::new(
            "invalid_actions_source",
            format!("emit.actions_source {actions_source:?} did not resolve to an array"),
        )
    })?;
    let data_val = substitute_placeholders(ctx, &serde_json::json!(data_source))
        .map_err(|error| EngineErrorDto::new("invalid_data_source", error.to_string()))?;
    let data = data_val.as_array().ok_or_else(|| {
        EngineErrorDto::new(
            "invalid_data_source",
            format!("emit.data_source {data_source:?} did not resolve to an array"),
        )
    })?;

    if tags.len() != data.len() {
        return Err(EngineErrorDto::new(
            "invalid_bundle",
            format!(
                "parallel_tagged_dispatch: actions[] len {} != data[] len {}",
                tags.len(),
                data.len()
            ),
        ));
    }
    if tags.len() as u64 > max_elements {
        return Err(EngineErrorDto::new(
            "invalid_bundle",
            format!(
                "parallel_tagged_dispatch: element count {} exceeds cap {max_elements}",
                tags.len()
            ),
        ));
    }

    let mut actions: Vec<v3_action::ActionBody> = Vec::with_capacity(tags.len());
    for (index, (tag_val, data_item)) in tags.iter().zip(data.iter()).enumerate() {
        // 1. tag → per_tag lookup key.
        let key = derive_tag_key(tag_val, tag_encoding).map_err(|error| {
            EngineErrorDto::new(
                "invalid_tag",
                format!("parallel_tagged_dispatch element #{index}: {error}"),
            )
        })?;

        // 2. Per-tag entry, else `default`, else fail-visible Unknown body.
        let Some(entry) = per_tag.get(&key).or_else(|| per_tag.get("default")) else {
            if deny_unknown {
                return Err(EngineErrorDto::new(
                    "unknown_tag",
                    format!(
                        "parallel_tagged_dispatch element #{index}: tag {key:?} absent \
                         from per_tag (unknown_tag_policy=deny)"
                    ),
                ));
            }
            eprintln!(
                "[declarative_exports] parallel_tagged_dispatch element #{index}: tag {key:?} \
                 absent from per_tag — fail-soft Unknown body"
            );
            let unknown = serde_json::json!({
                "domain": "unknown",
                "unknown": {
                    "target": "$to",
                    "chain": "$chain",
                    "calldata": "$calldata",
                    "value": "$tx.value"
                }
            });
            let body = build_action_body(ctx, &unknown, None).map_err(|error| {
                EngineErrorDto::new("build_action_body_failed", error.to_string())
            })?;
            actions.push(body);
            continue;
        };

        let body_template = entry.get("body").ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_bundle",
                format!("parallel_tagged_dispatch per_tag {key:?} missing body"),
            )
        })?;

        // 3. ABI-decode this element's `data[i]` with the entry's `inputs_abi`.
        //    A decode failure → `Null`, so a body `$inputs.<x>` ref surfaces a
        //    precise UnresolvedPlaceholder (no silent bogus default) — same
        //    best-effort contract as tagged_dispatch / opcode-stream.
        let data_hex = data_item.as_str().ok_or_else(|| {
            EngineErrorDto::new(
                "invalid_data_source",
                format!("parallel_tagged_dispatch element #{index}: data not a hex string"),
            )
        })?;
        let data_bytes =
            hex::decode(data_hex.strip_prefix("0x").unwrap_or(data_hex)).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_data_source",
                    format!("parallel_tagged_dispatch element #{index}: data not hex: {error}"),
                )
            })?;
        let decoded = entry
            .get("inputs_abi")
            .and_then(serde_json::Value::as_str)
            .and_then(|sig| decode_inputs_abi_tuple(sig, &data_bytes).ok())
            .unwrap_or(serde_json::Value::Null);

        // 4. Augment a per-element `$resolved` from decoded inputs (e.g.
        //    `compound_v3_base_asset` keyed by the decoded `comet`, since `$to`
        //    is the Bulker, not the market). A no-op when the field is absent or
        //    the resolver does not cover the address.
        let mut child_resolved = ctx.resolved.clone();
        for (resolver_name, field) in &resolve_from_inputs {
            if let Some(addr) = decoded.get(field).and_then(serde_json::Value::as_str) {
                if let Some(asset) = resolve_named(chain_id, resolver_name, addr) {
                    child_resolved.insert(
                        resolver_name.clone(),
                        serde_json::Value::String(asset.to_owned()),
                    );
                }
            }
        }

        let child_ctx = V3MapContext {
            chain: ctx.chain.clone(),
            tx_to: ctx.tx_to,
            tx_from: ctx.tx_from,
            value: ctx.value,
            submitted_at: ctx.submitted_at,
            args_json: ctx.args_json,
            raw_calldata: ctx.raw_calldata,
            resolved: child_resolved,
            // Defense-in-depth: today's one parallel-tagged manifest (Compound
            // Bulker) emits no $derived references, so re-deriving here is inert on
            // the body; it pre-closes the same clone-without-re-derive footgun that
            // hit the opcode-stream path for any future element body that uses $derived.*.
            derived: rederive_for_child(&ctx.derived, Some(&decoded)),
            inputs: Some(&decoded),
        };
        let body = build_action_body(&child_ctx, body_template, entry.get("live_inputs"))
            .map_err(|error| EngineErrorDto::new("build_action_body_failed", error.to_string()))?;
        actions.push(body);
    }

    Ok(v3_action::ActionBody::Multicall { actions })
}

/// Decode a single opcode's `inputs_abi` Solidity tuple signature against a
/// raw byte buffer, returning a JSON object keyed by the tuple's named
/// fields.
///
/// Implementation notes:
///   * Reuse [`abi_resolver::decode::decode_with_function`] so we do not pull
///     `alloy_json_abi` / `alloy_dyn_abi` symbols into the WASM surface beyond
///     what abi-resolver already links.
///   * The signature is wrapped into a synthetic `step<sig>` function so alloy
///     can parse it (mirrors `subdecode::opcode_stream`'s pattern). Selector is
///     recomputed from that function so `decode_with_function`'s
///     selector-equality guard always passes; opcode dispatch already verified
///     the outer call site, and this helper only re-decodes the inner tuple.
///   * Each `DecodedArg.value` (a `DynSolValue`) routes through the shared
///     `bridge::convert_value` -> `eval::decoded_value_to_json` chain, so the
///     resulting `$inputs.<name>` JSON shape matches the `$args.<name>` view the
///     action_builder's placeholder walker already understands.
///   * Best-effort: any parse / decode / convert failure returns `Err` and
///     the caller substitutes `Value::Null`. The action_builder then
///     surfaces a precise `UnresolvedPlaceholder` for `$inputs.<x>`
///     references instead of producing a silent bogus default.
fn decode_inputs_abi_tuple(
    inputs_abi: &str,
    input_bytes: &[u8],
) -> Result<serde_json::Value, String> {
    use alloy_json_abi::Function;

    let synthetic = format!("step{inputs_abi}");
    let function =
        Function::parse(&synthetic).map_err(|error| format!("parse {inputs_abi:?}: {error}"))?;
    let selector = function.selector().0;

    let mut prefixed = Vec::with_capacity(4 + input_bytes.len());
    prefixed.extend_from_slice(&selector);
    prefixed.extend_from_slice(input_bytes);

    let decoded = abi_resolver::decode::decode_with_function(&function, &prefixed)
        .map_err(|error| format!("decode {inputs_abi:?}: {error}"))?;

    let mut obj = serde_json::Map::with_capacity(decoded.args.len());
    for arg in &decoded.args {
        // `bridge::convert_arg` both converts the `DynSolValue` AND rebuilds the
        // canonical ABI type from `sol_type` + `components` (`canonical_abi_type`)
        // — the bare `decode::DecodedArg.sol_type` is `"tuple"` for compound
        // params, so we need the rebuilt `abi_type` to thread per-field widths.
        let converted = abi_resolver::bridge::convert_arg(arg.clone())
            .map_err(|error| format!("convert {inputs_abi:?}.{}: {error}", arg.name))?;
        // Thread the per-field ABI width (`uint24` / `int24` / `uint48` ...) so
        // narrow ints render as JSON NUMBERS, matching `args_to_json`'s
        // `decoded_value_to_json_typed` convention (commit 6a24f09). Without it
        // a `uint24 fee` / `int24 tickSpacing|tickLower` collapses to a decimal
        // string — which both fails the `i32 RangeSpec::Tick.lower` deserialize
        // AND breaks `compute_v4_pool_id`'s `as_u64`/`as_i64` reads. `uint256`
        // / `address` / `bytes` / `bool` are unaffected (string/bool either
        // way), so existing UR opcode-stream bodies keep their shape.
        obj.insert(
            converted.name.clone(),
            mappers::declarative::args_json::decoded_value_to_json_typed(
                &converted.value,
                &converted.abi_type,
            ),
        );
    }
    Ok(serde_json::Value::Object(obj))
}

/// B.1.c — decode a Uniswap V4 `unlockData` bytes arg into `(actions, params)`.
///
/// `modifyLiquidities(bytes unlockData, ...)` carries `unlockData =
/// abi.encode(bytes actions, bytes[] params)` (verified against
/// `BaseActionsRouter._unlockCallback` / `CalldataDecoder.
/// decodeActionsRouterParams`). `src` is the `$args.<name>` placeholder naming
/// the bytes arg (`"$args.unlockData"`); we resolve it against `ctx`, hex-decode
/// the bytes, then ABI-decode the canonical `(bytes, bytes[])` tuple.
///
/// Returns `(actions_bytes, params_hex_array)` where `actions_bytes` is the
/// packed one-byte-per-action command blob (used as `commands_bytes`) and
/// `params_hex_array` is the parallel `bytes[]` of per-action tuples (used as
/// `inputs_array`). This reuses [`decode_inputs_abi_tuple`] — which routes
/// through `abi_resolver` — so this crate never names `DynSolValue` directly
/// (matching the `alloy-dyn-abi`-is-dev-only constraint).
///
/// A standard `(bytes, bytes[])` ABI decode matches the wire: the
/// `OFFSET_OR_LENGTH_MASK` (`& 0xffffffff`) the strict on-chain decoder applies
/// is irrelevant for honest calls (offsets never exceed 32 bits).
fn decode_v4_unlock_data(
    ctx: &V3MapContext<'_>,
    src: &str,
) -> Result<(Vec<u8>, Vec<serde_json::Value>), String> {
    // Resolve the `$args.<name>` placeholder to the unlockData bytes hex.
    let unlock_hex = substitute_placeholders(ctx, &serde_json::Value::String(src.to_owned()))
        .map_err(|error| format!("resolve {src:?}: {error}"))?;
    let unlock_str = unlock_hex
        .as_str()
        .ok_or_else(|| format!("{src:?} did not resolve to a bytes hex string"))?;
    let unlock_bytes = hex::decode(unlock_str.strip_prefix("0x").unwrap_or(unlock_str))
        .map_err(|error| format!("unlockData not hex: {error}"))?;

    // ABI-decode `(bytes actions, bytes[] params)` via the shared tuple decoder.
    let decoded = decode_inputs_abi_tuple("(bytes actions, bytes[] params)", &unlock_bytes)?;

    let actions_str = decoded
        .get("actions")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "decoded unlockData missing `actions` bytes".to_string())?;
    let actions_bytes = hex::decode(actions_str.strip_prefix("0x").unwrap_or(actions_str))
        .map_err(|error| format!("actions not hex: {error}"))?;

    let params = decoded
        .get("params")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "decoded unlockData missing `params` array".to_string())?
        .clone();

    if actions_bytes.len() != params.len() {
        return Err(format!(
            "InputLengthMismatch: {} actions vs {} params",
            actions_bytes.len(),
            params.len()
        ));
    }

    Ok((actions_bytes, params))
}

/// B.1.c / B.1.c.2 — if a decoded V4 action's inputs object carries an inline
/// `PoolKey`, compute `pool_id = keccak256(abi.encode(poolKey))` and splice it
/// in as a synthetic top-level `pool_id` field.
///
/// Two carry shapes are recognised:
///   * HEAD-FLATTENED (`MINT_POSITION` 0x02 inside `modifyLiquidities`): the
///     `currency0`/`currency1`/`fee`/`tickSpacing`/`hooks` fields sit at the
///     TOP level of the decoded inputs object.
///   * NESTED (`SWAP_EXACT_IN_SINGLE` 0x06 / `SWAP_EXACT_OUT_SINGLE` 0x08
///     inside a UR `V4_SWAP`): the struct's first member is a `PoolKey poolKey`
///     sub-tuple, so the 5 fields live under a nested `poolKey` object.
///
/// The top-level shape is tried first (byte-identical to the prior
/// MINT-only behaviour); the nested `poolKey` shape is the B.1.c.2 addition.
///
/// `PoolId.toId` is `keccak256(poolKey, 0xa0)` over the 5 contiguous 32-byte
/// slots — identical to `keccak256(abi.encode(poolKey))` since `PoolKey` has no
/// dynamic members (`cast`-verified 0xa0 length). Actions carrying NO PoolKey
/// (INCREASE/DECREASE/BURN, SETTLE/TAKE, multi-hop SWAP_EXACT_IN) hit neither
/// gate and stay a no-op (their `pool_id` keeps the manifest's `"unknown"`
/// sentinel).
///
/// The canonical 0xa0 encoding is built by hand (address left-pad, `uint24`
/// big-endian, `int24` two's-complement big-endian) so this crate keeps
/// `alloy-dyn-abi` out of its non-dev surface — only `alloy_primitives::
/// keccak256` (a regular dep) is used.
fn maybe_inject_v4_pool_id(decoded: &mut serde_json::Value) {
    let Some(obj) = decoded.as_object() else {
        return;
    };

    // Extract the 5 PoolKey fields from a NAMED object (head-flattened MINT).
    fn pool_id_from_obj(src: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
        let c0 = src.get("currency0").and_then(serde_json::Value::as_str)?;
        let c1 = src.get("currency1").and_then(serde_json::Value::as_str)?;
        let fee = src.get("fee")?;
        let spacing = src.get("tickSpacing")?;
        let hooks = src.get("hooks").and_then(serde_json::Value::as_str)?;
        compute_v4_pool_id(c0, c1, fee, spacing, hooks)
    }
    // Extract from a POSITIONAL array (a nested `poolKey` tuple — alloy renders
    // nested tuples as positional JSON arrays `[c0, c1, fee, tickSpacing,
    // hooks]`, NOT named objects).
    fn pool_id_from_arr(arr: &[serde_json::Value]) -> Option<String> {
        let c0 = arr.first()?.as_str()?;
        let c1 = arr.get(1)?.as_str()?;
        let fee = arr.get(2)?;
        let spacing = arr.get(3)?;
        let hooks = arr.get(4)?.as_str()?;
        compute_v4_pool_id(c0, c1, fee, spacing, hooks)
    }

    // Top-level (MINT head-flatten) first, then nested `poolKey` (older
    // V4_SWAP manifest shape), then the deployed V4Router single-arg `params`
    // tuple where params[0] is the PoolKey.
    let pool_id = pool_id_from_obj(obj)
        .or_else(|| {
            obj.get("poolKey")
                .and_then(serde_json::Value::as_array)
                .and_then(|arr| pool_id_from_arr(arr))
        })
        .or_else(|| {
            obj.get("params")
                .and_then(serde_json::Value::as_array)
                .and_then(|params| params.first())
                .and_then(serde_json::Value::as_array)
                .and_then(|pool_key| pool_id_from_arr(pool_key))
        });
    let Some(pool_id) = pool_id else {
        return;
    };
    if let Some(obj_mut) = decoded.as_object_mut() {
        obj_mut.insert("pool_id".to_owned(), serde_json::Value::String(pool_id));
    }
}

/// Build the canonical 0xa0 `abi.encode(PoolKey)` and return
/// `0x` + `keccak256` hex, or `None` on any malformed field.
///
/// `fee` (`uint24`, ≤64 bits) arrives as a JSON number; `tickSpacing` (`int24`)
/// as a (possibly negative) JSON number — both per `eval::*_to_json`'s
/// width-aware rendering. Addresses are `0x`-prefixed 20-byte hex.
fn compute_v4_pool_id(
    currency0: &str,
    currency1: &str,
    fee: &serde_json::Value,
    tick_spacing: &serde_json::Value,
    hooks: &str,
) -> Option<String> {
    let mut buf = [0u8; 0xa0]; // 5 × 32 bytes.

    write_address_word(&mut buf[0x00..0x20], currency0)?;
    write_address_word(&mut buf[0x20..0x40], currency1)?;
    // fee: uint24, zero-extended big-endian in the low 3 bytes of the word.
    let fee_u = fee.as_u64()?;
    buf[0x5d..0x60].copy_from_slice(&fee_u.to_be_bytes()[5..8]);
    // tickSpacing: int24, sign-extended two's-complement big-endian.
    let spacing_i = tick_spacing.as_i64()?;
    let spacing_bytes = spacing_i.to_be_bytes(); // i64 two's-complement, 8 bytes.
    let fill = if spacing_i < 0 { 0xffu8 } else { 0x00u8 };
    for b in &mut buf[0x60..0x80] {
        *b = fill;
    }
    // Low 3 bytes hold the int24; the sign-extension above covers the rest.
    buf[0x7d..0x80].copy_from_slice(&spacing_bytes[5..8]);
    write_address_word(&mut buf[0x80..0xa0], hooks)?;

    Some(format!(
        "0x{}",
        hex::encode(alloy_primitives::keccak256(buf))
    ))
}

/// Write a `0x`-prefixed 20-byte address into a 32-byte word slot (left-padded
/// with 12 zero bytes). Returns `None` if the hex is malformed.
fn write_address_word(word: &mut [u8], addr_hex: &str) -> Option<()> {
    let stripped = addr_hex.strip_prefix("0x").unwrap_or(addr_hex);
    let bytes = hex::decode(stripped).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    word[12..32].copy_from_slice(&bytes);
    Some(())
}

/// Write a base-10 `uint256` (decimal-string, as [`args_to_json`] renders any
/// `uint` wider than 64 bits) into a 32-byte big-endian word. Returns `None` on
/// a malformed value.
fn write_u256_word(word: &mut [u8], value: &serde_json::Value) -> Option<()> {
    let v = match value {
        serde_json::Value::String(s) => V3U256::from_str_radix(s, 10).ok()?,
        // `uint <= 64` would arrive as a JSON number; `lltv` is `uint256` so the
        // string arm is the live path, but accept a number defensively.
        serde_json::Value::Number(n) => V3U256::from(n.as_u64()?),
        _ => return None,
    };
    word.copy_from_slice(&v.to_be_bytes::<32>());
    Some(())
}

/// Morpho Blue — compute a market id from the decoded `MarketParams` tuple.
///
/// `MarketParamsLib.id` (morpho-blue v1.0.0) is `keccak256(marketParams, 5*32)`
/// — a keccak over the five contiguous 32-byte words of `(address loanToken,
/// address collateralToken, address oracle, address irm, uint256 lltv)`. Because
/// the struct has no dynamic members this is byte-identical to
/// `keccak256(abi.encode(marketParams))`. The four addresses are left-padded;
/// `lltv` is the raw `uint256`.
///
/// Returns `0x` + `keccak256` hex, or `None` on any malformed field. This is the
/// single_emit analogue of [`maybe_inject_v4_pool_id`]: the declarative grammar
/// cannot hash, so a `LendingVenue::MorphoBlue.market_id` (a keccak-derived
/// string) can only be produced in Rust.
fn compute_morpho_market_id(market_params: &[serde_json::Value]) -> Option<String> {
    let mut buf = [0u8; 0xa0]; // 5 × 32 bytes.
    write_address_word(&mut buf[0x00..0x20], market_params[0].as_str()?)?;
    write_address_word(&mut buf[0x20..0x40], market_params[1].as_str()?)?;
    write_address_word(&mut buf[0x40..0x60], market_params[2].as_str()?)?;
    write_address_word(&mut buf[0x60..0x80], market_params[3].as_str()?)?;
    write_u256_word(&mut buf[0x80..0xa0], &market_params[4])?;
    Some(format!(
        "0x{}",
        hex::encode(alloy_primitives::keccak256(buf))
    ))
}

/// If the decoded top-level args carry a Morpho `marketParams` 5-tuple, inject
/// `$derived.morpho_market_id` so a single_emit manifest can fill a
/// `LendingVenue::MorphoBlue.market_id`. Shape-gated (a 5-element `marketParams`
/// array) — a no-op for every other call, and harmless even on the (unlikely)
/// non-Morpho contract carrying a same-named 5-tuple, since only Morpho
/// manifests reference the placeholder. See [`compute_morpho_market_id`].
fn maybe_inject_morpho_market_id(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    let Some(mp) = args_json
        .get("marketParams")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    if mp.len() != 5 {
        return;
    }
    if let Some(id) = compute_morpho_market_id(mp) {
        derived.insert("morpho_market_id".to_owned(), serde_json::Value::String(id));
    }
}

/// If the decoded top-level args carry a MetaMorpho `vault` argument (a
/// GeneralAdapter1 `erc4626*` leg — `erc4626Deposit/Mint/Withdraw/Redeem`),
/// inject `$derived.metamorpho_underlying`: the vault's underlying asset, which
/// is NOT in the leg calldata (the leg carries only the vault address). Sourced
/// from the committed mainnet vault→underlying snapshot
/// ([`crate::metamorpho_underlying`]). A no-op for every non-vault call
/// (value-gated on a known listed vault). The single_emit analogue of the
/// per-vault baked `asset` used by the DIRECT metamorpho manifests.
fn maybe_inject_metamorpho_underlying(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    let Some(vault) = args_json.get("vault").and_then(serde_json::Value::as_str) else {
        return;
    };
    if let Some(underlying) = crate::metamorpho_underlying::underlying_of(&vault.to_lowercase()) {
        derived.insert(
            "metamorpho_underlying".to_owned(),
            serde_json::Value::String(underlying.to_owned()),
        );
    }
}

/// Uniswap V3 SwapRouter `exactInput` / `exactOutput` carry a packed `bytes path`
/// in top-level args. The declarative grammar can reference derived placeholders
/// but cannot unpack `[token20][fee3]...`; inject the stable endpoints and first
/// hop fee so direct SwapRouter02 manifests can stay declarative.
fn maybe_inject_uniswap_v3_path(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    let Some(path) = args_json.get("path").and_then(serde_json::Value::as_str) else {
        return;
    };
    let path = path.strip_prefix("0x").unwrap_or(path);
    let Ok(bytes) = hex::decode(path) else {
        return;
    };
    let Ok((tokens, fees)) = abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path(&bytes)
    else {
        return;
    };
    let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
        return;
    };
    derived.insert(
        "v3_path_first_token".to_owned(),
        serde_json::Value::String(format!("0x{}", hex::encode(first.0))),
    );
    derived.insert(
        "v3_path_last_token".to_owned(),
        serde_json::Value::String(format!("0x{}", hex::encode(last.0))),
    );
    if let Some(fee) = fees.first() {
        derived.insert(
            "fee_tier_bp".to_owned(),
            serde_json::Value::Number(serde_json::Number::from(*fee)),
        );
    }
}

/// Re-derive the shape-gated `$derived.*` endpoints for a NESTED child leg.
///
/// The route-level injection (the `maybe_inject_*` calls in
/// `declarative_route_request_v3_json`) only sees the OUTER call's decoded args.
/// A nested leg (an opcode-stream opcode, a tagged sub-action) carries its own
/// `path` / `marketParams` / `vault` in its OWN decoded inputs, so a child body
/// that references `$derived.v3_path_first_token` / `$derived.morpho_market_id` /
/// `$derived.metamorpho_underlying` would otherwise resolve against the parent's
/// (often empty) `derived` map — that was the Universal Router V3 swap leg
/// `token_in/out == 0x0` bug (the outer `execute(commands, inputs)` args have no
/// `path`, and the per-opcode child merely cloned the empty parent `derived`).
///
/// Mirrors the per-opcode `maybe_inject_v4_pool_id` precedent: clone the parent
/// `derived`, then re-run each injector against the child's inputs. Every
/// injector keys off a UNIQUE argument name and early-returns (no mutation) on a
/// shape mismatch, so calling all three is a safe no-op for any leg whose inputs
/// don't carry the matching key (the three trigger keys are disjoint, so they
/// cannot cross-fire). NOTE: `maybe_inject_uniswap_v3_path` gates on a `bytes`
/// arg literally named `path` — a V2 `address[] path` fails its `as_str` gate, so
/// `bytes path` is effectively reserved for V3 packed paths.
fn rederive_for_child(
    parent: &BTreeMap<String, serde_json::Value>,
    inputs: Option<&serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    let mut derived = parent.clone();
    if let Some(inputs) = inputs {
        maybe_inject_morpho_market_id(inputs, &mut derived);
        maybe_inject_metamorpho_underlying(inputs, &mut derived);
        maybe_inject_uniswap_v3_path(inputs, &mut derived);
    }
    derived
}

/// `multicall_recurse` (Cat D) — flatten a self-`multicall(bytes[])` into one
/// [`v3_action::ActionBody::Multicall`].
///
/// `self_array_bytes_last_arg`: the inner sub-calls live in a `bytes[]`
/// argument (SwapRouter02 `multicall(uint256 deadline, bytes[] data)` has a
/// leading non-array `deadline`; NFPM / V4 PositionManager `multicall(bytes[]
/// data)` has only the array). [`args_to_json`] renders that `bytes[]` as a JSON
/// array of `"0x.."` strings, so the common case picks the sole array-valued
/// arg. Wrappers with sibling array args can set `emit.recurse_arg` to the
/// decoded argument name that holds child calldata.
///
/// Each inner leg targets the SAME `to`. We resolve + decode + build it by
/// RE-ENTERING [`declarative_route_request_v3_json`] (the public entrypoint), so
/// every inner strategy is handled transparently — single_emit, opcode_stream
/// dispatch (e.g. an inner V4 `modifyLiquidities`), and even a nested
/// `multicall`. Inner legs with no installed mapper (helper calls Uniswap
/// routinely bundles — `refundETH` / `sweepToken` / `unwrapWETH9`) are SKIPPED
/// rather than failing the batch; but if NO leg resolves we reject so the policy
/// engine never receives a misleading empty no-op for calldata we could not map.
///
/// Recursion is bounded: every inner element is a strict sub-slice of the outer
/// calldata, so a `multicall`-of-`multicall` chain shrinks each level; the
/// per-level fan-out is capped at [`MAX_MULTICALL_CHILDREN`].
fn build_multicall_recurse_body(
    chain_id: u64,
    to: &str,
    submitter: &str,
    submitted_at: u64,
    args_json: &serde_json::Value,
    emit: &serde_json::Value,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    const MAX_MULTICALL_CHILDREN: usize = 64;

    let recurse_rule_id = emit
        .get("recurse_rule_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if recurse_rule_id != "self_array_bytes_last_arg" {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            format!(
                "unsupported multicall recurse_rule_id {recurse_rule_id:?} \
                 (only self_array_bytes_last_arg)"
            ),
        ));
    }

    let inner_calls_value =
        if let Some(recurse_arg) = emit.get("recurse_arg").and_then(serde_json::Value::as_str) {
            args_json.get(recurse_arg).ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall_recurse: recurse_arg {recurse_arg:?} not found"),
                )
            })?
        } else {
            // self_array_bytes_last_arg → exactly one array-valued argument (the
            // `bytes[]`). A `uint256 deadline` sibling renders as a decimal string,
            // never an array, so the array arg is unambiguous for every existing
            // simple multicall shape.
            let Some(obj) = args_json.as_object() else {
                return Err(EngineErrorDto::new(
                    "build_multicall_failed",
                    "multicall_recurse: no bytes[] array argument".to_string(),
                ));
            };
            let mut array_args = obj.values().filter(|v| v.is_array());
            match (array_args.next(), array_args.next()) {
                (Some(single), None) => single,
                (None, _) => {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        "multicall_recurse: no bytes[] array argument".to_string(),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        "multicall_recurse: ambiguous (multiple array arguments)".to_string(),
                    ));
                }
            }
        };
    let inner_calls = inner_calls_value.as_array().ok_or_else(|| {
        EngineErrorDto::new(
            "build_multicall_failed",
            "multicall_recurse: selected argument is not a bytes[] array".to_string(),
        )
    })?;

    if inner_calls.len() > MAX_MULTICALL_CHILDREN {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            format!(
                "multicall child count {} exceeds cap {MAX_MULTICALL_CHILDREN}",
                inner_calls.len()
            ),
        ));
    }

    let mut actions: Vec<v3_action::ActionBody> = Vec::new();
    let mut resolved = 0usize;
    for (index, item) in inner_calls.iter().enumerate() {
        let inner_hex = item.as_str().ok_or_else(|| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall child #{index} is not a hex string"),
            )
        })?;
        let stripped = inner_hex.strip_prefix("0x").unwrap_or(inner_hex);
        let inner_bytes = hex::decode(stripped).map_err(|error| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall child #{index} not hex: {error}"),
            )
        })?;
        if inner_bytes.len() < 4 {
            return Err(EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall child #{index} calldata < 4 bytes"),
            ));
        }
        let inner_selector = format!("0x{}", hex::encode(&inner_bytes[0..4]));

        // Re-enter the public entrypoint for this leg. Inner calls carry no
        // independent msg.value (they execute under the outer call's context),
        // so value is "0".
        let inner_input = serde_json::json!({
            "chain_id": chain_id,
            "to": to,
            "selector": inner_selector,
            "calldata": inner_hex,
            "value": "0",
            "submitter": submitter,
            "submitted_at": submitted_at,
        });
        let out = declarative_route_request_v3_json(inner_input.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).map_err(|error| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall child #{index} result not JSON: {error}"),
            )
        })?;

        if parsed.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            let kind = parsed
                .pointer("/error/kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            // Unmapped helper leg (refundETH / sweepToken / unwrapWETH9 / a
            // permit variant we don't model) — skip; the mapped legs carry the
            // intent. Any OTHER error (a mapped leg that failed to decode) is
            // surfaced so the batch fails loud rather than silently dropping it.
            if kind == "no_declarative_v3_mapper" {
                continue;
            }
            let message = parsed
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            return Err(EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall child #{index} ({inner_selector}): {kind}: {message}"),
            ));
        }
        resolved += 1;

        let inner_actions = parsed
            .pointer("/data/actions")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall child #{index} result missing data.actions"),
                )
            })?;
        for action in inner_actions {
            let body_json = action.get("body").ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall child #{index} action missing body"),
                )
            })?;
            let body: v3_action::ActionBody =
                serde_json::from_value(body_json.clone()).map_err(|error| {
                    EngineErrorDto::new(
                        "build_multicall_failed",
                        format!("multicall child #{index} body deserialize: {error}"),
                    )
                })?;
            actions.push(body);
        }
    }

    if resolved == 0 {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            "multicall_recurse: no inner leg resolved to an installed mapper".to_string(),
        ));
    }

    Ok(v3_action::ActionBody::Multicall { actions })
}

/// Cat D' — `multicall_call_array`: Bundler3-style `multicall(Call[])` where each
/// `Call = (address to, bytes data, uint256 value, bool skipRevert, bytes32 callbackHash)`
/// carries its OWN target. Unlike [`build_multicall_recurse_body`] (same-`to`,
/// `bytes[]` legs), we read each leg's `to` from the decoded positional tuple and
/// re-route the leg's `data` THERE (e.g. Bundler3 → GeneralAdapter1), wrapping the
/// mapped legs in one [`ActionBody::Multicall`]. Unmapped helper legs (no installed
/// mapper) are skipped; if NO leg resolves we reject (never an empty no-op).
///
/// Recursion is bounded: each leg's `data` is a strict sub-slice of the outer
/// calldata (shrinks every level) and the per-level fan-out is capped at
/// `MAX_MULTICALL_CHILDREN`.
fn build_multicall_call_array_body(
    chain_id: u64,
    submitter: &str,
    submitted_at: u64,
    args_json: &serde_json::Value,
    emit: &serde_json::Value,
) -> Result<v3_action::ActionBody, EngineErrorDto> {
    const MAX_MULTICALL_CHILDREN: usize = 64;

    // Select the `Call[]` argument: an explicit `recurse_arg` name, else the single
    // array-valued argument (the bundle is the only array for `multicall(Call[])`).
    let bundle_value =
        if let Some(recurse_arg) = emit.get("recurse_arg").and_then(serde_json::Value::as_str) {
            args_json.get(recurse_arg).ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall_call_array: recurse_arg {recurse_arg:?} not found"),
                )
            })?
        } else {
            let Some(obj) = args_json.as_object() else {
                return Err(EngineErrorDto::new(
                    "build_multicall_failed",
                    "multicall_call_array: no Call[] array argument".to_string(),
                ));
            };
            let mut array_args = obj.values().filter(|v| v.is_array());
            match (array_args.next(), array_args.next()) {
                (Some(single), None) => single,
                (None, _) => {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        "multicall_call_array: no Call[] array argument".to_string(),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        "multicall_call_array: ambiguous (multiple array arguments)".to_string(),
                    ));
                }
            }
        };
    let legs = bundle_value.as_array().ok_or_else(|| {
        EngineErrorDto::new(
            "build_multicall_failed",
            "multicall_call_array: selected argument is not a Call[] array".to_string(),
        )
    })?;
    if legs.len() > MAX_MULTICALL_CHILDREN {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            format!(
                "multicall_call_array child count {} exceeds cap {MAX_MULTICALL_CHILDREN}",
                legs.len()
            ),
        ));
    }

    // `max_depth` bounds only the `reenter(Call[])` callback recursion (D-C).
    let max_depth = emit
        .get("max_depth")
        .and_then(serde_json::Value::as_u64)
        .map_or(4, |d| usize::try_from(d).unwrap_or(4));

    let actions = process_call_legs(chain_id, submitter, submitted_at, legs, 0, max_depth)?;

    if actions.is_empty() {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            "multicall_call_array: no inner leg resolved to an installed mapper".to_string(),
        ));
    }

    Ok(v3_action::ActionBody::Multicall { actions })
}

/// Process a decoded `Call[]` (positional `[to,data,value,skipRevert,callbackHash]`
/// tuples), re-routing each leg AT ITS OWN `to` and folding the mapped legs into a
/// flat `Vec<ActionBody>`. Shared by the top-level Bundler3 `multicall(Call[])`
/// decode and the nested `reenter(Call[])` callback recursion (D-C).
///
/// LENIENT: returns whatever resolved (possibly EMPTY) so a `reenter` callback
/// whose legs are all unmapped (e.g. a swap via a deferred adapter) does not fail
/// the outer bundle — only the TOP-LEVEL caller rejects an all-empty decode.
/// `depth`/`max_depth` bound the callback recursion (each callback `data` is a
/// strict sub-slice; the per-level fan-out is capped at `MAX_MULTICALL_CHILDREN`).
fn process_call_legs(
    chain_id: u64,
    submitter: &str,
    submitted_at: u64,
    legs: &[serde_json::Value],
    depth: usize,
    max_depth: usize,
) -> Result<Vec<v3_action::ActionBody>, EngineErrorDto> {
    const MAX_MULTICALL_CHILDREN: usize = 64;
    if legs.len() > MAX_MULTICALL_CHILDREN {
        return Err(EngineErrorDto::new(
            "build_multicall_failed",
            format!(
                "multicall_call_array child count {} exceeds cap {MAX_MULTICALL_CHILDREN}",
                legs.len()
            ),
        ));
    }

    let mut actions: Vec<v3_action::ActionBody> = Vec::new();
    for (index, leg) in legs.iter().enumerate() {
        // Each leg is a positional tuple array: [to, data, value, skipRevert, callbackHash].
        let fields = leg.as_array().ok_or_else(|| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall_call_array leg #{index} is not a Call tuple array"),
            )
        })?;
        let leg_to = fields
            .first()
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall_call_array leg #{index} missing tuple field 0 (to)"),
                )
            })?;
        let leg_data = fields
            .get(1)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new(
                    "build_multicall_failed",
                    format!("multicall_call_array leg #{index} missing tuple field 1 (data)"),
                )
            })?;
        // Call.value (tuple field 2, uint256 → decimal string) — forward it so a
        // native-value leg (e.g. wrapNative) sees the right msg.value. Default "0".
        let leg_value = fields
            .get(2)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0");

        let stripped = leg_data.strip_prefix("0x").unwrap_or(leg_data);
        let data_bytes = hex::decode(stripped).map_err(|error| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall_call_array leg #{index} data not hex: {error}"),
            )
        })?;
        if data_bytes.len() < 4 {
            // A bare value-transfer leg (empty / `<4B` data) has no selector to
            // route — skip it; the mapped legs carry the intent.
            continue;
        }
        let leg_selector = format!("0x{}", hex::encode(&data_bytes[0..4]));

        // Re-enter the public entrypoint AT THE LEG'S OWN `to` (the per-leg-to
        // difference vs `multicall_recurse`).
        let inner_input = serde_json::json!({
            "chain_id": chain_id,
            "to": leg_to,
            "selector": leg_selector,
            "calldata": leg_data,
            "value": leg_value,
            "submitter": submitter,
            "submitted_at": submitted_at,
        });
        let out = declarative_route_request_v3_json(inner_input.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).map_err(|error| {
            EngineErrorDto::new(
                "build_multicall_failed",
                format!("multicall_call_array leg #{index} result not JSON: {error}"),
            )
        })?;

        if parsed.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            let inner_actions = parsed
                .pointer("/data/actions")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    EngineErrorDto::new(
                        "build_multicall_failed",
                        format!("multicall_call_array leg #{index} result missing data.actions"),
                    )
                })?;
            for action in inner_actions {
                let body_json = action.get("body").ok_or_else(|| {
                    EngineErrorDto::new(
                        "build_multicall_failed",
                        format!("multicall_call_array leg #{index} action missing body"),
                    )
                })?;
                let body: v3_action::ActionBody = serde_json::from_value(body_json.clone())
                    .map_err(|error| {
                        EngineErrorDto::new(
                            "build_multicall_failed",
                            format!("multicall_call_array leg #{index} body deserialize: {error}"),
                        )
                    })?;
                // D-A: a GeneralAdapter1 `erc4626*` leg whose MetaMorpho vault is
                // OUTSIDE the committed `metamorpho_underlying` snapshot could not
                // resolve its underlying, so the required `asset` fell back to the
                // zero address — a confidently-WRONG decode. Refuse the WHOLE bundle
                // (fail loud → warn-closed) rather than silently skip the leg: a
                // malicious batch could otherwise hide a large unknown-vault deposit
                // behind a benign known-vault one. (Re-gen the snapshot when the
                // listed set changes; see `crate::metamorpho_underlying`.)
                if is_unresolved_metamorpho_underlying(&body) {
                    return Err(EngineErrorDto::new(
                        "build_multicall_failed",
                        format!(
                            "multicall_call_array leg #{index}: MetaMorpho vault underlying \
                             unresolved (vault outside the committed snapshot) — refusing a \
                             0x0-asset decode"
                        ),
                    ));
                }
                actions.push(body);
            }
        } else {
            let kind = parsed
                .pointer("/error/kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            // Unmapped helper / deferred-adapter leg (no installed mapper, or no
            // adapter registered for that `to`) — no primary body, but DON'T bail:
            // a Morpho callback leg (notably the EXCLUDE `morphoFlashLoan`) still
            // carries a `reenter(Call[])` we want to surface, so fall through to the
            // callback extraction below. Any OTHER error (a mapped leg that failed
            // to decode) is surfaced so the batch fails loud, not silently dropped.
            if kind != "no_declarative_v3_mapper" && kind != "route_failed" {
                let message = parsed
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                return Err(EngineErrorDto::new(
                    "build_multicall_failed",
                    format!(
                        "multicall_call_array leg #{index} ({leg_selector}): {kind}: {message}"
                    ),
                ));
            }
        }

        // D-C (generalized): the route surfaces `data.reenter_callback` when the
        // leg's manifest declares `emit.reenter_callback_arg` — the raw
        // `abi.encode(Call[])` re-entry callback (Bundler3 re-enters it at the
        // just-called adapter mid-execution: leverage / flash-loan loops). The
        // primary body (if any) is already pushed; ALSO decode the callback legs so
        // they aren't opaque. Bounded by `max_depth`; an all-unmapped callback
        // contributes nothing. NO per-protocol selector list — fully manifest-driven.
        if depth < max_depth {
            if let Some(callback_hex) = parsed
                .pointer("/data/reenter_callback")
                .and_then(serde_json::Value::as_str)
            {
                if let Some(callback_legs) = decode_reenter_call_array(callback_hex)? {
                    let nested = process_call_legs(
                        chain_id,
                        submitter,
                        submitted_at,
                        &callback_legs,
                        depth + 1,
                        max_depth,
                    )?;
                    actions.extend(nested);
                }
            }
        }
    }

    Ok(actions)
}

/// Decode a `reenter(Call[])` callback — the raw `abi.encode(Call[])` bytes a
/// manifest's `emit.reenter_callback_arg` points at (surfaced by the route as
/// `data.reenter_callback`) — into its positional `Call[]` legs. `Ok(None)` for an
/// empty callback. PROTOCOL-AGNOSTIC: the `Call = (address,bytes,uint256,bool,
/// bytes32)` tuple is the framework's standard re-entry shape, so this carries no
/// per-protocol knowledge — any bundler-adapter that nests a `Call[]` in a leg arg
/// reuses it by declaring the arg name in its manifest. (The callback `data` has
/// NO `reenter` selector prefix — Bundler3's `CoreAdapter.reenterBundler3` does
/// `bytes.concat(reenter.selector, data)` itself.)
fn decode_reenter_call_array(
    callback_hex: &str,
) -> Result<Option<Vec<serde_json::Value>>, EngineErrorDto> {
    let stripped = callback_hex.strip_prefix("0x").unwrap_or(callback_hex);
    if stripped.is_empty() {
        return Ok(None);
    }
    let data_bytes = hex::decode(stripped).map_err(|error| {
        EngineErrorDto::new(
            "build_multicall_failed",
            format!("reenter callback: data not hex: {error}"),
        )
    })?;
    let bundle = decode_inputs_abi_tuple(
        "((address,bytes,uint256,bool,bytes32)[] bundle)",
        &data_bytes,
    )
    .map_err(|error| {
        EngineErrorDto::new(
            "build_multicall_failed",
            format!("reenter callback: Call[] decode failed: {error}"),
        )
    })?;
    let legs = bundle
        .get("bundle")
        .and_then(serde_json::Value::as_array)
        .filter(|legs| !legs.is_empty());
    Ok(legs.cloned())
}

/// D-A guard for [`build_multicall_call_array_body`]: is `body` a GeneralAdapter1
/// `erc4626*` leg (MetaMorpho `supply`/`withdraw`) whose required `asset` resolved
/// to the ZERO address? That happens only when the leg's vault is outside the
/// committed `metamorpho_underlying` snapshot, so `maybe_inject_metamorpho_underlying`
/// could not fill the underlying and placeholder substitution fell back to `0x0`.
/// A real MetaMorpho underlying is never the zero address, so `0x0` is an
/// unambiguous "unresolved" sentinel. (morpho* legs are unaffected — their asset
/// derives from `MarketParams`, which is always present in the leg calldata.)
fn is_unresolved_metamorpho_underlying(body: &v3_action::ActionBody) -> bool {
    use v3_action::lending::{LendingAction, LendingVenue};
    let v3_action::ActionBody::Lending(action) = body else {
        return false;
    };
    let (venue, asset) = match action {
        LendingAction::Supply(a) => (&a.venue, &a.asset),
        LendingAction::Withdraw(a) => (&a.venue, &a.asset),
        _ => return false,
    };
    matches!(venue, LendingVenue::MetaMorpho { .. })
        && asset
            .key
            .contract()
            .is_some_and(|addr| *addr == alloy_primitives::Address::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::{Address as AlloyAddress, U256};
    use serde_json::{json, Value};

    // ──────────────────────────────────────────────────────────────────────
    // Phase 4B — declarative_route_request_v3_json
    // ──────────────────────────────────────────────────────────────────────

    fn v3_route_input() -> Value {
        json!({
            "chain_id":    1,
            "to":          "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
            "selector":    "0x38ed1739",
            "calldata":    "0x38ed1739dead",
            "value":       "0",
            "gas_limit":   "200000",
            "gas_price":   "20000000000",
            "submitter":   "0x000000000000000000000000000000000000aaaa",
            "submitted_at": 1_700_000_000_u64,
            "nonce": 42_u64,
            "block_timestamp": 1_700_000_010_u64
        })
    }

    #[test]
    fn route_request_v3_misses_without_v3_install() {
        // M2 contract: a callkey with no v3 manifest installed surfaces
        // `no_declarative_v3_mapper` so the SW caller can surface the gap.
        let out = declarative_route_request_v3_json(v3_route_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(
            parsed["error"]["kind"], "no_declarative_v3_mapper",
            "{parsed}"
        );
    }

    #[test]
    fn route_request_v3_rejects_invalid_json() {
        let out = declarative_route_request_v3_json("{not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
    }

    #[test]
    fn route_request_v3_rejects_invalid_address() {
        let mut input = v3_route_input();
        input["submitter"] = json!("not-an-address");
        let out = declarative_route_request_v3_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
        let message = parsed["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("submitter"),
            "expected submitter diagnostic, got: {message}"
        );
    }

    #[test]
    fn route_request_v3_serde_defaults_round_trip_through_miss() {
        // Pin the serde defaults for `value` / `gas_limit` / `gas_price` /
        // `nonce` — they're still part of the wire contract even though the
        // miss path never builds the meta. We assert via the error envelope:
        // the early-parse stage succeeds (no `invalid_input_json` kind) and
        // the bridge lookup is what fails.
        let input = json!({
            "chain_id":    8453,
            "to":          "0x0000000000000000000000000000000000001234",
            "selector":    "0x12345678",
            "calldata":    "0x12345678",
            "submitter":   "0x000000000000000000000000000000000000aaaa",
            "submitted_at": 1_700_000_000_u64
        });
        let out = declarative_route_request_v3_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        // Bridge miss — the defaults parsed successfully (no
        // `invalid_input_json` from the U256 / address parsers above).
        assert_eq!(
            parsed["error"]["kind"], "no_declarative_v3_mapper",
            "{parsed}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // N2/N3/K3 — a batch leg the decoder cannot map must surface as `Unknown`
    // (warn-closed downstream), NOT be silently dropped (which let a benign
    // sibling, or an empty batch, aggregate to PASS).
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn opcode_stream_all_unknown_decodes_to_unknown_not_empty_multicall() {
        // An opcode stream whose commands are ALL unmapped used to drop every leg
        // and return an EMPTY Multicall — which aggregates to PASS (N2 fail-open).
        // It must now surface a single `Unknown` body so it warn-closes. (A single
        // unmapped opcode in an otherwise-decoded stream is still dropped — the
        // registry's `warn`=skip intent; only an all-empty result is escalated.)
        let args = json!({});
        let ctx = V3MapContext {
            chain: V3ChainId::new("eip155:1".to_string()),
            tx_to: parse_v3_address("0x000000000000000000000000000000000000babe", "to").unwrap(),
            tx_from: parse_v3_address("0x000000000000000000000000000000000000aaaa", "from")
                .unwrap(),
            value: V3U256::ZERO,
            submitted_at: V3Time::from_unix(1_700_000_000),
            args_json: &args,
            raw_calldata: "0xabcdef",
            resolved: std::collections::BTreeMap::new(),
            derived: std::collections::BTreeMap::new(),
            inputs: None,
        };
        let per_opcode_body = serde_json::Map::new(); // nothing mapped → 0x01 unknown
        let commands = [0x01u8];
        let body = dispatch_opcode_stream(
            &ctx,
            &per_opcode_body,
            &commands,
            &[],
            0xff,
            0x00,
            V3UnknownOpcodePolicy::Warn,
            0,
            5,
        )
        .unwrap();
        let body_json = serde_json::to_value(body).unwrap();
        assert_eq!(
            body_json["domain"], "unknown",
            "all-unknown opcode stream must surface Unknown (not an empty Multicall \
             that aggregates to PASS): {body_json}"
        );
    }

    #[test]
    fn array_emit_empty_array_routes_to_unknown_not_empty_multicall() {
        // N2 catch-all: an `array_emit` over a length-0 dynamic array (Permit2
        // lockdown([]) / permitBatch([]) etc.) builds an empty Multicall, which
        // would aggregate to PASS. The route-level guard must surface it as Unknown
        // so it warn-closes. (An empty batch is an on-chain no-op — warn-closing
        // costs the user nothing; it just denies the silent PASS.)
        let contract = "0x000000000000000000000000000000000000c0de";
        let batch_fn = Function::parse("batch(uint256[] xs)").unwrap();
        let calldata = batch_fn
            .abi_encode_input(&[DynSolValue::Array(vec![])])
            .unwrap();
        let selector = format!("0x{}", hex::encode(&calldata[0..4]));
        let calldata_hex = format!("0x{}", hex::encode(&calldata));

        let bundle = json!({
            "type": "adapter_action", "id": "test/array-emit-empty@1.0.0",
            "publisher": "test", "schema_version": "3",
            "match": { "selector": selector, "chain_to_addresses": { "1": [contract] } },
            "abi_fragment": { "function_name": "batch", "abi": {
                "name": "batch", "type": "function", "stateMutability": "nonpayable",
                "inputs": [{ "name": "xs", "type": "uint256[]" }], "outputs": [] } },
            "emit": { "strategy": "array_emit", "array_source": "$args.xs", "body": {
                "domain": "token", "token": { "action": "erc20_transfer", "erc20_transfer": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$to" } },
                    "recipient": "$submitter", "amount": "$inputs" } } } },
            "requires": { "imperative": [], "adapter_capabilities": [], "host_capabilities": [], "extension": ">=0.1.0" }
        });
        let installed: Value =
            serde_json::from_str(&declarative_install_v3_json(bundle.to_string())).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let out = declarative_route_request_v3_json(
            json!({
                "chain_id": 1, "to": contract, "selector": selector, "calldata": calldata_hex,
                "submitter": "0x000000000000000000000000000000000000aaaa", "submitted_at": 1_700_000_000_u64
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["actions"][0]["body"]["domain"], "unknown",
            "empty array_emit must route to Unknown, not an empty Multicall (PASS): {parsed}"
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // Address-agnostic (selector-only) route tier — standard NFT
    // `setApprovalForAll`. Decodes on ANY collection, registered or not, so the
    // #1 NFT-drain vector is no longer warn-closed for unlisted collections.
    // ──────────────────────────────────────────────────────────────────────

    fn set_approval_for_all_agnostic_bundle() -> Value {
        json!({
            "type": "adapter_action",
            "id": "standard/nft/set-approval-for-all@1.0.0",
            "schema_version": "3",
            "match": { "selector": "0xa22cb465", "address_agnostic": true, "chain_ids": [1] },
            "abi_fragment": { "function_name": "setApprovalForAll", "abi": {
                "name": "setApprovalForAll", "type": "function", "stateMutability": "nonpayable",
                "inputs": [
                    { "name": "operator", "type": "address" },
                    { "name": "approved", "type": "bool" }
                ],
                "outputs": [] } },
            "emit": { "strategy": "single_emit", "body": {
                "domain": "token", "token": {
                    "action": "nft_set_approval_for_all",
                    "nft_set_approval_for_all": {
                        "chain": "$chain", "contract": "$to",
                        "spender": "$args.operator", "approved": "$args.approved" } } },
                "live_inputs": {} },
            "requires": { "imperative": [], "adapter_capabilities": [], "host_capabilities": [], "extension": ">=0.1.0" }
        })
    }

    #[test]
    fn address_agnostic_set_approval_for_all_decodes_on_unregistered_collection() {
        let installed: Value = serde_json::from_str(&declarative_install_v3_json(
            set_approval_for_all_agnostic_bundle().to_string(),
        ))
        .unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        // A collection address NEVER registered as a per-address callkey.
        let collection = "0x000000000000000000000000000000000000beef";
        let operator = "0x000000000000000000000000000000000000cafe";
        // setApprovalForAll(operator, approved=true)
        let calldata = format!(
            "0xa22cb465{:0>64}{:0>64}",
            operator.trim_start_matches("0x"),
            "1"
        );

        let out = declarative_route_request_v3_json(
            json!({
                "chain_id": 1, "to": collection, "selector": "0xa22cb465", "calldata": calldata,
                "submitter": "0x000000000000000000000000000000000000aaaa", "submitted_at": 1_700_000_000_u64
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed["ok"], true,
            "address-agnostic decode must hit on an unregistered collection: {parsed}"
        );

        // ActionBody serializes FLAT (internally tagged on domain + action):
        // {domain:"token", action:"nft_set_approval_for_all", chain, contract, spender, approved}.
        let body = &parsed["data"]["actions"][0]["body"];
        assert_eq!(body["domain"], "token", "{body}");
        assert_eq!(body["action"], "nft_set_approval_for_all", "{body}");
        assert_eq!(
            body["contract"].as_str().unwrap().to_lowercase(),
            collection,
            "contract must be the live tx `to` (the collection) — proves address-agnostic routing: {body}"
        );
        assert_eq!(
            body["spender"].as_str().unwrap().to_lowercase(),
            operator,
            "{body}"
        );
        assert_eq!(body["approved"], true, "{body}");
    }

    #[test]
    fn address_agnostic_bridge_is_chain_scoped_not_a_catch_all() {
        // The selector-only fallback stays keyed by `(chain_id, selector)`: the
        // same selector on a chain the bundle did NOT declare still misses
        // (warn-closed), proving the tier is not a blanket selector wildcard.
        let installed: Value = serde_json::from_str(&declarative_install_v3_json(
            set_approval_for_all_agnostic_bundle().to_string(),
        ))
        .unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let calldata = format!(
            "0xa22cb465{:0>64}{:0>64}",
            "000000000000000000000000000000000000cafe", "1"
        );
        let out = declarative_route_request_v3_json(
            json!({
                // chain 999 is NOT in the bundle's `chain_ids: [1]`.
                "chain_id": 999, "to": "0x000000000000000000000000000000000000beef",
                "selector": "0xa22cb465", "calldata": calldata,
                "submitter": "0x000000000000000000000000000000000000aaaa", "submitted_at": 1_700_000_000_u64
            })
            .to_string(),
        );
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(
            parsed["error"]["kind"], "no_declarative_v3_mapper",
            "{parsed}"
        );
    }

    #[test]
    fn multicall_recurse_uses_explicit_recurse_arg_when_sibling_arrays_exist() {
        let router = "0x000000000000000000000000000000000000babe";
        let submitter = "0x000000000000000000000000000000000000aaaa";
        let child_fn = Function::parse("foo(uint256)").unwrap();
        let child_calldata = child_fn
            .abi_encode_input(&[DynSolValue::Uint(U256::from(123_u64), 256)])
            .unwrap();
        let child_selector = format!("0x{}", hex::encode(&child_calldata[0..4]));
        let child_hex = format!("0x{}", hex::encode(&child_calldata));

        let child_bundle = json!({
            "type": "adapter_action",
            "id": "test/multicall-recurse-child@1.0.0",
            "publisher": "test",
            "schema_version": "3",
            "match": {
                "selector": child_selector,
                "chain_to_addresses": { "1": [router] }
            },
            "abi_fragment": {
                "function_name": "foo",
                "abi": {
                    "name": "foo",
                    "type": "function",
                    "stateMutability": "nonpayable",
                    "inputs": [{ "name": "amount", "type": "uint256" }],
                    "outputs": []
                }
            },
            "emit": {
                "strategy": "single_emit",
                "body": {
                    "domain": "unknown",
                    "unknown": {
                        "target": "$to",
                        "chain": "$chain",
                        "calldata": "$calldata",
                        "value": "$tx.value"
                    }
                }
            },
            "requires": {
                "imperative": [],
                "adapter_capabilities": [],
                "host_capabilities": [],
                "extension": ">=0.1.0"
            }
        });
        let installed: Value =
            serde_json::from_str(&declarative_install_v3_json(child_bundle.to_string())).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let args = json!({
            "permitBatch": [],
            "permitSignatures": [],
            "permit2Batch": {
                "details": [],
                "spender": "0x0000000000000000000000000000000000000000",
                "sigDeadline": "0"
            },
            "permit2Signature": "0x",
            "multicallData": [child_hex]
        });

        let ambiguous = build_multicall_recurse_body(
            1,
            router,
            submitter,
            1_700_000_000,
            &args,
            &json!({
                "strategy": "multicall_recurse",
                "recurse_rule_id": "self_array_bytes_last_arg",
                "max_depth": 3
            }),
        )
        .unwrap_err();
        assert!(
            ambiguous.message.contains("ambiguous"),
            "expected ambiguous sibling-array error, got {ambiguous:?}"
        );

        let body = build_multicall_recurse_body(
            1,
            router,
            submitter,
            1_700_000_000,
            &args,
            &json!({
                "strategy": "multicall_recurse",
                "recurse_rule_id": "self_array_bytes_last_arg",
                "recurse_arg": "multicallData",
                "max_depth": 3
            }),
        )
        .unwrap();
        let body_json = serde_json::to_value(body).unwrap();
        assert_eq!(body_json["domain"], "multicall", "{body_json}");
        assert_eq!(body_json["actions"].as_array().unwrap().len(), 1);
        assert_eq!(body_json["actions"][0]["domain"], "unknown", "{body_json}");
    }

    /// `parallel_tagged_dispatch` end-to-end: a Compound v3
    /// `Bulker.invoke(bytes32[] actions, bytes[] data)` with two index-aligned
    /// legs (`ACTION_SUPPLY_ASSET` + `ACTION_CLAIM_REWARD`) decodes — through the
    /// real install + route path — to a `Multicall` of one `lending::supply` and
    /// one `airdrop::claim`. Crucially asserts the per-element `resolve_from_inputs`
    /// mechanism: `base_asset` is resolved from the DECODED `comet` (cUSDCv3 →
    /// USDC), NOT from `$to` (the Bulker) — the wrinkle that makes the strategy
    /// necessary.
    #[test]
    fn parallel_tagged_dispatch_bulker_invoke_fans_out_to_multicall() {
        use alloy_primitives::B256;

        let bulker = "0xa397a8c2086c554b531c02e29f3291c9704b00c7";
        let comet = "0xc3d688b66703497daa19211eedff47f25384cdc3"; // cUSDCv3 → base USDC
        let usdc = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
        let user = "0x000000000000000000000000000000000000aaaa";
        let asset = "0x1111111111111111111111111111111111111111";
        let rewards = "0x1b0e765f6224c21223aea2af16c1c46e38885a40"; // CometRewards mainnet
        let src = "0x000000000000000000000000000000000000bbbb";

        // bytes32 ASCII tags (right-zero-padded action names).
        let tag = |name: &str| -> B256 {
            let mut b = [0u8; 32];
            b[..name.len()].copy_from_slice(name.as_bytes());
            B256::from(b)
        };

        // data[i] = abi.encode(...) — params encoding (selector-stripped).
        let params_enc = |sig: &str, vals: &[DynSolValue]| -> Vec<u8> {
            let f = Function::parse(sig).unwrap();
            f.abi_encode_input(vals).unwrap()[4..].to_vec()
        };
        let supply_data = params_enc(
            "x(address,address,address,uint256)",
            &[
                DynSolValue::Address(comet.parse().unwrap()),
                DynSolValue::Address(user.parse().unwrap()),
                DynSolValue::Address(asset.parse().unwrap()),
                DynSolValue::Uint(U256::from(5000_u64), 256),
            ],
        );
        let claim_data = params_enc(
            "x(address,address,address,bool)",
            &[
                DynSolValue::Address(comet.parse().unwrap()),
                DynSolValue::Address(rewards.parse().unwrap()),
                DynSolValue::Address(src.parse().unwrap()),
                DynSolValue::Bool(true),
            ],
        );

        // outer invoke(bytes32[],bytes[]) calldata.
        let invoke_fn = Function::parse("invoke(bytes32[],bytes[])").unwrap();
        let invoke_cd = invoke_fn
            .abi_encode_input(&[
                DynSolValue::Array(vec![
                    DynSolValue::FixedBytes(tag("ACTION_SUPPLY_ASSET"), 32),
                    DynSolValue::FixedBytes(tag("ACTION_CLAIM_REWARD"), 32),
                ]),
                DynSolValue::Array(vec![
                    DynSolValue::Bytes(supply_data),
                    DynSolValue::Bytes(claim_data),
                ]),
            ])
            .unwrap();
        let invoke_selector = format!("0x{}", hex::encode(&invoke_cd[0..4]));
        let invoke_hex = format!("0x{}", hex::encode(&invoke_cd));

        let bundle = json!({
            "type": "adapter_action",
            "id": "test/bulker-invoke@1.0.0",
            "publisher": "test",
            "schema_version": "3",
            "match": {
                "selector": invoke_selector,
                "chain_to_addresses": { "1": [bulker] }
            },
            "abi_fragment": {
                "function_name": "invoke",
                "abi": {
                    "name": "invoke",
                    "type": "function",
                    "stateMutability": "payable",
                    "inputs": [
                        { "name": "actions", "type": "bytes32[]" },
                        { "name": "data", "type": "bytes[]" }
                    ],
                    "outputs": []
                }
            },
            "emit": {
                "strategy": "parallel_tagged_dispatch",
                "actions_source": "$args.actions",
                "data_source": "$args.data",
                "tag_encoding": "bytes32_ascii",
                "max_elements": 32,
                "unknown_tag_policy": "warn",
                "resolve_from_inputs": { "compound_v3_base_asset": "comet" },
                "per_tag": {
                    "ACTION_SUPPLY_ASSET": {
                        "inputs_abi": "(address comet, address to, address asset, uint256 amount)",
                        "body": {
                            "domain": "lending",
                            "lending": {
                                "action": "supply",
                                "supply": {
                                    "venue": {
                                        "name": "compound_v3",
                                        "chain": "$chain",
                                        "comet": "$inputs.comet",
                                        "base_asset": { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.compound_v3_base_asset" } }
                                    },
                                    "asset": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.asset" } },
                                    "amount": "$inputs.amount",
                                    "on_behalf_of": "$inputs.to"
                                }
                            }
                        },
                        "live_inputs": {
                            "reserve_state": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_reserve_state" }, "ttl_s": 30 },
                            "supply_apy": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_supply_apy" }, "ttl_s": 30 },
                            "a_token_price_usd": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_a_token_price_usd" }, "ttl_s": 60 },
                            "eligible_as_collat": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_eligible_as_collat" }, "ttl_s": 60 },
                            "user_state_before": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_user_state_before" }, "ttl_s": 12 }
                        }
                    },
                    "ACTION_CLAIM_REWARD": {
                        "inputs_abi": "(address comet, address rewards, address src, bool shouldAccrue)",
                        "body": {
                            "domain": "airdrop",
                            "airdrop": {
                                "action": "claim",
                                "claim": {
                                    "source": { "name": "compound" },
                                    "claim_target": { "kind": "staking_claim", "chain": "$chain", "contract": "$inputs.rewards" },
                                    "recipient": "$inputs.src",
                                    "proof": null,
                                    "sig": null
                                }
                            }
                        },
                        "live_inputs": {
                            "is_still_claimable": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_is_still_claimable" }, "ttl_s": 30 },
                            "actual_amount": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_actual_amount" }, "ttl_s": 60 },
                            "claim_token": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_claim_token" }, "ttl_s": 60 },
                            "claim_window": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "compound_v3_claim_window" }, "ttl_s": 60 }
                        }
                    }
                }
            },
            "requires": {
                "imperative": [],
                "adapter_capabilities": [],
                "host_capabilities": [],
                "extension": ">=0.1.0"
            }
        });
        let installed: Value =
            serde_json::from_str(&declarative_install_v3_json(bundle.to_string())).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let route_input = json!({
            "chain_id": 1,
            "to": bulker,
            "selector": invoke_selector,
            "calldata": invoke_hex,
            "value": "0",
            "submitter": user,
            "submitted_at": 1_700_000_000_u64
        });
        let out: Value =
            serde_json::from_str(&declarative_route_request_v3_json(route_input.to_string()))
                .unwrap();
        assert_eq!(out["ok"], true, "{out}");

        // One routed Action whose body is a Multicall of the two legs.
        let routed = out["data"]["actions"].as_array().unwrap();
        assert_eq!(routed.len(), 1, "{out}");
        let body = &routed[0]["body"];
        assert_eq!(body["domain"], "multicall", "{body}");
        let children = body["actions"].as_array().unwrap();
        assert_eq!(children.len(), 2, "{body}");

        // Leg 0 — lending::supply, base_asset RESOLVED FROM the decoded comet.
        // ActionBody serialises internally-tagged (flat): `{domain, action, …}`.
        let c0 = &children[0];
        assert_eq!(c0["domain"], "lending", "{c0}");
        assert_eq!(c0["action"], "supply", "{c0}");
        assert_eq!(c0["venue"]["name"], "compound_v3", "{c0}");
        assert_eq!(c0["venue"]["comet"].as_str().unwrap().to_lowercase(), comet);
        assert_eq!(
            c0["venue"]["base_asset"]["key"]["address"]
                .as_str()
                .unwrap()
                .to_lowercase(),
            usdc,
            "base_asset must resolve from $inputs.comet (cUSDCv3→USDC), not $to (the Bulker): {c0}"
        );
        assert_eq!(
            c0["asset"]["key"]["address"]
                .as_str()
                .unwrap()
                .to_lowercase(),
            asset
        );
        let amount_hex = c0["amount"].as_str().unwrap();
        let amount = u64::from_str_radix(amount_hex.strip_prefix("0x").unwrap_or(amount_hex), 16)
            .unwrap_or_else(|_| panic!("amount not hex: {amount_hex}"));
        assert_eq!(amount, 5000, "{c0}");
        assert_eq!(c0["on_behalf_of"].as_str().unwrap().to_lowercase(), user);

        // Leg 1 — airdrop::claim, recipient = decoded src, target = decoded rewards.
        let c1 = &children[1];
        assert_eq!(c1["domain"], "airdrop", "{c1}");
        assert_eq!(c1["action"], "claim", "{c1}");
        assert_eq!(
            c1["recipient"].as_str().unwrap().to_lowercase(),
            src,
            "{c1}"
        );
        assert_eq!(
            c1["claim_target"]["contract"]
                .as_str()
                .unwrap()
                .to_lowercase(),
            rewards,
            "{c1}"
        );
    }

    #[test]
    fn multicall_call_array_routes_each_leg_by_its_own_to() {
        // Per-leg-to: the child mapper is installed ONLY at `adapter`. If the
        // decode wrongly used a fixed outer `to` it would MISS — so a successful
        // decode structurally proves each leg is routed at its own `Call.to`.
        let adapter = "0x000000000000000000000000000000000000ada9";
        let submitter = "0x000000000000000000000000000000000000aaaa";
        let zero32 = "0x0000000000000000000000000000000000000000000000000000000000000000";

        let child_fn = Function::parse("foo(uint256)").unwrap();
        let child_calldata = child_fn
            .abi_encode_input(&[DynSolValue::Uint(U256::from(777_u64), 256)])
            .unwrap();
        let child_selector = format!("0x{}", hex::encode(&child_calldata[0..4]));
        let child_hex = format!("0x{}", hex::encode(&child_calldata));

        let child_bundle = json!({
            "type": "adapter_action",
            "id": "test/mca-child@1.0.0",
            "publisher": "test",
            "schema_version": "3",
            "match": { "selector": child_selector, "chain_to_addresses": { "1": [adapter] } },
            "abi_fragment": {
                "function_name": "foo",
                "abi": {
                    "name": "foo", "type": "function", "stateMutability": "nonpayable",
                    "inputs": [{ "name": "amount", "type": "uint256" }], "outputs": []
                }
            },
            "emit": {
                "strategy": "single_emit",
                "body": { "domain": "unknown", "unknown": {
                    "target": "$to", "chain": "$chain", "calldata": "$calldata", "value": "$tx.value"
                } }
            },
            "requires": { "imperative": [], "adapter_capabilities": [], "host_capabilities": [], "extension": ">=0.1.0" }
        });
        let installed: Value =
            serde_json::from_str(&declarative_install_v3_json(child_bundle.to_string())).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        // Decoded `Call[]` bundle: each leg is a POSITIONAL tuple array
        // [to, data, value, skipRevert, callbackHash]. One mapped leg → adapter.
        let args = json!({ "bundle": [[adapter, child_hex, "0", false, zero32]] });
        let body = build_multicall_call_array_body(
            1,
            submitter,
            1_700_000_000,
            &args,
            &json!({ "strategy": "multicall_call_array", "recurse_arg": "bundle", "max_depth": 3 }),
        )
        .unwrap();
        let body_json = serde_json::to_value(body).unwrap();
        assert_eq!(body_json["domain"], "multicall", "{body_json}");
        assert_eq!(body_json["actions"].as_array().unwrap().len(), 1);
        assert_eq!(body_json["actions"][0]["domain"], "unknown", "{body_json}");
        // (Per-leg-to is proven structurally: the child mapper is installed ONLY at
        // `adapter`, so a successful decode REQUIRES routing the leg at its own `to`.)

        // A bundle whose ONLY leg targets an uninstalled `to` resolves nothing →
        // reject (never an empty no-op). The all-empty rejection becomes the
        // caller's warn-closed fault — the registry's `warn`=skip intent for a
        // PARTIAL batch is preserved; only an empty result is escalated.
        let unmapped = "0x000000000000000000000000000000000000dead";
        let only_unmapped = json!({ "bundle": [[unmapped, child_hex, "0", false, zero32]] });
        let err = build_multicall_call_array_body(
            1,
            submitter,
            1_700_000_000,
            &only_unmapped,
            &json!({ "strategy": "multicall_call_array", "recurse_arg": "bundle" }),
        )
        .unwrap_err();
        assert!(
            err.message.contains("no inner leg resolved"),
            "expected resolved==0 rejection, got {err:?}"
        );
    }

    #[test]
    fn multicall_call_array_unknown_vault_metamorpho_fails_whole_bundle() {
        // D-A: a GeneralAdapter1 `erc4626*` leg whose vault is OUTSIDE the
        // committed `metamorpho_underlying` snapshot cannot resolve its required
        // `asset` (the underlying is a runtime arg, not in calldata) → placeholder
        // substitution falls back to 0x0. We REFUSE the whole bundle rather than
        // emit a confidently-wrong 0x0-asset Supply. A KNOWN vault still decodes
        // with its real underlying (proving the guard is value-gated, not a blanket
        // metamorpho block).
        let adapter = "0x4a6c312ec70e8747a587ee860a0353cd42be0ae0"; // GeneralAdapter1
        let submitter = "0x000000000000000000000000000000000000aaaa";
        let zero32 = "0x0000000000000000000000000000000000000000000000000000000000000000";

        let erc4626 = Function::parse(
            "erc4626Deposit(address vault, uint256 assets, uint256 maxSharePriceE27, address receiver)",
        )
        .unwrap();
        let probe = erc4626
            .abi_encode_input(&[
                DynSolValue::Address(AlloyAddress::ZERO),
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Address(AlloyAddress::ZERO),
            ])
            .unwrap();
        let selector = format!("0x{}", hex::encode(&probe[0..4]));

        // Synthetic copy of the real GA1 erc4626Deposit manifest (body +
        // live_inputs skeleton); only the `match` is test-local.
        let manifest = json!({
            "type": "adapter_action",
            "id": "test/mca-erc4626@1.0.0",
            "publisher": "test",
            "schema_version": "3",
            "match": { "selector": selector, "chain_to_addresses": { "1": [adapter] } },
            "abi_fragment": {
                "function_name": "erc4626Deposit",
                "abi": {
                    "name": "erc4626Deposit", "type": "function", "stateMutability": "nonpayable",
                    "inputs": [
                        { "name": "vault", "type": "address" },
                        { "name": "assets", "type": "uint256" },
                        { "name": "maxSharePriceE27", "type": "uint256" },
                        { "name": "receiver", "type": "address" }
                    ],
                    "outputs": []
                }
            },
            "emit": {
                "strategy": "single_emit",
                "body": { "domain": "lending", "lending": { "action": "supply", "supply": {
                    "venue": { "name": "metamorpho", "chain": "$chain", "vault": "$args.vault" },
                    "asset": { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.metamorpho_underlying" } },
                    "amount": "$args.assets",
                    "on_behalf_of": "$args.receiver"
                } } },
                "live_inputs": {
                    "reserve_state": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "metamorpho_reserve_state_skeleton" }, "ttl_s": 30 },
                    "supply_apy": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "metamorpho_supply_apy_skeleton" }, "ttl_s": 30 },
                    "a_token_price_usd": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "metamorpho_share_price_skeleton" }, "ttl_s": 60 },
                    "eligible_as_collat": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "metamorpho_collat_flag_skeleton" }, "ttl_s": 60 },
                    "user_state_before": { "source": { "kind": "derived_from", "inputs": [], "calc_id": "metamorpho_user_state_skeleton" }, "ttl_s": 12 }
                }
            },
            "requires": { "imperative": [], "adapter_capabilities": [], "host_capabilities": [], "extension": ">=0.1.0" }
        });
        let installed: Value =
            serde_json::from_str(&declarative_install_v3_json(manifest.to_string())).unwrap();
        assert_eq!(installed["ok"], true, "{installed}");

        let receiver: AlloyAddress = submitter.parse().unwrap();
        let emit =
            json!({ "strategy": "multicall_call_array", "recurse_arg": "bundle", "max_depth": 3 });
        let encode_leg = |vault: &str| -> String {
            let cd = erc4626
                .abi_encode_input(&[
                    DynSolValue::Address(vault.parse().unwrap()),
                    DynSolValue::Uint(U256::from(1_000_000_u64), 256),
                    DynSolValue::Uint(U256::ZERO, 256),
                    DynSolValue::Address(receiver),
                ])
                .unwrap();
            format!("0x{}", hex::encode(cd))
        };

        // KNOWN vault (snapshot[0]) → underlying resolves to USDC, decodes clean.
        let known_vault = "0x0b2d98bbf3e38df1d1b7be7343732e32e8b1f818";
        let known_underlying: AlloyAddress = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            .parse()
            .unwrap();
        let known_bundle =
            json!({ "bundle": [[adapter, encode_leg(known_vault), "0", false, zero32]] });
        let body =
            build_multicall_call_array_body(1, submitter, 1_700_000_000, &known_bundle, &emit)
                .expect("known-vault metamorpho leg should decode");
        let v3_action::ActionBody::Multicall { actions } = body else {
            panic!("expected a multicall body");
        };
        assert_eq!(actions.len(), 1);
        let v3_action::ActionBody::Lending(v3_action::lending::LendingAction::Supply(supply)) =
            &actions[0]
        else {
            panic!("expected a metamorpho supply leg, got {:?}", actions[0]);
        };
        assert_eq!(
            supply.asset.key.contract(),
            Some(&known_underlying),
            "known vault underlying must resolve from the snapshot"
        );

        // UNKNOWN vault (not in the snapshot) → underlying unresolved → the whole
        // bundle is REFUSED (no 0x0-asset Supply leaks through).
        let unknown_vault = "0x000000000000000000000000000000000000dead";
        let unknown_bundle =
            json!({ "bundle": [[adapter, encode_leg(unknown_vault), "0", false, zero32]] });
        let err =
            build_multicall_call_array_body(1, submitter, 1_700_000_000, &unknown_bundle, &emit)
                .unwrap_err();
        assert!(
            err.message.contains("underlying unresolved"),
            "expected a metamorpho-underlying rejection, got {err:?}"
        );
    }

    #[test]
    fn decode_reenter_call_array_decodes_callback_legs() {
        // The reenter callback is raw abi.encode(Call[]) (the route surfaces it as
        // `data.reenter_callback` via the manifest's `reenter_callback_arg`). The
        // decoder is protocol-agnostic — no selector knowledge. Build a 2-leg
        // callback and assert the positional Call[] decode.
        let a1 = AlloyAddress::from([0x11; 20]);
        let a2 = AlloyAddress::from([0x22; 20]);
        let mk_call = |to: AlloyAddress, sel: u32| {
            DynSolValue::Tuple(vec![
                DynSolValue::Address(to),
                DynSolValue::Bytes(sel.to_be_bytes().to_vec()),
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Bool(false),
                DynSolValue::FixedBytes(alloy_primitives::B256::ZERO, 32),
            ])
        };
        // abi.encode(Call[]) == the args encoding of `reenter(Call[])` minus its
        // 4-byte selector.
        let reenter =
            Function::parse("reenter((address,bytes,uint256,bool,bytes32)[] bundle)").unwrap();
        let reenter_calldata = reenter
            .abi_encode_input(&[DynSolValue::Array(vec![
                mk_call(a1, 0xaabb_ccdd),
                mk_call(a2, 0x1122_3344),
            ])])
            .unwrap();
        let callback_hex = format!("0x{}", hex::encode(&reenter_calldata[4..]));

        let legs = decode_reenter_call_array(&callback_hex)
            .unwrap()
            .expect("non-empty callback");
        assert_eq!(legs.len(), 2, "callback Call[] should decode 2 legs");
        let leg_to = |i: usize| {
            legs[i].as_array().unwrap()[0]
                .as_str()
                .unwrap()
                .parse::<AlloyAddress>()
                .unwrap()
        };
        assert_eq!(leg_to(0), a1, "leg0 `to`");
        assert_eq!(leg_to(1), a2, "leg1 `to`");

        // Empty callback → None (a plain leg with no re-entry).
        assert!(decode_reenter_call_array("0x").unwrap().is_none());
        assert!(decode_reenter_call_array("").unwrap().is_none());
    }

    #[test]
    fn decode_inputs_abi_tuple_handles_v4_path_key_arrays() {
        let currency_in = AlloyAddress::ZERO;
        let currency_out = AlloyAddress::from([0x22; 20]);
        let hook = AlloyAddress::from([0x33; 20]);
        let path = DynSolValue::Array(vec![DynSolValue::Tuple(vec![
            DynSolValue::Address(currency_out),
            DynSolValue::Uint(U256::from(500_u64), 24),
            DynSolValue::Int(alloy_primitives::I256::try_from(60_i64).unwrap(), 24),
            DynSolValue::Address(hook),
            DynSolValue::Bytes(vec![0xab, 0xcd]),
        ])]);
        let encoder =
            Function::parse("step(address,(address,uint24,int24,address,bytes)[],uint128,uint128)")
                .unwrap();
        let encoded = encoder
            .abi_encode_input(&[
                DynSolValue::Address(currency_in),
                path,
                DynSolValue::Uint(U256::from(1_000_u64), 128),
                DynSolValue::Uint(U256::from(900_u64), 128),
            ])
            .unwrap();

        let decoded = decode_inputs_abi_tuple(
            "(address currencyIn, (address,uint24,int24,address,bytes)[] path, uint128 amountIn, uint128 amountOutMinimum)",
            &encoded[4..],
        )
        .unwrap();

        assert_eq!(
            decoded["currencyIn"],
            json!("0x0000000000000000000000000000000000000000")
        );
        assert_eq!(decoded["amountIn"], json!("1000"));
        assert_eq!(decoded["amountOutMinimum"], json!("900"));
        assert_eq!(decoded["path"][0][0], json!(format!("{currency_out:?}")));
        assert_eq!(decoded["path"][0][1], json!(500_u64));
        assert_eq!(decoded["path"][0][2], json!(60_i64));
        assert_eq!(decoded["path"][0][3], json!(format!("{hook:?}")));
        assert_eq!(decoded["path"][0][4], json!("0xabcd"));
    }

    #[test]
    fn maybe_inject_v4_pool_id_handles_v4_swap_params_tuple() {
        let currency0 = AlloyAddress::from([0x11; 20]);
        let currency1 = AlloyAddress::from([0x22; 20]);
        let hook = AlloyAddress::from([0x33; 20]);
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(currency0),
            DynSolValue::Address(currency1),
            DynSolValue::Uint(U256::from(500_u64), 24),
            DynSolValue::Int(alloy_primitives::I256::try_from(60_i64).unwrap(), 24),
            DynSolValue::Address(hook),
        ]);
        let encoder = Function::parse(
            "step(((address,address,uint24,int24,address),bool,uint128,uint128,bytes))",
        )
        .unwrap();
        let encoded = encoder
            .abi_encode_input(&[DynSolValue::Tuple(vec![
                pool_key,
                DynSolValue::Bool(true),
                DynSolValue::Uint(U256::from(1_000_u64), 128),
                DynSolValue::Uint(U256::from(900_u64), 128),
                DynSolValue::Bytes(Vec::new()),
            ])])
            .unwrap();

        let mut decoded = decode_inputs_abi_tuple(
            "(((address,address,uint24,int24,address),bool,uint128,uint128,bytes) params)",
            &encoded[4..],
        )
        .unwrap();
        maybe_inject_v4_pool_id(&mut decoded);

        let expected = compute_v4_pool_id(
            &format!("{currency0:?}"),
            &format!("{currency1:?}"),
            &json!(500_u64),
            &json!(60_i64),
            &format!("{hook:?}"),
        )
        .unwrap();
        assert_eq!(decoded["pool_id"], json!(expected));
    }

    #[test]
    fn maybe_inject_uniswap_v3_path_extracts_endpoints_and_fee() {
        let mut path = Vec::new();
        path.extend_from_slice(&[0x11; 20]);
        path.extend_from_slice(&[0x00, 0x01, 0xf4]);
        path.extend_from_slice(&[0x22; 20]);
        path.extend_from_slice(&[0x00, 0x0b, 0xb8]);
        path.extend_from_slice(&[0x33; 20]);

        let args = json!({ "path": format!("0x{}", hex::encode(path)) });
        let mut derived = BTreeMap::new();
        maybe_inject_uniswap_v3_path(&args, &mut derived);

        assert_eq!(
            derived["v3_path_first_token"],
            json!("0x1111111111111111111111111111111111111111")
        );
        assert_eq!(
            derived["v3_path_last_token"],
            json!("0x3333333333333333333333333333333333333333")
        );
        assert_eq!(derived["fee_tier_bp"], json!(500_u64));
    }

    #[test]
    fn decode_stream_inputs_uses_v4_inputs_abi_alternatives_and_normalizes_params() {
        let currency_in = AlloyAddress::ZERO;
        let currency_out = AlloyAddress::from([0x22; 20]);
        let path = DynSolValue::Array(vec![DynSolValue::Tuple(vec![
            DynSolValue::Address(currency_out),
            DynSolValue::Uint(U256::from(500_u64), 24),
            DynSolValue::Int(alloy_primitives::I256::try_from(60_i64).unwrap(), 24),
            DynSolValue::Address(AlloyAddress::ZERO),
            DynSolValue::Bytes(Vec::new()),
        ])]);
        let encoder = Function::parse(
            "step((address,(address,uint24,int24,address,bytes)[],uint128,uint128))",
        )
        .unwrap();
        let encoded = encoder
            .abi_encode_input(&[DynSolValue::Tuple(vec![
                DynSolValue::Address(currency_in),
                path,
                DynSolValue::Uint(U256::from(1_000_u64), 128),
                DynSolValue::Uint(U256::from(900_u64), 128),
            ])])
            .unwrap();
        let inputs = vec![json!(format!("0x{}", hex::encode(&encoded[4..])))];
        let mut table = serde_json::Map::new();
        table.insert(
            "0x07".to_owned(),
            json!({
                "inputs_abi": "((address,(address,uint24,int24,address,bytes)[],uint256[],uint128,uint128) params)",
                "inputs_abi_alternatives": [
                    "((address,(address,uint24,int24,address,bytes)[],uint128,uint128) params)"
                ]
            }),
        );

        let decoded = decode_stream_inputs(&table, &[0x07], &inputs, 0xff).unwrap();

        assert_eq!(
            decoded[0]["currencyIn"],
            json!("0x0000000000000000000000000000000000000000")
        );
        assert_eq!(decoded[0]["amountIn"], json!("1000"));
        assert_eq!(decoded[0]["amountOutMinimum"], json!("900"));
    }
}
