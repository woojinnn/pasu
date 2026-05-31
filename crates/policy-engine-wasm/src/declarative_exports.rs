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
//!     `simulation_reducer::action::Action`).
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

// Phase 4B — v3 action tree imports.
use simulation_reducer::action as v3_action;
use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
use simulation_state::primitives::{
    Address as V3Address, ChainId as V3ChainId, Time as V3Time, U256 as V3U256,
};

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
// Parallel to `declarative_install_json` (v1) but stores the raw manifest in
// `DECLARATIVE_V3_STATE` so [`declarative_route_request_v3_json`] can route
// against the v3 `emit.body` / `emit.live_inputs` / `emit.per_opcode_body`
// templates. The v1 install path is untouched.
//
// The v3 install validates only the structural envelope:
//   * `bundle.id`         — required, non-empty string. Used as decoder_id.
//   * `bundle.match`      — parsed via `BundleMatch` so v1 (`chain_ids × to`)
//                           and v2 (`chain_to_addresses`) bundles both yield
//                           `(chain_id, address)` pairs.
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
/// `decoder_id` and `bundle_id` are populated to the same value so the wire
/// shape stays identical to v1 [`DeclarativeInstallResultDto`].
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
// Phase 4B — `declarative_route_request_v3_json`
// ───────────────────────────────────────────────────────────────────────────
//
// v3 route entry. Looks up the callkey through the bridge populated by
// `declarative_install_v3_json`, decodes the calldata against the bundle's
// ABI, then runs the manifest's emit-rule via `action_builder` to produce
// the hierarchical `simulation_reducer::action::Action` tree (PDF FSM spec).
//
// Phase 4B scope = **WASM boundary only**:
//   * Define the TS↔Rust wire (input/output JSON shape).
//   * Build a minimal stub `Action` whose body is `ActionBody::Unknown` —
//     the registry-v2 manifest lookup + decode + emit-rule → ActionBody
//     conversion is the responsibility of Phase 4D (Multicall handler /
//     adapters-v3 crate). The stub is enough to wire the SW orchestrator
//     and round-trip through Cedar without blocking on the full mapping.
//
// Legacy `declarative_route_request_json` stays untouched — Phase 4 keeps
// both entries in parallel so the existing SW path (envelope-driven Cedar
// pipeline) continues to function during cutover.

/// Phase 4B — v3 orchestrator entry emitting the PDF FSM `Action` tree.
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
/// Phase 4B emits a single-element `actions` vec whose body is
/// `ActionBody::Unknown { target, chain, calldata, value }` — the policy
/// engine downstream evaluates Unknown with a warn/deny default per
/// `action-design.md`. Phase 4D replaces this stub with the registry-v2
/// manifest lookup + emit-rule decode pipeline.
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
        //      can either fall through to the v1 path or surface the gap.
        //   2. Decode the raw calldata against the manifest's
        //      `abi_fragment.abi` (same JSON-ABI helper v1 uses).
        //   3. Build a [`V3MapContext`] from the request + the decoded args
        //      (`args_to_json` mirrors the v1 eval convention so M3 / M4 do
        //      not have to learn a second arg shape).
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
                state.bridge.get(&key).and_then(|bundle_id| {
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

        // Decode calldata against the manifest ABI (same pattern as v1). A bare
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

        // Plan §M5 — chain 별 well-known token addresses pre-populate.
        // Sync orchestrator (별 plan) 가 채울 동적 resolved (pool / factory
        // 등) 외에, chain ID 만으로 결정되는 정적 token address (WETH 등) 는
        // 본 layer 에서 미리 채워 manifest 의 `$resolved.weth` 같은 placeholder
        // 가 zero address fallback 대신 정확한 값으로 substitute 되도록 함.
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

        // Tier-B synthetic derivations the declarative grammar cannot express
        // (it can index/slice but not hash). Morpho Blue's `market_id` =
        // keccak(MarketParams); inject it as `$derived.morpho_market_id` so the
        // single_emit `LendingVenue::MorphoBlue.market_id` field resolves. A
        // no-op for every non-Morpho call (shape-gated on a `marketParams`
        // 5-tuple). The single_emit analogue of `maybe_inject_v4_pool_id`.
        let mut derived = BTreeMap::new();
        maybe_inject_morpho_market_id(&args_json, &mut derived);
        maybe_inject_aave_l2_packed_args(
            input.chain_id,
            &key.to,
            &key.selector,
            &args_json,
            &mut derived,
        );
        maybe_inject_balancer_v2_pool_args(&key.selector, &args_json, &mut derived);

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
                let per_item_live_inputs = emit.get("live_inputs");
                build_array_emit(&ctx, array_source, per_item_body, per_item_live_inputs).map_err(
                    |error| EngineErrorDto::new("build_array_emit_failed", error.to_string()),
                )?
            }
            // A-redux.2 — `tagged_dispatch` (HyperLiquid CoreWriter mechanism).
            // ONE action is encoded as `data[0]=version ‖ data[tag..tag+sz]=
            // action_id (uintN BE) ‖ data[tag+sz..]=abi.encode(args)`. Resolve
            // `bytes_source` → bytes, assert the version byte, read the action
            // id, look up `per_action_body["<decimal id>"]`, abi-decode the
            // trailing args with that action's `inputs_abi` into the ctx
            // `inputs`, and build that ONE action's `body` (NOT a Multicall).
            "tagged_dispatch" => build_tagged_dispatch(&ctx, emit)?,
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
            other => {
                return Err(EngineErrorDto::new(
                    "unsupported_strategy",
                    format!("unsupported emit.strategy: {other}"),
                ));
            }
        };

        let action = v3_action::Action { meta, body };

        Ok(DeclarativeRouteRequestV3ResultDto {
            actions: vec![action],
            decoder_id: bundle_id,
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
        );

        // ── V3MapContext (same resolved/derived population as calldata) ─────
        // Plan §M5 — static WETH-by-chain pre-populate (mirrors the calldata
        // route path so `$resolved.weth` substitutes the correct address).
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
            let per_item_live_inputs = emit.get("live_inputs");
            build_array_emit(&ctx, array_source, per_item_body, per_item_live_inputs).map_err(
                |error| EngineErrorDto::new("build_array_emit_failed", error.to_string()),
            )?
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
fn build_typed_data_args_json(
    abi: Option<&serde_json::Value>,
    primary_type: &str,
    message: &serde_json::Value,
) -> serde_json::Value {
    let payload_inputs: Option<Vec<(String, String)>> = abi
        .and_then(|abi| abi.get("inputs"))
        .and_then(serde_json::Value::as_array)
        .map(|inputs| {
            inputs
                .iter()
                .filter_map(|i| {
                    let name = i.get("name").and_then(serde_json::Value::as_str)?;
                    let ty = i.get("type").and_then(serde_json::Value::as_str)?;
                    if matches!(name, "owner" | "signature" | "v" | "r" | "s") {
                        None
                    } else {
                        Some((name.to_owned(), ty.to_owned()))
                    }
                })
                .collect()
        });

    match payload_inputs {
        // Single tuple payload → wrap under its param name.
        Some(ref payload) if payload.len() == 1 && payload[0].1.starts_with("tuple") => {
            serde_json::json!({ payload[0].0.clone(): message.clone() })
        }
        // Multiple scalars (or single non-tuple) → flat, no wrap.
        Some(payload) if !payload.is_empty() => message.clone(),
        // Fallback: inputs missing / empty / unreadable → wrap under the
        // lower-camel-cased primary_type (the dominant single-tuple shape).
        _ => serde_json::json!({ primary_type_to_lower_camel(primary_type): message.clone() }),
    }
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
            derived: ctx.derived.clone(),
            inputs: inputs_for_this,
        };
        let child_action = build_action_body(&child_ctx, body_template, None)
            .map_err(|error| EngineErrorDto::new("build_multicall_failed", error.to_string()))?;
        actions.push(child_action);
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
        derived: ctx.derived.clone(),
        inputs: Some(&decoded),
    };
    build_action_body(&child_ctx, body_template, action_entry.get("live_inputs"))
        .map_err(|error| EngineErrorDto::new("build_action_body_failed", error.to_string()))
}

/// Decode a single opcode's `inputs_abi` Solidity tuple signature against a
/// raw byte buffer, returning a JSON object keyed by the tuple's named
/// fields.
///
/// M2 narrow scope (per plan §3 "본 wire-up 만, 실제 inputs_abi decode
/// logic 은 M5 의 manual e2e 시 첫 raw Tx 로 검증"):
///   * Reuse [`abi_resolver::decode::decode_with_function`] so we do not
///     pull `alloy_json_abi` / `alloy_dyn_abi` symbols into the WASM
///     surface beyond what abi-resolver already links.
///   * The signature is wrapped into a synthetic `step<sig>` function so
///     alloy can parse it (mirrors `subdecode::opcode_stream`'s pattern).
///     Selector is recomputed from that function so `decode_with_function`'s
///     selector-equality guard always passes — opcode dispatch already
///     verified the outer call site, we are only re-decoding the inner
///     tuple here.
///   * Each `DecodedArg.value` (a `DynSolValue`) routes through the same
///     `bridge::convert_value` → `eval::decoded_value_to_json` chain v1
///     uses, so the resulting `$inputs.<name>` JSON shape matches the v1
///     `$args.<name>` view the action_builder's placeholder walker already
///     understands.
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

fn maybe_inject_aave_l2_packed_args(
    chain_id: u64,
    target: &str,
    selector: &str,
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    if !is_aave_l2_pool(chain_id, target) {
        return;
    }

    match selector {
        // supply(bytes32) / supplyWithPermit(bytes32,bytes32,bytes32)
        "0xf7a73840" | "0x680dd47c" => {
            let Some(args) = aave_l2_bytes32_arg(args_json, "args") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_asset",
                chain_id,
                target,
                aave_l2_u16(args, 0),
            );
            derived.insert(
                "aave_l2_amount".to_owned(),
                serde_json::Value::String(aave_l2_bits(args, 16, 128).to_string()),
            );
        }
        // withdraw(bytes32)
        "0x8e19899e" => {
            let Some(args) = aave_l2_bytes32_arg(args_json, "args") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_asset",
                chain_id,
                target,
                aave_l2_u16(args, 0),
            );
            let amount = aave_l2_expand_uint128_max(aave_l2_bits(args, 16, 128));
            derived.insert(
                "aave_l2_amount".to_owned(),
                serde_json::Value::String(amount.to_string()),
            );
        }
        // borrow(bytes32)
        "0xd5eed868" => {
            let Some(args) = aave_l2_bytes32_arg(args_json, "args") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_asset",
                chain_id,
                target,
                aave_l2_u16(args, 0),
            );
            derived.insert(
                "aave_l2_amount".to_owned(),
                serde_json::Value::String(aave_l2_bits(args, 16, 128).to_string()),
            );
            derived.insert(
                "aave_l2_rate_mode".to_owned(),
                serde_json::Value::Number(serde_json::Number::from(aave_l2_u8(args, 144))),
            );
        }
        // repay(bytes32) / repayWithPermit(bytes32,bytes32,bytes32) / repayWithATokens(bytes32)
        "0x563dd613" | "0x94b576de" | "0xdc7c0bff" => {
            let Some(args) = aave_l2_bytes32_arg(args_json, "args") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_asset",
                chain_id,
                target,
                aave_l2_u16(args, 0),
            );
            let amount = aave_l2_expand_uint128_max(aave_l2_bits(args, 16, 128));
            derived.insert(
                "aave_l2_amount".to_owned(),
                serde_json::Value::String(amount.to_string()),
            );
            derived.insert(
                "aave_l2_rate_mode".to_owned(),
                serde_json::Value::Number(serde_json::Number::from(aave_l2_u8(args, 144))),
            );
        }
        // setUserUseReserveAsCollateral(bytes32)
        "0x4d013f03" => {
            let Some(args) = aave_l2_bytes32_arg(args_json, "args") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_asset",
                chain_id,
                target,
                aave_l2_u16(args, 0),
            );
            derived.insert(
                "aave_l2_use_as_collateral".to_owned(),
                serde_json::Value::Bool(aave_l2_u8(args, 16) != 0),
            );
        }
        // liquidationCall(bytes32,bytes32)
        "0xfd21ecff" => {
            let Some(args1) = aave_l2_bytes32_arg(args_json, "args1") else {
                return;
            };
            let Some(args2) = aave_l2_bytes32_arg(args_json, "args2") else {
                return;
            };
            inject_aave_l2_asset(
                derived,
                "aave_l2_collat_asset",
                chain_id,
                target,
                aave_l2_u16(args1, 0),
            );
            inject_aave_l2_asset(
                derived,
                "aave_l2_debt_asset",
                chain_id,
                target,
                aave_l2_u16(args1, 16),
            );
            derived.insert(
                "aave_l2_user".to_owned(),
                serde_json::Value::String(aave_l2_address(args1, 32)),
            );
            let debt_to_cover = aave_l2_expand_uint128_max(aave_l2_bits(args2, 0, 128));
            derived.insert(
                "aave_l2_debt_to_cover".to_owned(),
                serde_json::Value::String(debt_to_cover.to_string()),
            );
            derived.insert(
                "aave_l2_receive_a_token".to_owned(),
                serde_json::Value::Bool(aave_l2_u8(args2, 128) != 0),
            );
        }
        _ => {}
    }
}

fn is_aave_l2_pool(chain_id: u64, target: &str) -> bool {
    match chain_id {
        8453 => target.eq_ignore_ascii_case("0xa238dd80c259a72e81d7e4664a9801593f98d1c5"),
        42161 => target.eq_ignore_ascii_case("0x794a61358d6845594f94dc1db02a252b5b4814ad"),
        _ => false,
    }
}

fn inject_aave_l2_asset(
    derived: &mut BTreeMap<String, serde_json::Value>,
    key: &str,
    chain_id: u64,
    target: &str,
    reserve_id: u64,
) {
    if let Some(address) = aave_l2_reserve_address(chain_id, target, reserve_id) {
        derived.insert(
            key.to_owned(),
            serde_json::Value::String(address.to_owned()),
        );
    }
}

fn aave_l2_reserve_address(chain_id: u64, target: &str, reserve_id: u64) -> Option<&'static str> {
    if !is_aave_l2_pool(chain_id, target) {
        return None;
    }

    // Aave V3 L2Pool packed calldata carries a uint16 reserve id instead of
    // an address. These snapshots come from the pool's `getReservesList()`.
    match chain_id {
        8453 => match reserve_id {
            0 => Some("0x4200000000000000000000000000000000000006"),
            1 => Some("0x2ae3f1ec7f1f5012cfeab0185bfc7aa3cf0dec22"),
            2 => Some("0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca"),
            3 => Some("0xc1cba3fcea344f92d9239c08c0568f6f2f0ee452"),
            4 => Some("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913"),
            5 => Some("0x04c0599ae5a44757c0af6f9ec3b93da8976c150a"),
            6 => Some("0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf"),
            7 => Some("0x2416092f143378750bb29b79ed961ab195cceea5"),
            8 => Some("0x6bb7a212910682dcfdbd5bcbb3e28fb4e8da10ee"),
            9 => Some("0xedfa23602d0ec14714057867a78d01e94176bea0"),
            10 => Some("0xecac9c5f704e954931349da37f60e39f515c11c1"),
            11 => Some("0x60a3e35cc302bfa44cb288bc5a4f316fdb1adb42"),
            12 => Some("0x63706e401c06ac8513145b7687a14804d17f814b"),
            13 => Some("0x236aa50979d5f3de3bd1eeb40e81137f22ab794b"),
            14 => Some("0x660975730059246a68521a3e2fbd4740173100f5"),
            _ => None,
        },
        42161 => match reserve_id {
            0 => Some("0xda10009cbd5d07dd0cecc66161fc93d7c9000da1"),
            1 => Some("0xf97f4df75117a78c1a5a0dbb814af92458539fb4"),
            2 => Some("0xff970a61a04b1ca14834a43f5de4533ebddb5cc8"),
            3 => Some("0x2f2a2543b76a4166549f7aab2e75bef0aefc5b0f"),
            4 => Some("0x82af49447d8a07e3bd95bd0d56f35241523fbab1"),
            5 => Some("0xfd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb9"),
            6 => Some("0xba5ddd1f9d7f570dc94a51479a000e3bce967196"),
            7 => Some("0xd22a58f79e9481d1a88e00c343885a588b34b68b"),
            8 => Some("0x5979d7b546e38e414f7e9822514be443a4800529"),
            9 => Some("0x3f56e0c36d275367b8c502090edf38289b3dea0d"),
            10 => Some("0xec70dcb4a1efa46b8f2d97c310c9c4790ba5ffa8"),
            11 => Some("0x93b346b6bc2548da6a1e7d98e9a421b42541425b"),
            12 => Some("0xaf88d065e77c8cc2239327c5edb3a432268e5831"),
            13 => Some("0x17fc002b466eec40dae837fc4be5c67993ddbd6f"),
            14 => Some("0x912ce59144191c1204e64559fe8253a0e49e6548"),
            15 => Some("0x35751007a407ca6feffe80b3cb397736d2cf4dbe"),
            16 => Some("0x7dff72693f6a4149b17e7c6314655f6a9f7c8b33"),
            17 => Some("0x2416092f143378750bb29b79ed961ab195cceea5"),
            18 => Some("0x4186bfc76e2e237523cbc30fd220fe055156b41f"),
            19 => Some("0x6c84a8f1c29108f47a79964b5fe888d4f4d0de40"),
            _ => None,
        },
        _ => None,
    }
}

fn aave_l2_bytes32_arg(
    args_json: &serde_json::Value,
    name: &str,
) -> Option<alloy_primitives::U256> {
    let raw = args_json.get(name)?.as_str()?.strip_prefix("0x")?;
    if raw.len() != 64 {
        return None;
    }
    let bytes = hex::decode(raw).ok()?;
    let bytes: [u8; 32] = bytes.try_into().ok()?;
    Some(alloy_primitives::U256::from_be_bytes(bytes))
}

fn aave_l2_bits(word: alloy_primitives::U256, shift: usize, bits: usize) -> alloy_primitives::U256 {
    debug_assert!(bits < 256);
    let mask = (alloy_primitives::U256::from(1u8) << bits) - alloy_primitives::U256::from(1u8);
    (word >> shift) & mask
}

fn aave_l2_u16(word: alloy_primitives::U256, shift: usize) -> u64 {
    aave_l2_bits(word, shift, 16).to::<u64>()
}

fn aave_l2_u8(word: alloy_primitives::U256, shift: usize) -> u64 {
    aave_l2_bits(word, shift, 8).to::<u64>()
}

fn aave_l2_address(word: alloy_primitives::U256, shift: usize) -> String {
    let value = aave_l2_bits(word, shift, 160);
    let bytes = value.to_be_bytes::<32>();
    format!("0x{}", hex::encode(&bytes[12..32]))
}

fn aave_l2_expand_uint128_max(amount: alloy_primitives::U256) -> alloy_primitives::U256 {
    if amount == alloy_primitives::U256::from(u128::MAX) {
        alloy_primitives::U256::MAX
    } else {
        amount
    }
}

fn maybe_inject_balancer_v2_pool_args(
    selector: &str,
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    match selector {
        "0x945bcec9" => inject_balancer_v2_batch_swap_args(args_json, derived),
        "0xb95cac28" => inject_balancer_v2_join_args(args_json, derived),
        "0x8bdb3913" => inject_balancer_v2_exit_args(args_json, derived),
        _ => {}
    }
}

fn inject_balancer_v2_pool_address(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    let Some(pool_id) = args_json.get("poolId").and_then(serde_json::Value::as_str) else {
        return;
    };
    let Some(hex) = pool_id.strip_prefix("0x") else {
        return;
    };
    if hex.len() < 40 {
        return;
    }
    derived.insert(
        "balancer_v2_pool_address".to_owned(),
        serde_json::Value::String(format!("0x{}", &hex[..40])),
    );
}

fn inject_balancer_v2_join_args(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    inject_balancer_v2_pool_address(args_json, derived);
    let fallback_amounts = balancer_request_amounts(args_json, 1);
    let parsed = args_json
        .get("request")
        .and_then(serde_json::Value::as_array)
        .and_then(|request| request.get(2))
        .and_then(serde_json::Value::as_str)
        .and_then(|user_data| {
            parse_balancer_v2_join_user_data(user_data, fallback_amounts.clone())
        });
    let (amounts_in, min_lp_out) =
        parsed.unwrap_or((fallback_amounts, alloy_primitives::U256::ZERO));
    derived.insert(
        "balancer_v2_join_amounts_in".to_owned(),
        serde_json::Value::Array(amounts_in),
    );
    derived.insert(
        "balancer_v2_min_lp_out".to_owned(),
        serde_json::Value::String(min_lp_out.to_string()),
    );
}

fn inject_balancer_v2_exit_args(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    inject_balancer_v2_pool_address(args_json, derived);
    let lp_amount = args_json
        .get("request")
        .and_then(serde_json::Value::as_array)
        .and_then(|request| request.get(2))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_balancer_v2_exit_user_data)
        .unwrap_or(alloy_primitives::U256::ZERO);
    derived.insert(
        "balancer_v2_exit_lp_amount".to_owned(),
        serde_json::Value::String(lp_amount.to_string()),
    );
}

fn inject_balancer_v2_batch_swap_args(
    args_json: &serde_json::Value,
    derived: &mut BTreeMap<String, serde_json::Value>,
) {
    let assets = args_json
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let swaps = args_json
        .get("swaps")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    if let Some(pool_id) = swaps
        .first()
        .and_then(|step| balancer_tuple_field(step, 0))
        .and_then(serde_json::Value::as_str)
    {
        derived.insert(
            "balancer_v2_batch_pool_id".to_owned(),
            serde_json::Value::String(pool_id.to_owned()),
        );
        if let Some(pool_addr) = balancer_v2_pool_id_address(pool_id) {
            derived.insert(
                "balancer_v2_batch_pool_address".to_owned(),
                serde_json::Value::String(pool_addr),
            );
        }
    }

    let positive_limit = balancer_batch_limit_side(args_json, true);
    let negative_limit = balancer_batch_limit_side(args_json, false);

    let first_step = swaps.first();
    let last_step = swaps.last();
    let fallback_token_in = first_step
        .and_then(|step| balancer_step_index(step, 1))
        .and_then(|idx| balancer_asset_at(&assets, idx));
    let fallback_token_out = last_step
        .and_then(|step| balancer_step_index(step, 2))
        .and_then(|idx| balancer_asset_at(&assets, idx));
    let fallback_amount_in = first_step
        .and_then(|step| balancer_tuple_field(step, 3))
        .and_then(balancer_json_u256)
        .unwrap_or(alloy_primitives::U256::ZERO);

    let token_in = positive_limit
        .as_ref()
        .map(|(token, _)| token.clone())
        .or(fallback_token_in)
        .unwrap_or_else(|| "0x0000000000000000000000000000000000000000".to_owned());
    let token_out = negative_limit
        .as_ref()
        .map(|(token, _)| token.clone())
        .or(fallback_token_out)
        .unwrap_or_else(|| "0x0000000000000000000000000000000000000000".to_owned());
    let amount_in = positive_limit
        .as_ref()
        .map_or(fallback_amount_in, |(_, amount)| *amount);
    let amount_out = negative_limit
        .as_ref()
        .map_or(alloy_primitives::U256::ZERO, |(_, amount)| *amount);

    derived.insert(
        "balancer_v2_batch_token_in".to_owned(),
        serde_json::Value::String(token_in),
    );
    derived.insert(
        "balancer_v2_batch_token_out".to_owned(),
        serde_json::Value::String(token_out),
    );
    derived.insert(
        "balancer_v2_batch_amount_in".to_owned(),
        serde_json::Value::String(amount_in.to_string()),
    );
    derived.insert(
        "balancer_v2_batch_min_amount_out".to_owned(),
        serde_json::Value::String(amount_out.to_string()),
    );
    derived.insert(
        "balancer_v2_batch_max_amount_in".to_owned(),
        serde_json::Value::String(amount_in.to_string()),
    );
    derived.insert(
        "balancer_v2_batch_amount_out".to_owned(),
        serde_json::Value::String(amount_out.to_string()),
    );
}

fn balancer_v2_pool_id_address(pool_id: &str) -> Option<String> {
    let hex = pool_id.strip_prefix("0x")?;
    if hex.len() < 40 {
        return None;
    }
    Some(format!("0x{}", &hex[..40]))
}

fn balancer_tuple_field(v: &serde_json::Value, index: usize) -> Option<&serde_json::Value> {
    v.as_array().and_then(|arr| arr.get(index))
}

fn balancer_step_index(step: &serde_json::Value, field_index: usize) -> Option<usize> {
    balancer_tuple_field(step, field_index)
        .and_then(balancer_json_u256)?
        .try_into()
        .ok()
}

fn balancer_asset_at(assets: &[serde_json::Value], index: usize) -> Option<String> {
    assets
        .get(index)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn balancer_batch_limit_side(
    args_json: &serde_json::Value,
    positive: bool,
) -> Option<(String, alloy_primitives::U256)> {
    let assets = args_json.get("assets")?.as_array()?;
    let limits = args_json.get("limits")?.as_array()?;
    let mut selected_token: Option<String> = None;
    let mut amount = alloy_primitives::U256::ZERO;

    for (idx, raw) in limits.iter().enumerate() {
        let (is_negative, abs) = balancer_signed_abs(raw)?;
        if abs.is_zero() || (positive && is_negative) || (!positive && !is_negative) {
            continue;
        }
        let Some(token) = balancer_asset_at(assets, idx) else {
            continue;
        };
        match &selected_token {
            None => {
                selected_token = Some(token);
                amount = abs;
            }
            Some(existing) if existing.eq_ignore_ascii_case(&token) => {
                amount = amount.saturating_add(abs);
            }
            Some(_) => {
                // The current `SwapAction` has one token_in and one token_out.
                // Multi-input/multi-output batch swaps are kept non-fatal by
                // preserving the first signed side rather than fabricating a
                // broader aggregate the Action model cannot express.
            }
        }
    }

    selected_token.map(|token| (token, amount))
}

fn balancer_json_u256(v: &serde_json::Value) -> Option<alloy_primitives::U256> {
    match v {
        serde_json::Value::String(s) => {
            alloy_primitives::U256::from_str_radix(s, 10)
                .ok()
                .or_else(|| {
                    s.strip_prefix("0x")
                        .and_then(|hex| alloy_primitives::U256::from_str_radix(hex, 16).ok())
                })
        }
        serde_json::Value::Number(n) => n.as_u64().map(alloy_primitives::U256::from),
        _ => None,
    }
}

fn balancer_signed_abs(v: &serde_json::Value) -> Option<(bool, alloy_primitives::U256)> {
    match v {
        serde_json::Value::String(s) => {
            if let Some(abs) = s.strip_prefix('-') {
                Some((true, alloy_primitives::U256::from_str_radix(abs, 10).ok()?))
            } else {
                Some((false, alloy_primitives::U256::from_str_radix(s, 10).ok()?))
            }
        }
        serde_json::Value::Number(n) => {
            let value = n.as_i64()?;
            let is_negative = value.is_negative();
            Some((
                is_negative,
                alloy_primitives::U256::from(value.unsigned_abs()),
            ))
        }
        _ => None,
    }
}

fn balancer_request_amounts(args_json: &serde_json::Value, index: usize) -> Vec<serde_json::Value> {
    args_json
        .get("request")
        .and_then(serde_json::Value::as_array)
        .and_then(|request| request.get(index))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn parse_balancer_v2_join_user_data(
    raw: &str,
    fallback_amounts: Vec<serde_json::Value>,
) -> Option<(Vec<serde_json::Value>, alloy_primitives::U256)> {
    let bytes = balancer_user_data_bytes(raw)?;
    let kind: u64 = balancer_word_u256(&bytes, 0)?.try_into().ok()?;
    match kind {
        // INIT: (kind, uint256[] amountsIn). No min-BPT floor in userData.
        0 => Some((
            balancer_u256_array_json(&bytes, balancer_word_usize(&bytes, 1)?)?,
            alloy_primitives::U256::ZERO,
        )),
        // EXACT_TOKENS_IN_FOR_BPT_OUT: (kind, amountsIn[], minBPTAmountOut).
        1 => Some((
            balancer_u256_array_json(&bytes, balancer_word_usize(&bytes, 1)?)?,
            balancer_word_u256(&bytes, 2)?,
        )),
        // TOKEN_IN_FOR_EXACT_BPT_OUT / ALL_TOKENS_IN_FOR_EXACT_BPT_OUT: the
        // request's maxAmountsIn[] is the available policy bound.
        2 | 3 => Some((fallback_amounts, balancer_word_u256(&bytes, 1)?)),
        _ => None,
    }
}

fn parse_balancer_v2_exit_user_data(raw: &str) -> Option<alloy_primitives::U256> {
    let bytes = balancer_user_data_bytes(raw)?;
    let kind: u64 = balancer_word_u256(&bytes, 0)?.try_into().ok()?;
    match kind {
        // EXACT_BPT_IN_FOR_ONE_TOKEN_OUT.
        0 => balancer_word_u256(&bytes, 1),
        // Weighted kind=1: EXACT_BPT_IN_FOR_TOKENS_OUT (kind, bptAmountIn).
        // Stable kind=1: BPT_IN_FOR_EXACT_TOKENS_OUT (kind, amountsOut[], maxBPTAmountIn).
        1 if bytes.len() == 64 => balancer_word_u256(&bytes, 1),
        1 => balancer_word_u256(&bytes, 2),
        // Weighted kind=2: BPT_IN_FOR_EXACT_TOKENS_OUT; Stable kind=2:
        // EXACT_BPT_IN_FOR_ALL_TOKENS_OUT.
        2 if bytes.len() == 64 => balancer_word_u256(&bytes, 1),
        2 => balancer_word_u256(&bytes, 2),
        // Recovery-mode exits are encoded as (255, bptAmountIn) in the pool
        // utility layer; preserve the burned BPT amount when present.
        255 if bytes.len() == 64 => balancer_word_u256(&bytes, 1),
        _ => None,
    }
}

fn balancer_user_data_bytes(raw: &str) -> Option<Vec<u8>> {
    let hex = raw.strip_prefix("0x")?;
    if hex.len() % 64 != 0 {
        return None;
    }
    hex::decode(hex).ok()
}

fn balancer_word_u256(bytes: &[u8], word_index: usize) -> Option<alloy_primitives::U256> {
    let start = word_index.checked_mul(32)?;
    let end = start.checked_add(32)?;
    if end > bytes.len() {
        return None;
    }
    Some(alloy_primitives::U256::from_be_slice(&bytes[start..end]))
}

fn balancer_word_usize(bytes: &[u8], word_index: usize) -> Option<usize> {
    let word = balancer_word_u256(bytes, word_index)?;
    word.try_into().ok()
}

fn balancer_u256_array_json(bytes: &[u8], offset: usize) -> Option<Vec<serde_json::Value>> {
    if offset % 32 != 0 {
        return None;
    }
    let base = offset / 32;
    let len = balancer_word_usize(bytes, base)?;
    let mut out = Vec::with_capacity(len);
    for idx in 0..len {
        out.push(serde_json::Value::String(
            balancer_word_u256(bytes, base + 1 + idx)?.to_string(),
        ));
    }
    Some(out)
}

/// `multicall_recurse` (Cat D) — flatten a self-`multicall(bytes[])` into one
/// [`v3_action::ActionBody::Multicall`].
///
/// `self_array_bytes_last_arg`: the inner sub-calls live in the single `bytes[]`
/// argument (SwapRouter02 `multicall(uint256 deadline, bytes[] data)` has a
/// leading non-array `deadline`; NFPM / V4 PositionManager `multicall(bytes[]
/// data)` has only the array). [`args_to_json`] renders that `bytes[]` as a JSON
/// array of `"0x.."` strings, so we pick the SOLE array-valued arg.
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

    // self_array_bytes_last_arg → exactly one array-valued argument (the
    // `bytes[]`). A `uint256 deadline` sibling renders as a decimal string, never
    // an array, so the array arg is unambiguous for every real shape.
    let array_args: Vec<&serde_json::Value> = args_json
        .as_object()
        .map(|obj| obj.values().filter(|v| v.is_array()).collect())
        .unwrap_or_default();
    let inner_calls = match array_args.as_slice() {
        [single] => single.as_array().expect("filtered is_array"),
        [] => {
            return Err(EngineErrorDto::new(
                "build_multicall_failed",
                "multicall_recurse: no bytes[] array argument".to_string(),
            ));
        }
        _ => {
            return Err(EngineErrorDto::new(
                "build_multicall_failed",
                "multicall_recurse: ambiguous (multiple array arguments)".to_string(),
            ));
        }
    };

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
