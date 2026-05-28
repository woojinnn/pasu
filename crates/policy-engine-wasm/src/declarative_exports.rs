//! `#[wasm_bindgen]` JSON-string exports for the declarative adapter pipeline.
//!
//! Phase 1A surface:
//!   * `declarative_install_json(bundle_json: String) -> String` —
//!     parses a bundle, constructs a [`DeclarativeMapper`], and stores it in a
//!     process-local registry keyed by the bundle's declarative decoder id.
//!     Returns the decoder id so the caller can record the
//!     `(chain_id, to, selector) → decoder_id` mapping bridge-side.
//!
//!   * `declarative_lookup_json(input_json: String) -> String` —
//!     resolves an installed mapper by decoder id and runs `Mapper::map`
//!     against a JSON-described `DecodedCall`. Returns the resulting
//!     `Vec<ActionEnvelope>`.
//!
//! Phase 6 additions:
//!   * Bridge table — install also expands `bundle.match.{chain_ids × to ×
//!     selector}` into a `(chain_id, to_lower, selector_lower) -> decoder_id`
//!     lookup, kept alongside the mapper registry in the same
//!     `DECLARATIVE_STATE`. Spec §5.5 calls this the "TS bridge"; we keep it
//!     WASM-side so a single Rust state owns both halves.
//!
//!   * `declarative_route_request_json(input_json: String) -> String` —
//!     orchestrator entry. Caller passes `(chain_id, to, selector, calldata,
//!     ctx)`; we look up the matching declarative decoder via the bridge,
//!     decode the raw calldata against the bundle's `abi_fragment.abi`
//!     (same pattern as `WasmChildResolver::resolve_child`), fall through to
//!     a miss when nothing matches, and otherwise run `mapper.map(ctx, decoded)`.
//!     Returns the same `{ ok, envelopes | error }` envelope as
//!     `declarative_lookup_json`.
//!
//! Wire shape (input/output) is documented inline next to each export. This
//! module forms the contract that the Phase 1B + Phase 6 TS bridges consume.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::subdecode::protocols::universal_router::{
    v3_position_manager_address, v4_position_manager_address, UNISWAP_UR_MASK,
};
use abi_resolver::{CallMatchKey, DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::{I256, U256};
use mappers::declarative::action_builder::{
    build_action_body, build_multicall_from_opcode_stream, UnknownOpcodePolicy as V3UnknownOpcodePolicy,
    V3MapContext,
};
use mappers::declarative::eval::args_to_json;
use mappers::declarative::multicall::extract_self_array_bytes;
use mappers::declarative::opcode_stream::{
    extract_ur_commands_and_inputs, DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
    DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER, DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER,
    DISPATCHER_ID_UNIVERSAL_ROUTER, DISPATCHER_ID_V4_POSITION_MANAGER,
};
use mappers::declarative::types::BundleMatch;
use mappers::declarative::{AdapterFunctionBundle, DeclarativeMapper, EmitRule};
use mappers::mapper::{ChildResolver, MapContext, Mapper, MapperError};
use mappers::token_registry::EmptyTokenRegistry;

// Cross-target opcodes for the Uniswap UR family (mirrored from
// `mappers::declarative::opcode_stream` where they are crate-private; kept
// here as the local single source of truth for the planner's cross-target
// child extraction).
const PLANNER_OPCODE_V3_PM_PERMIT: u8 = 0x11;
const PLANNER_OPCODE_V3_PM_CALL: u8 = 0x12;
const PLANNER_OPCODE_V4_PM_CALL: u8 = 0x14;
use policy_engine::action::{Address, DecimalString};
use wasm_bindgen::prelude::*;

use crate::dto::{
    DeclarativeChildCallKeyDto, DeclarativeInstallResultDto, DeclarativeLookupInputDto,
    DeclarativePlanChildrenResultDto, DeclarativeRouteRequestInputDto,
    DeclarativeRouteRequestResultDto, DeclarativeRouteRequestV3InputDto,
    DeclarativeRouteRequestV3ResultDto, DecodedArgDto, DecodedCallDto, DecodedValueDto,
    EngineErrorDto, Envelope,
};
use crate::exports::check_input_size;

// Phase 4B — v3 action tree imports. Kept namespaced under `v3_action` so the
// legacy `policy_engine::action` path stays readable for the v1 entries above.
use simulation_reducer::action as v3_action;
use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
use simulation_state::primitives::{
    Address as V3Address, ChainId as V3ChainId, Time as V3Time, U256 as V3U256,
};

/// Process-local state for the declarative pipeline. Two halves:
///
/// * `mappers` — installed [`DeclarativeMapper`]s keyed by their canonical
///   `declarative.<path>` decoder id. Spec §5.4.
///
/// * `bridge`  — `(chain_id, to_lowercase, selector_lowercase) -> decoder_id`
///   table. Populated at install time from `bundle.match.{chain_ids × to ×
///   selector}` so the orchestrator entry (§5.5 + §7.2) can resolve a raw tx
///   tuple to the right mapper without knowing the bundle ahead of time.
///
/// Both halves live in the same `RefCell` so a re-install is atomic from the
/// caller's perspective: we never serve a stale bridge entry pointing at a
/// mapper that has already been replaced.
struct DeclarativeState {
    mappers: HashMap<String, Arc<DeclarativeMapper>>,
    bridge: HashMap<BridgeKey, String>,
}

impl DeclarativeState {
    fn new() -> Self {
        Self {
            mappers: HashMap::new(),
            bridge: HashMap::new(),
        }
    }
}

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

thread_local! {
    /// Installed declarative mappers + bridge lookup. Single state instance
    /// per WASM module (one per SW lifetime in the extension).
    static DECLARATIVE_STATE: RefCell<DeclarativeState> = RefCell::new(DeclarativeState::new());
}

// ───────────────────────────────────────────────────────────────────────────
// M2 — v3 declarative state (parallel to v1 above)
// ───────────────────────────────────────────────────────────────────────────
//
// v3 (PDF FSM hierarchical `ActionBody`) flows through a separate state from
// the v1 envelope path so a re-install on one tier cannot disturb the other
// — `feat/registry-v2` keeps both running in parallel through cutover.
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
    /// same accessor v1 uses so the dual-schema (`chain_to_addresses` /
    /// `chain_ids × to`) split is invisible here.
    bridge: HashMap<BridgeKey, String>,
    /// `bundle_id` → raw manifest JSON. Stored as `serde_json::Value` (not
    /// the strongly-typed `AdapterFunctionBundle`) because the v3 templates
    /// (`emit.body`, `emit.live_inputs`, `emit.per_opcode_body`) are not
    /// modelled in `EmitRule` — the action_builder consumes them as-is.
    bundles: HashMap<String, serde_json::Value>,
}

thread_local! {
    /// v3 install table — separate from `DECLARATIVE_STATE` so a v3 install
    /// (re)install cannot disturb the v1 mapper + bridge halves. Single
    /// instance per WASM module lifetime (one per SW lifetime in the
    /// extension).
    static DECLARATIVE_V3_STATE: RefCell<DeclarativeV3State> = RefCell::new(DeclarativeV3State::default());
}

/// Expand the bundle's `match` entries into bridge entries. v2 schema
/// (`chain_to_addresses` map) and v1 legacy (`chain_ids × to` cartesian)
/// both flow through [`BundleMatch::entries`]. Existing entries for the
/// same callkey are replaced (re-install semantics).
fn register_bridge_entries(
    bundle: &AdapterFunctionBundle,
    decoder_id: &str,
    state: &mut DeclarativeState,
) {
    let selector = bundle.match_.selector.to_ascii_lowercase();
    for (chain_id, to) in bundle.match_.entries() {
        let key = BridgeKey {
            chain_id,
            to: to.to_ascii_lowercase(),
            selector: selector.clone(),
        };
        state.bridge.insert(key, decoder_id.to_string());
    }
}

/// WASM-side [`ChildResolver`] implementation. Looks each child
/// `CallMatchKey` up through the same `DECLARATIVE_STATE` bridge that
/// `declarative_route_request_json` consults at the top level, then decodes
/// the inner calldata against the bundle's `abi_fragment.abi` and recurses
/// into the bundle's `Mapper::map`.
///
/// The resolver is stateless — it re-acquires the (immutable) state borrow
/// on every `resolve_child` call. This avoids holding a long-lived borrow
/// across the top-level `mapper.map(...)` call, so a concurrent
/// `declarative_install_json` (which needs `borrow_mut`) cannot panic mid-
/// recursion. In practice WASM is single-threaded per service-worker, but
/// the shorter borrow window is also clearer about lifetimes.
///
/// Re-entrancy safety:
///   * The outer `declarative_route_request_json` clones the parent
///     `Arc<DeclarativeMapper>` *out* of the state before calling
///     `mapper.map(...)`, so the parent invocation does not require a live
///     `DECLARATIVE_STATE` borrow.
///   * `WasmChildResolver::resolve_child` opens a fresh
///     `DECLARATIVE_STATE.borrow()` and drops it before invoking the child's
///     `mapper.map(...)`. The child invocation may itself recurse — at which
///     point the same fresh-borrow pattern applies.
///   * `borrow_mut` is taken only by `declarative_install_json`, which
///     never runs while a `declarative_route_request_json` call is in
///     progress on the same thread. The fresh-borrow pattern therefore
///     can never observe a `BorrowMutError`.
struct WasmChildResolver;

impl ChildResolver for WasmChildResolver {
    fn resolve_child(
        &self,
        child: &CallMatchKey,
        ctx: &MapContext<'_>,
        child_calldata: &[u8],
    ) -> Result<Vec<policy_engine::ActionEnvelope>, MapperError> {
        // Build the bridge key from the child match key. Lowercase
        // normalisation matches the install-time `register_bridge_entries`
        // path. `policy_engine::action::Address` is constructed lowercase
        // (see `FromStr` impl), so `to_string()` already produces the form
        // the bridge expects — `to_ascii_lowercase` is belt-and-braces in
        // case the canonicalisation ever changes.
        let key = BridgeKey {
            chain_id: child.chain_id,
            to: child.to.to_string().to_ascii_lowercase(),
            selector: format!("0x{}", hex::encode(child.selector)).to_ascii_lowercase(),
        };

        // Snapshot (clone Arc) under a short immutable borrow so the
        // subsequent decode + map() does not hold the borrow open.
        let lookup = DECLARATIVE_STATE.with(|state_cell| {
            let state = state_cell.borrow();
            state.bridge.get(&key).and_then(|decoder_id| {
                state
                    .mappers
                    .get(decoder_id)
                    .cloned()
                    .map(|mapper| (decoder_id.clone(), mapper))
            })
        });

        let (decoder_id, mapper) = lookup.ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "WasmChildResolver: no declarative mapper bridged for \
                 chain_id={} to={} selector={}",
                key.chain_id,
                key.to,
                key.selector
            ))
        })?;

        // Decode the inner calldata against the matched bundle's ABI.
        let abi_json = &mapper.bundle().abi_fragment.abi;
        let mut decoded = abi_resolver::bridge::decode_with_json_abi(abi_json, child_calldata)
            .map_err(|error| {
                MapperError::Internal(anyhow::anyhow!(
                    "WasmChildResolver: decode inner calldata failed (decoder_id={}): {error}",
                    decoder_id
                ))
            })?;

        // `decode_with_json_abi` derives a decoder_id from the static
        // selector lookup table; overwrite it with the declarative one so
        // `DeclarativeMapper::accepts` (strict equality on
        // `declarative.<path>`) matches.
        decoded.decoder_id = DecoderId::new(decoder_id);

        mapper.map(ctx, &decoded)
    }
}

/// Install (or replace) a declarative adapter bundle.
///
/// Input JSON shape: the full bundle as per
/// `ADAPTER_LOADER_ARCHITECTURE.md` §4.1 (see
/// `crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json`).
///
/// Output:
/// ```json
/// { "ok": true, "data": { "decoder_id": "declarative.<path>", "bundle_id": "<id>@<ver>" } }
/// ```
/// or `{ "ok": false, "error": { "kind": "...", "message": "..." } }`.
#[wasm_bindgen]
pub fn declarative_install_json(bundle_json: String) -> String {
    let result = (|| -> Result<DeclarativeInstallResultDto, EngineErrorDto> {
        check_input_size(&bundle_json, "declarative_install_json")?;
        let bundle: AdapterFunctionBundle =
            serde_json::from_str(&bundle_json).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_bundle_json",
                    format!("invalid bundle json: {error}"),
                )
            })?;
        let mapper = DeclarativeMapper::new(bundle.clone());
        let decoder_id = mapper.declarative_decoder_id().as_str().to_owned();
        let bundle_id = bundle.id.clone();
        DECLARATIVE_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.mappers.insert(decoder_id.clone(), Arc::new(mapper));
            // Phase 6 — populate the bridge table so the orchestrator entry
            // (`declarative_route_request_json`) can route a raw
            // `(chain_id, to, selector)` tuple to the installed mapper
            // without the caller having to know the decoder_id.
            register_bridge_entries(&bundle, &decoder_id, &mut state);
        });
        Ok(DeclarativeInstallResultDto {
            decoder_id,
            bundle_id,
        })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
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
            .ok_or_else(|| EngineErrorDto::new("missing_id", "bundle.id missing or not a string".to_string()))?
            .to_owned();

        let match_value = bundle_value
            .get("match")
            .ok_or_else(|| EngineErrorDto::new("invalid_match", "bundle.match missing".to_string()))?;
        let bundle_match: BundleMatch =
            serde_json::from_value(match_value.clone()).map_err(|error| {
                EngineErrorDto::new("invalid_match", format!("bundle.match parse failed: {error}"))
            })?;

        let selector = bundle_match.selector.to_ascii_lowercase();

        DECLARATIVE_V3_STATE.with(|state| {
            let mut state = state.borrow_mut();
            for (chain_id, to) in bundle_match.entries() {
                let key = BridgeKey {
                    chain_id,
                    to: to.to_ascii_lowercase(),
                    selector: selector.clone(),
                };
                state.bridge.insert(key, bundle_id.clone());
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

/// Run an installed declarative mapper against a JSON-described `DecodedCall`.
///
/// Input JSON shape (see `DeclarativeLookupInputDto`):
/// ```json
/// {
///   "decoder_id": "declarative.uniswap/v2/swapExactTokensForTokens",
///   "ctx": {
///     "chain_id": 1,
///     "from": "0x..",
///     "to":   "0x..",
///     "value_wei": "0",            // optional, default "0"
///     "block_timestamp": 1700000000 // optional
///   },
///   "decoded": {
///     "decoder_id": "declarative.uniswap/v2/swapExactTokensForTokens",
///     "function_signature": "...",
///     "args": [
///       { "name": "amountIn", "abi_type": "uint256",
///         "value": { "kind": "uint", "value": "1000000000000000000" } },
///       ...
///       { "name": "path", "abi_type": "address[]",
///         "value": { "kind": "array",
///                    "value": [ { "kind": "address", "value": "0x.." }, ... ] } }
///     ]
///   }
/// }
/// ```
///
/// Output: `{ "ok": true, "data": { "envelopes": [...] } }` where `envelopes`
/// is the JSON-serialised `Vec<ActionEnvelope>`.
#[wasm_bindgen]
pub fn declarative_lookup_json(input_json: String) -> String {
    let result = (|| -> Result<DeclarativeLookupResultDto, EngineErrorDto> {
        check_input_size(&input_json, "declarative_lookup_json")?;
        let input: DeclarativeLookupInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let mapper =
            DECLARATIVE_STATE.with(|state| state.borrow().mappers.get(&input.decoder_id).cloned());
        let mapper = mapper.ok_or_else(|| {
            EngineErrorDto::new(
                "decoder_id_not_installed",
                format!(
                    "no declarative mapper installed for decoder_id {:?}",
                    input.decoder_id
                ),
            )
        })?;

        let from = Address::from_str(&input.ctx.from).map_err(|message| {
            EngineErrorDto::new("invalid_from", format!("invalid ctx.from: {message}"))
        })?;
        let to = Address::from_str(&input.ctx.to).map_err(|message| {
            EngineErrorDto::new("invalid_to", format!("invalid ctx.to: {message}"))
        })?;
        let value_wei = input.ctx.value_wei.as_deref().unwrap_or("0");
        let value = DecimalString::from_str(value_wei).map_err(|message| {
            EngineErrorDto::new(
                "invalid_value_wei",
                format!("invalid ctx.value_wei: {message}"),
            )
        })?;
        let block_timestamp = input.ctx.block_timestamp;

        let decoded = decoded_call_from_dto(input.decoded)?;

        let registry = EmptyTokenRegistry;
        // PoC scope: WASM-side `multicall_recurse` e2e is deferred (spec §0).
        // Rust-side unit tests cover the strategy via `ChildResolver` mocks.
        // We leave `resolver: None` here — a bundle that requires recursion
        // will surface `multicall_recurse requires ctx.resolver` and the host
        // can decide whether to add WASM-side recursion later. The remaining
        // single_emit bundles (V2/V3/SR02) are unaffected.
        let ctx = MapContext {
            chain_id: input.ctx.chain_id,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp,
            token_registry: &registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        };

        let envelopes = mapper
            .map(&ctx, &decoded)
            .map_err(|error| EngineErrorDto::new("map_failed", error.to_string()))?;
        Ok(DeclarativeLookupResultDto { envelopes })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct DeclarativeLookupResultDto {
    pub envelopes: Vec<policy_engine::ActionEnvelope>,
}

/// Phase 6 — orchestrator entry.
///
/// Resolves `(chain_id, to, selector)` through the bridge table populated at
/// install time, then runs the matching `DeclarativeMapper` against the
/// caller-provided `decoded` call. Functionally `declarative_lookup_json`
/// composed with a bridge lookup; surfaced separately so the orchestrator
/// can stay agnostic of `decoder_id` minting (the bundle owns that).
///
/// Input JSON shape (see `DeclarativeRouteRequestInputDto`):
/// ```json
/// {
///   "chain_id": 1,
///   "to":       "0x7a25...",
///   "selector": "0x38ed1739",
///   "ctx": {
///     "chain_id": 1,
///     "from": "0x..",
///     "to":   "0x..",
///     "value_wei": "0",
///     "block_timestamp": 1700000000
///   },
///   "calldata": "0x38ed1739..."   // raw "0x"-prefixed calldata; WASM decodes internally
/// }
/// ```
///
/// Output:
/// ```json
/// { "ok": true, "data": { "envelopes": [...], "decoder_id": "declarative.<path>" } }
/// ```
/// or `{ "ok": false, "error": { "kind": "no_declarative_mapper" | "decoder_id_not_installed" | ..., "message": "..." } }`.
///
/// Miss semantics: when no bridge entry exists, we return
/// `EngineErrorDto { kind: "no_declarative_mapper", ... }`. The orchestrator
/// uses this as the "fall through to static path" signal — it does NOT
/// indicate engine failure.
#[wasm_bindgen]
pub fn declarative_route_request_json(input_json: String) -> String {
    let result = (|| -> Result<DeclarativeRouteRequestResultDto, EngineErrorDto> {
        check_input_size(&input_json, "declarative_route_request_json")?;
        let input: DeclarativeRouteRequestInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let key = BridgeKey {
            chain_id: input.chain_id,
            to: input.to.to_ascii_lowercase(),
            selector: input.selector.to_ascii_lowercase(),
        };

        // Single lock so the bridge → mapper lookup is atomic with any
        // concurrent install (re-installs replace both halves of the state
        // inside one borrow_mut).
        let mapper_with_id = DECLARATIVE_STATE.with(|state| {
            let state = state.borrow();
            state.bridge.get(&key).and_then(|decoder_id| {
                state
                    .mappers
                    .get(decoder_id)
                    .cloned()
                    .map(|mapper| (decoder_id.clone(), mapper))
            })
        });
        let (decoder_id, mapper) = mapper_with_id.ok_or_else(|| {
            EngineErrorDto::new(
                "no_declarative_mapper",
                format!(
                    "no declarative mapper bridged for chain_id={} to={} selector={}",
                    input.chain_id, input.to, input.selector
                ),
            )
        })?;

        let from = Address::from_str(&input.ctx.from).map_err(|message| {
            EngineErrorDto::new("invalid_from", format!("invalid ctx.from: {message}"))
        })?;
        let to = Address::from_str(&input.ctx.to).map_err(|message| {
            EngineErrorDto::new("invalid_to", format!("invalid ctx.to: {message}"))
        })?;
        let value_wei = input.ctx.value_wei.as_deref().unwrap_or("0");
        let value = DecimalString::from_str(value_wei).map_err(|message| {
            EngineErrorDto::new(
                "invalid_value_wei",
                format!("invalid ctx.value_wei: {message}"),
            )
        })?;
        let block_timestamp = input.ctx.block_timestamp;

        let calldata_hex = input.calldata.strip_prefix("0x").unwrap_or(&input.calldata);
        let calldata_bytes = hex::decode(calldata_hex).map_err(|error| {
            EngineErrorDto::new(
                "invalid_calldata",
                format!("calldata is not valid hex: {error}"),
            )
        })?;
        let abi_json = &mapper.bundle().abi_fragment.abi;
        let mut decoded = abi_resolver::bridge::decode_with_json_abi(abi_json, &calldata_bytes)
            .map_err(|error| {
                EngineErrorDto::new("decode_failed", format!("calldata decode failed: {error}"))
            })?;
        decoded.decoder_id = DecoderId::new(decoder_id.clone());

        let registry = EmptyTokenRegistry;
        // Phase 7 T-B4 — wire the WASM-side ChildResolver so
        // `multicall_recurse` bundles (V3 NFPM `multicall(bytes[])`, SR02
        // multicall overloads, Multicall3 …) can dispatch each inner
        // sub-call back through this entry. Static (`single_emit`,
        // `opcode_stream_dispatch`) bundles ignore `ctx.resolver`, so this
        // change does not affect existing PoC paths.
        let resolver = WasmChildResolver;
        let ctx = MapContext {
            chain_id: input.ctx.chain_id,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp,
            token_registry: &registry,
            parent_calldata: None,
            depth: 0,
            resolver: Some(&resolver),
        };

        let envelopes = mapper
            .map(&ctx, &decoded)
            .map_err(|error| EngineErrorDto::new("map_failed", error.to_string()))?;

        Ok(DeclarativeRouteRequestResultDto {
            envelopes,
            decoder_id,
        })
    })();

    match result {
        Ok(dto) => Envelope::ok(dto).to_json(),
        Err(error) => Envelope::<()>::err(error.kind, error.message).to_json(),
    }
}

/// `multicall_recurse` child-callkey planner (child-prefetch support).
///
/// The WASM-side `WasmChildResolver` is synchronous and can only resolve a
/// child sub-call if the child bundle is already mounted in `DECLARATIVE_STATE`
/// — it cannot fetch. This export lets the TS host run a fetch+install pass
/// *before* `declarative_route_request_json`:
/// 1. TS calls this with the outer tx tuple (same input shape as
///    `declarative_route_request_json`).
/// 2. We resolve the outer bundle via the bridge, confirm it is a
///    `multicall_recurse` bundle, decode the outer calldata against the
///    bundle ABI, and pull the inner `bytes[]` via `extract_self_array_bytes`.
/// 3. We return one `(chain_id, to, selector)` callkey per child.
/// 4. TS `resolveAdapter`s each child, then calls
///    `declarative_route_request_json` — the resolver now finds every child.
///
/// Depth-1 only: NFPM children (`mint`, `refundETH`) are leaf `single_emit`
/// functions, so one prefetch level suffices. Nested multicalls are a follow-up.
///
/// Miss / non-recurse: returns `ok:true` with `children: []` when no outer
/// bridge entry exists or the outer bundle is not `multicall_recurse`. A
/// malformed input returns `ok:false`; the TS caller treats a planner fault as
/// best-effort and proceeds to `declarative_route_request_json` regardless.
#[wasm_bindgen]
pub fn declarative_plan_children_json(input_json: String) -> String {
    let result = (|| -> Result<DeclarativePlanChildrenResultDto, EngineErrorDto> {
        check_input_size(&input_json, "declarative_plan_children_json")?;
        let input: DeclarativeRouteRequestInputDto =
            serde_json::from_str(&input_json).map_err(|error| {
                EngineErrorDto::new("invalid_input_json", format!("invalid input json: {error}"))
            })?;

        let key = BridgeKey {
            chain_id: input.chain_id,
            to: input.to.to_ascii_lowercase(),
            selector: input.selector.to_ascii_lowercase(),
        };

        // A bridge miss is not an error — return an empty child list so the TS
        // caller skips the prefetch; `declarative_route_request_json` then
        // produces the real miss.
        let lookup = DECLARATIVE_STATE.with(|state| {
            let state = state.borrow();
            state.bridge.get(&key).and_then(|decoder_id| {
                state
                    .mappers
                    .get(decoder_id)
                    .cloned()
                    .map(|mapper| (decoder_id.clone(), mapper))
            })
        });
        let (decoder_id, mapper) = match lookup {
            Some(pair) => pair,
            None => {
                return Ok(DeclarativePlanChildrenResultDto {
                    children: Vec::new(),
                    decoder_id: String::new(),
                })
            }
        };

        // Strategy 별 child extraction:
        //   - MulticallRecurse → self-multicall (child to == outer to)
        //   - OpcodeStreamDispatch (UR family) → cross-target (V3/V4 PM)
        //   - 그 외 (single_emit, enum_tagged, array_emit, V4 PM dispatcher) → 없음
        let needs_decode = matches!(
            mapper.bundle().emit,
            EmitRule::MulticallRecurse { .. } | EmitRule::OpcodeStreamDispatch { .. }
        );
        if !needs_decode {
            return Ok(DeclarativePlanChildrenResultDto {
                children: Vec::new(),
                decoder_id,
            });
        }

        // Decode the outer calldata against the bundle ABI — same pattern as
        // `declarative_route_request_json` / `WasmChildResolver`.
        let calldata_hex = input.calldata.strip_prefix("0x").unwrap_or(&input.calldata);
        let calldata_bytes = hex::decode(calldata_hex).map_err(|error| {
            EngineErrorDto::new(
                "invalid_calldata",
                format!("calldata is not valid hex: {error}"),
            )
        })?;
        let abi_json = &mapper.bundle().abi_fragment.abi;
        let decoded = abi_resolver::bridge::decode_with_json_abi(abi_json, &calldata_bytes)
            .map_err(|error| {
                EngineErrorDto::new("decode_failed", format!("calldata decode failed: {error}"))
            })?;

        let children = match &mapper.bundle().emit {
            EmitRule::MulticallRecurse { .. } => {
                // Existing path — self-multicall (child to == outer to).
                let child_calldatas = extract_self_array_bytes(&decoded).map_err(|error| {
                    EngineErrorDto::new(
                        "decode_failed",
                        format!("multicall child extraction failed: {error}"),
                    )
                })?;
                let mut children = Vec::with_capacity(child_calldatas.len());
                for (index, child) in child_calldatas.iter().enumerate() {
                    if child.len() < 4 {
                        return Err(EngineErrorDto::new(
                            "decode_failed",
                            format!(
                                "multicall child #{index} calldata shorter than 4 bytes (len={})",
                                child.len()
                            ),
                        ));
                    }
                    children.push(DeclarativeChildCallKeyDto {
                        chain_id: input.chain_id,
                        to: input.to.to_ascii_lowercase(),
                        selector: format!("0x{}", hex::encode(&child[..4])),
                    });
                }
                children
            }
            EmitRule::OpcodeStreamDispatch { dispatcher_id, .. } => {
                // Track B Fix 3a — UR family 의 cross-target opcode (0x11/0x12/0x14) 의
                // inner calldata 를 추출 후 per-chain V3 NPM / V4 PM 의 callkey 로 변환.
                // V4 PM dispatcher 는 internal action stream 만 가지므로 cross-target 없음.
                match dispatcher_id.as_str() {
                    DISPATCHER_ID_UNIVERSAL_ROUTER | DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER => {
                        extract_ur_cross_target_children(&decoded, input.chain_id)?
                    }
                    DISPATCHER_ID_V4_POSITION_MANAGER => {
                        // V4 PM action stream — V4_ROUTER_TABLE 내부 처리, cross-target 없음.
                        Vec::new()
                    }
                    DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER => {
                        // Pancake UR 0x11/0x12 는 Commands.sol placeholder (revert).
                        // 0x13/0x14 INFI_*_INITIALIZE_POOL 은 self-stored immutable
                        // pool manager 로 forward — cross-target callkey 없음.
                        // 따라서 planner 가 prefetch 할 child callkey 부재.
                        Vec::new()
                    }
                    DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER => {
                        // Pancake Infinity PositionManager — flat opcode set
                        // (PANCAKE_INFI_TABLE), recursive / cross-table action
                        // 미존재. cross-target callkey 없음.
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };

        Ok(DeclarativePlanChildrenResultDto {
            children,
            decoder_id,
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
// v3 route entry. Same callkey lookup pattern as
// `declarative_route_request_json` (v1), but emits the new hierarchical
// `simulation_reducer::action::Action` tree (PDF FSM spec) instead of the flat
// `policy_engine::ActionEnvelope`.
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
        let input: DeclarativeRouteRequestV3InputDto = serde_json::from_str(&input_json)
            .map_err(|error| {
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
        // `resolved` / `derived` are empty `BTreeMap`s — the Sync
        // orchestrator that fills them is M5+. Manifests that reference
        // `$resolved.<k>` or `$derived.<k>` therefore surface a precise
        // `unresolved_placeholder` error at this stage, which is the
        // intended observable behaviour while the resolver layer is wired.
        let key = BridgeKey {
            chain_id: input.chain_id,
            to: input.to.to_ascii_lowercase(),
            selector: input.selector.to_ascii_lowercase(),
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
                        "no v3 mapper bridged for chain_id={} to={} selector={}",
                        input.chain_id, input.to, input.selector
                    ),
                )
            })?;

        // Decode calldata against the manifest ABI (same pattern as v1).
        let calldata_hex = input.calldata.strip_prefix("0x").unwrap_or(&input.calldata);
        let calldata_bytes = hex::decode(calldata_hex).map_err(|error| {
            EngineErrorDto::new(
                "invalid_calldata",
                format!("calldata is not valid hex: {error}"),
            )
        })?;
        let abi_json = bundle_value.pointer("/abi_fragment/abi").ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing abi_fragment.abi".to_string())
        })?;
        let decoded = abi_resolver::bridge::decode_with_json_abi(abi_json, &calldata_bytes)
            .map_err(|error| {
                EngineErrorDto::new("decode_failed", format!("calldata decode failed: {error}"))
            })?;
        let args_json = args_to_json(&decoded);

        let emit = bundle_value.get("emit").ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit".to_string())
        })?;
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

        let ctx = V3MapContext {
            chain: chain.clone(),
            tx_to: target,
            tx_from: submitter,
            value,
            submitted_at,
            args_json: &args_json,
            resolved,
            derived: BTreeMap::new(),
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
                    emit.get("mask").and_then(serde_json::Value::as_str).unwrap_or("0xff"),
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

                let commands_str = args_json
                    .get("commands")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        EngineErrorDto::new("invalid_args", "missing args.commands".to_string())
                    })?;
                let commands_bytes = hex::decode(
                    commands_str.strip_prefix("0x").unwrap_or(commands_str),
                )
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
                        EngineErrorDto::new("invalid_args", "missing args.inputs".to_string())
                    })?;

                // Per-opcode `inputs_abi` ABI-decode pass. The v3 manifest
                // attaches the Solidity tuple signature next to each
                // opcode's `body` template (e.g. UR V3_SWAP_EXACT_IN:
                // `(address recipient, uint256 amountIn, uint256
                // amountOutMin, bytes path, bool payerIsUser)`). Decoding
                // here yields a JSON value the action_builder's
                // `$inputs.<path>` placeholder walker can consume.
                //
                // M2 narrow scope: we go through abi-resolver's existing
                // `decode_with_signature` helper rather than re-implementing
                // the alloy `Function::parse` + `abi_decode_input` chain.
                // The helper expects a 4-byte selector prefix — we prepend a
                // synthetic zero selector (`0x00000000`) since the opcode
                // dispatch already established the function shape via the
                // manifest. Best-effort: missing `inputs_abi`, parse
                // failure, or decode failure all degrade to `Value::Null`,
                // which the action_builder's `$inputs.<x>` walker surfaces
                // as a clear `UnresolvedPlaceholder` error rather than a
                // silent bogus default — that is the M5 manual-e2e
                // contract.
                let mut decoded_inputs_array = Vec::with_capacity(inputs_array.len());
                for (i, input_hex) in inputs_array.iter().enumerate() {
                    let input_hex_str = input_hex.as_str().ok_or_else(|| {
                        EngineErrorDto::new(
                            "invalid_inputs",
                            format!("inputs[{i}] not string"),
                        )
                    })?;
                    let input_bytes = hex::decode(
                        input_hex_str.strip_prefix("0x").unwrap_or(input_hex_str),
                    )
                    .map_err(|error| {
                        EngineErrorDto::new(
                            "invalid_inputs_hex",
                            format!("inputs[{i}]: {error}"),
                        )
                    })?;

                    let opcode_byte = *commands_bytes.get(i).ok_or_else(|| {
                        EngineErrorDto::new(
                            "invalid_commands",
                            format!("commands shorter than inputs at {i}"),
                        )
                    })?;
                    let opcode_id = opcode_byte & mask;
                    let opcode_key = format!("0x{opcode_id:02x}");

                    let decoded_input = per_opcode_body
                        .get(&opcode_key)
                        .and_then(|entry| entry.get("inputs_abi"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|sig| decode_inputs_abi_tuple(sig, &input_bytes).ok())
                        .unwrap_or(serde_json::Value::Null);
                    decoded_inputs_array.push(decoded_input);
                }

                build_multicall_from_opcode_stream(
                    &ctx,
                    per_opcode_body,
                    &commands_bytes,
                    &decoded_inputs_array,
                    mask,
                    allow_revert_bit,
                    unknown_policy,
                )
                .map_err(|error| {
                    EngineErrorDto::new("build_multicall_failed", error.to_string())
                })?
            }
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
    let function = Function::parse(&synthetic)
        .map_err(|error| format!("parse {inputs_abi:?}: {error}"))?;
    let selector = function.selector().0;

    let mut prefixed = Vec::with_capacity(4 + input_bytes.len());
    prefixed.extend_from_slice(&selector);
    prefixed.extend_from_slice(input_bytes);

    let decoded = abi_resolver::decode::decode_with_function(&function, &prefixed)
        .map_err(|error| format!("decode {inputs_abi:?}: {error}"))?;

    let mut obj = serde_json::Map::with_capacity(decoded.args.len());
    for arg in &decoded.args {
        let decoded_value = abi_resolver::bridge::convert_value(arg.value.clone())
            .map_err(|error| format!("convert {inputs_abi:?}.{}: {error}", arg.name))?;
        obj.insert(
            arg.name.clone(),
            mappers::declarative::eval::decoded_value_to_json(&decoded_value),
        );
    }
    Ok(serde_json::Value::Object(obj))
}

// ───────────────────────────────────────────────────────────────────────────
// Track B Fix 3a — UR cross-target child callkey extraction
// ───────────────────────────────────────────────────────────────────────────

/// Extract cross-target child callkeys from a UR `execute` outer call.
///
/// Mirrors `opcode_stream::execute_position_manager_step` but stops at the
/// (chain_id, to, selector) tuple — does not invoke any inner mapper. Used
/// by the planner so the TS host can fetch+install each child bundle before
/// `declarative_route_request_json` runs and the WasmChildResolver tries to
/// resolve them.
///
/// Returns empty when:
///   * the outer args don't structurally match UR `execute` (commands + inputs)
///   * none of the commands are cross-target opcodes (V3/V4 PM)
///   * the chain has no V3/V4 PM address registered for the encountered opcode
fn extract_ur_cross_target_children(
    decoded: &DecodedCall,
    chain_id: u64,
) -> Result<Vec<DeclarativeChildCallKeyDto>, EngineErrorDto> {
    let extracted = extract_ur_commands_and_inputs(decoded).map_err(|error| {
        EngineErrorDto::new(
            "decode_failed",
            format!("UR commands/inputs extraction failed: {error}"),
        )
    })?;
    let Some((commands, inputs)) = extracted else {
        // Non-UR outer call or ABI shape mismatch — planner is best-effort.
        return Ok(Vec::new());
    };
    let mut children = Vec::new();
    for (i, &cmd) in commands.iter().enumerate() {
        let masked = cmd & UNISWAP_UR_MASK;
        let pm_addr = match masked {
            PLANNER_OPCODE_V3_PM_PERMIT | PLANNER_OPCODE_V3_PM_CALL => {
                v3_position_manager_address(chain_id)
            }
            PLANNER_OPCODE_V4_PM_CALL => v4_position_manager_address(chain_id),
            _ => continue,
        };
        let Some(addr) = pm_addr else {
            // Chain has no PM registered for this opcode — the cross-target Tx
            // would itself revert on-chain, but the planner just skips so the
            // route stays best-effort. WasmChildResolver still surfaces a
            // precise error if this path is actually reached.
            continue;
        };
        let Some(inner) = inputs.get(i) else {
            // commands.len() > inputs.len() — Tier B would also reject. Skip.
            continue;
        };
        if inner.len() < 4 {
            return Err(EngineErrorDto::new(
                "decode_failed",
                format!(
                    "UR cross-target step {i} (opcode {masked:#04x}) inner calldata \
                     shorter than 4 bytes (len={})",
                    inner.len()
                ),
            ));
        }
        children.push(DeclarativeChildCallKeyDto {
            chain_id,
            to: format!("0x{}", hex::encode(addr)),
            selector: format!("0x{}", hex::encode(&inner[..4])),
        });
    }
    Ok(children)
}

// ───────────────────────────────────────────────────────────────────────────
// DecodedCallDto → DecodedCall conversion
// ───────────────────────────────────────────────────────────────────────────

fn decoded_call_from_dto(dto: DecodedCallDto) -> Result<DecodedCall, EngineErrorDto> {
    let args = dto
        .args
        .into_iter()
        .map(decoded_arg_from_dto)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DecodedCall {
        decoder_id: DecoderId::new(dto.decoder_id),
        function_signature: dto.function_signature,
        args,
        nested: vec![],
    })
}

fn decoded_arg_from_dto(dto: DecodedArgDto) -> Result<DecodedArg, EngineErrorDto> {
    let value = decoded_value_from_dto(dto.value)?;
    Ok(DecodedArg {
        name: dto.name,
        abi_type: dto.abi_type,
        value,
    })
}

fn decoded_value_from_dto(dto: DecodedValueDto) -> Result<DecodedValue, EngineErrorDto> {
    match dto {
        DecodedValueDto::Address(raw) => {
            let address = Address::from_str(&raw).map_err(|message| {
                EngineErrorDto::new(
                    "invalid_decoded_value",
                    format!("invalid address {raw:?}: {message}"),
                )
            })?;
            Ok(DecodedValue::Address(address))
        }
        DecodedValueDto::Uint(raw) => {
            let value = U256::from_str_radix(&raw, 10).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_decoded_value",
                    format!("invalid uint {raw:?}: {error}"),
                )
            })?;
            Ok(DecodedValue::Uint(value))
        }
        DecodedValueDto::Int(raw) => {
            let value = I256::from_str(&raw).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_decoded_value",
                    format!("invalid int {raw:?}: {error}"),
                )
            })?;
            Ok(DecodedValue::Int(value))
        }
        DecodedValueDto::Bool(value) => Ok(DecodedValue::Bool(value)),
        DecodedValueDto::Bytes(raw) => {
            let hex_part = raw.strip_prefix("0x").unwrap_or(&raw);
            let bytes = hex::decode(hex_part).map_err(|error| {
                EngineErrorDto::new(
                    "invalid_decoded_value",
                    format!("invalid bytes {raw:?}: {error}"),
                )
            })?;
            Ok(DecodedValue::Bytes(bytes))
        }
        DecodedValueDto::String(value) => Ok(DecodedValue::String(value)),
        DecodedValueDto::Array(items) => {
            let inner = items
                .into_iter()
                .map(decoded_value_from_dto)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DecodedValue::Array(inner))
        }
        DecodedValueDto::Tuple(items) => {
            let inner = items
                .into_iter()
                .map(decoded_value_from_dto)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DecodedValue::Tuple(inner))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const V2_BUNDLE_JSON: &str =
        include_str!("../../adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json");

    fn install() -> Value {
        let out = declarative_install_json(V2_BUNDLE_JSON.to_owned());
        serde_json::from_str::<Value>(&out).unwrap()
    }

    #[test]
    fn install_returns_decoder_id() {
        let parsed = install();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["decoder_id"],
            "declarative.uniswap/v2/swapExactTokensForTokens"
        );
        assert_eq!(
            parsed["data"]["bundle_id"],
            "uniswap/v2/swapExactTokensForTokens@1.0.0"
        );
    }

    #[test]
    fn install_rejects_invalid_json() {
        let out = declarative_install_json("{not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_bundle_json");
    }

    fn v2_lookup_input() -> Value {
        json!({
            "decoder_id": "declarative.uniswap/v2/swapExactTokensForTokens",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "decoded": {
                "decoder_id": "declarative.uniswap/v2/swapExactTokensForTokens",
                "function_signature":
                    "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)",
                "args": [
                    { "name": "amountIn",     "abi_type": "uint256",
                      "value": { "kind": "uint", "value": "1000000000000000000" } },
                    { "name": "amountOutMin", "abi_type": "uint256",
                      "value": { "kind": "uint", "value": "1900000" } },
                    { "name": "path",         "abi_type": "address[]",
                      "value": { "kind": "array", "value": [
                          { "kind": "address",
                            "value": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
                          { "kind": "address",
                            "value": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2" }
                      ] } },
                    { "name": "to",           "abi_type": "address",
                      "value": { "kind": "address",
                                 "value": "0x4444444444444444444444444444444444444444" } },
                    { "name": "deadline",     "abi_type": "uint256",
                      "value": { "kind": "uint", "value": "1700000900" } }
                ]
            }
        })
    }

    #[test]
    fn lookup_returns_swap_envelope_after_install() {
        install();
        let out = declarative_lookup_json(v2_lookup_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");

        let envelopes = parsed["data"]["envelopes"].as_array().expect("array");
        assert_eq!(envelopes.len(), 1);
        let env = &envelopes[0];
        assert_eq!(env["category"], "dex");
        assert_eq!(env["action"], "swap");
        assert_eq!(env["fields"]["swapMode"], "exact_in");
        assert_eq!(
            env["fields"]["inputToken"]["asset"]["address"],
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
        assert_eq!(env["fields"]["inputToken"]["amount"]["kind"], "exact");
        assert_eq!(
            env["fields"]["inputToken"]["amount"]["value"],
            "1000000000000000000"
        );
        assert_eq!(
            env["fields"]["outputToken"]["asset"]["address"],
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
        assert_eq!(env["fields"]["outputToken"]["amount"]["kind"], "min");
        assert_eq!(env["fields"]["outputToken"]["amount"]["value"], "1900000");
        assert_eq!(
            env["fields"]["recipient"],
            "0x4444444444444444444444444444444444444444"
        );
        assert_eq!(env["fields"]["validity"]["source"], "tx-deadline");
        assert_eq!(env["fields"]["validity"]["expiresAt"], "1700000900");
    }

    #[test]
    fn lookup_unknown_decoder_errors() {
        let input = json!({
            "decoder_id": "declarative.unknown/x",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x000000000000000000000000000000000000bbbb"
            },
            "decoded": {
                "decoder_id": "declarative.unknown/x",
                "function_signature": "",
                "args": []
            }
        });
        let out = declarative_lookup_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "decoder_id_not_installed");
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 6 — declarative_route_request_json + bridge layer
    // ──────────────────────────────────────────────────────────────────────

    /// Encode ABI calldata: 4-byte selector + ABI-encoded params.
    ///
    /// Dynamic types (address[], bytes[], …) are encoded correctly by
    /// `alloy_dyn_abi` — never hand-type dynamic ABI hex.
    fn encode_calldata(selector: &str, args: &[alloy_dyn_abi::DynSolValue]) -> String {
        let sel = hex::decode(selector.trim_start_matches("0x")).unwrap();
        let body = alloy_dyn_abi::DynSolValue::Tuple(args.to_vec()).abi_encode_params();
        format!("0x{}{}", hex::encode(sel), hex::encode(body))
    }

    fn v2_route_input() -> Value {
        use alloy_dyn_abi::DynSolValue;
        use alloy_primitives::{Address as AlloyAddress, U256 as AlloyU256};
        let calldata = encode_calldata(
            "0x38ed1739",
            &[
                DynSolValue::Uint(AlloyU256::from(1_000_000_000_000_000_000u128), 256),
                DynSolValue::Uint(AlloyU256::from(1_900_000u64), 256),
                DynSolValue::Array(vec![
                    DynSolValue::Address(
                        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                            .parse::<AlloyAddress>()
                            .unwrap(),
                    ),
                    DynSolValue::Address(
                        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                            .parse::<AlloyAddress>()
                            .unwrap(),
                    ),
                ]),
                DynSolValue::Address(
                    "0x4444444444444444444444444444444444444444"
                        .parse::<AlloyAddress>()
                        .unwrap(),
                ),
                DynSolValue::Uint(AlloyU256::from(1_700_000_900u64), 256),
            ],
        );
        json!({
            "chain_id": 1,
            "to":       "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
            "selector": "0x38ed1739",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        })
    }

    #[test]
    fn route_request_misses_before_install() {
        // No install — bridge is empty, route MUST miss.
        let out = declarative_route_request_json(v2_route_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "no_declarative_mapper");
    }

    #[test]
    fn route_request_resolves_via_bridge_after_install() {
        install();
        let out = declarative_route_request_json(v2_route_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");

        assert_eq!(
            parsed["data"]["decoder_id"],
            "declarative.uniswap/v2/swapExactTokensForTokens"
        );
        let envelopes = parsed["data"]["envelopes"].as_array().expect("array");
        assert_eq!(envelopes.len(), 1);
        let env = &envelopes[0];
        assert_eq!(env["category"], "dex");
        assert_eq!(env["action"], "swap");
        assert_eq!(env["fields"]["swapMode"], "exact_in");
    }

    #[test]
    fn route_request_is_case_insensitive_on_to_and_selector() {
        install();
        // Same bundle, but caller supplies upper-case `to` (checksummed) and
        // selector. The bridge must normalise both sides.
        let mut input = v2_route_input();
        input["to"] = json!("0x7A250D5630B4cF539739dF2C5dAcb4c659F2488D");
        input["selector"] = json!("0x38ED1739");
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["decoder_id"],
            "declarative.uniswap/v2/swapExactTokensForTokens"
        );
    }

    #[test]
    fn route_request_unknown_to_address_misses() {
        install();
        let mut input = v2_route_input();
        // V2 bundle's `to` allow-list only contains the V2 router; any other
        // address must miss.
        input["to"] = json!("0x0000000000000000000000000000000000001234");
        input["ctx"]["to"] = json!("0x0000000000000000000000000000000000001234");
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "no_declarative_mapper");
    }

    #[test]
    fn route_request_unknown_chain_id_misses() {
        install();
        let mut input = v2_route_input();
        // V2 bundle covers chains [1, 8453, 10, 42161]. Polygon (137) is not
        // in the match table.
        input["chain_id"] = json!(137);
        input["ctx"]["chain_id"] = json!(137);
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "no_declarative_mapper");
    }

    #[test]
    fn route_request_normalises_decoder_id_so_static_callers_match() {
        install();
        // The caller's `decoded.decoder_id` here is the *static* id (the
        // Tier B abi-resolver still hands it out). The route entry must
        // overwrite it with the declarative one before invoking
        // `DeclarativeMapper::accepts` — otherwise `accepts` rejects and the
        // map fails. This regression-guards the v2_route_input shape.
        let out = declarative_route_request_json(v2_route_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
    }

    /// Primary verification that `declarative_route_request_json` decodes
    /// calldata internally (WASM-side decode path). Installs the
    /// `NFPM_BURN_BUNDLE_JSON` bundle and passes raw `burn(uint256)` calldata
    /// built by `burn_calldata` — no pre-decoded DTO, no dynamic types.
    /// Asserts the `burn_liquidity_nft` envelope and tokenId.
    #[test]
    fn route_request_burn_calldata_decoded_in_wasm() {
        let burn_out = declarative_install_json(NFPM_BURN_BUNDLE_JSON.to_owned());
        let burn_parsed: Value = serde_json::from_str(&burn_out).unwrap();
        assert_eq!(burn_parsed["ok"], true, "{burn_parsed}");

        let token_id: u64 = 42_000;
        let calldata_bytes = burn_calldata(token_id);
        let calldata_hex = format!("0x{}", hex::encode(&calldata_bytes));

        let input = json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0x42966c68",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata_hex
        });

        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["decoder_id"], "declarative.uniswap/v3/burn");
        let envelopes = parsed["data"]["envelopes"].as_array().expect("array");
        assert_eq!(envelopes.len(), 1);
        let env = &envelopes[0];
        assert_eq!(env["category"], "dex");
        assert_eq!(env["action"], "burn_liquidity_nft");
        assert_eq!(env["fields"]["nft"]["tokenId"], token_id.to_string());
        assert_eq!(env["fields"]["nft"]["kind"], "erc721");
    }

    #[test]
    fn route_request_invalid_json_errors_cleanly() {
        let out = declarative_route_request_json("{not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 7 T-B4 — WasmChildResolver + multicall_recurse wire-up
    // ──────────────────────────────────────────────────────────────────────

    /// V3 NFPM `multicall(bytes[])` outer bundle — `multicall_recurse`
    /// strategy. Selector `0xac9650d8`. Mirrors
    /// `registry/manifests/uniswap/v3/nfpm-multicall@1.0.0.json`.
    const NFPM_MULTICALL_BUNDLE_JSON: &str = r#"{
      "type": "adapter_function",
      "id": "uniswap/v3/nfpm-multicall@1.0.0",
      "publisher": "uniswap.eth",
      "match": {
        "chain_ids": [1],
        "to": ["0xC36442b4a4522E871399CD717aBDD847Ab11FE88"],
        "selector": "0xac9650d8"
      },
      "abi_fragment": {
        "function_name": "multicall",
        "abi": {
          "name": "multicall",
          "type": "function",
          "inputs": [
            { "name": "data", "type": "bytes[]" }
          ]
        }
      },
      "emit": {
        "strategy": "multicall_recurse",
        "recurse_rule_id": "self_array_bytes_last_arg",
        "max_depth": 3
      },
      "requires": {
        "imperative": ["multicall-recurse@^1.0"],
        "adapter_capabilities": [],
        "host_capabilities": [],
        "extension": ">=0.1.0"
      }
    }"#;

    /// V3 NFPM `burn(uint256 tokenId)` inner bundle — `single_emit`. Picks
    /// `burn_liquidity_nft` because mint/decreaseLiquidity carry tuple args
    /// and burn keeps the encoding trivial for a unit test. Mirrors
    /// `registry/manifests/uniswap/v3/burn@1.0.0.json`.
    const NFPM_BURN_BUNDLE_JSON: &str = r#"{
      "type": "adapter_function",
      "id": "uniswap/v3/burn@1.0.0",
      "publisher": "uniswap.eth",
      "match": {
        "chain_ids": [1],
        "to": ["0xC36442b4a4522E871399CD717aBDD847Ab11FE88"],
        "selector": "0x42966c68"
      },
      "abi_fragment": {
        "function_name": "burn",
        "abi": {
          "name": "burn",
          "type": "function",
          "inputs": [
            { "name": "tokenId", "type": "uint256" }
          ]
        }
      },
      "emit": {
        "strategy": "single_emit",
        "category": "dex",
        "action": "burn_liquidity_nft",
        "fields": {
          "nft.kind":      { "literal": "erc721" },
          "nft.address":   { "literal": "0xC36442b4a4522E871399CD717aBDD847Ab11FE88" },
          "nft.tokenId":   { "from": "$.args.tokenId" },
          "burnKind":      { "literal": "empty_only" }
        }
      },
      "requires": {
        "imperative": [],
        "adapter_capabilities": [],
        "host_capabilities": [],
        "extension": ">=0.1.0"
      }
    }"#;

    /// Build a `burn(uint256)` calldata payload for a given tokenId.
    fn burn_calldata(token_id: u64) -> Vec<u8> {
        let mut calldata = vec![0x42, 0x96, 0x6c, 0x68];
        let token_id_word: [u8; 32] = U256::from(token_id).to_be_bytes();
        calldata.extend_from_slice(&token_id_word);
        calldata
    }

    /// Build a `multicall(bytes[])` route input with a single inner `burn`
    /// calldata. Outer calldata is ABI-encoded via `alloy_dyn_abi` — no
    /// hand-typed hex for dynamic types.
    fn nfpm_multicall_route_input(token_id: u64) -> Value {
        use alloy_dyn_abi::DynSolValue;
        let inner = burn_calldata(token_id);
        let calldata = encode_calldata(
            "0xac9650d8",
            &[DynSolValue::Array(vec![DynSolValue::Bytes(inner)])],
        );
        json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0xac9650d8",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        })
    }

    #[test]
    fn route_request_resolves_v3_nfpm_multicall_through_resolver() {
        // Install both bundles. Burn is the inner mapper; nfpm-multicall is
        // the outer with `multicall_recurse`.
        let burn_out = declarative_install_json(NFPM_BURN_BUNDLE_JSON.to_owned());
        let burn_parsed: Value = serde_json::from_str(&burn_out).unwrap();
        assert_eq!(burn_parsed["ok"], true, "{burn_parsed}");
        assert_eq!(
            burn_parsed["data"]["decoder_id"],
            "declarative.uniswap/v3/burn"
        );

        let outer_out = declarative_install_json(NFPM_MULTICALL_BUNDLE_JSON.to_owned());
        let outer_parsed: Value = serde_json::from_str(&outer_out).unwrap();
        assert_eq!(outer_parsed["ok"], true, "{outer_parsed}");
        assert_eq!(
            outer_parsed["data"]["decoder_id"],
            "declarative.uniswap/v3/nfpm-multicall"
        );

        // Route the outer multicall — the WasmChildResolver MUST dispatch the
        // inner `burn` calldata back through this WASM state and surface a
        // single `burn_liquidity_nft` envelope.
        let input = nfpm_multicall_route_input(7777);
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");

        assert_eq!(
            parsed["data"]["decoder_id"],
            "declarative.uniswap/v3/nfpm-multicall"
        );
        let envelopes = parsed["data"]["envelopes"].as_array().expect("array");
        assert_eq!(
            envelopes.len(),
            1,
            "expected 1 inner burn envelope, got {}",
            envelopes.len()
        );
        let env = &envelopes[0];
        assert_eq!(env["category"], "dex");
        assert_eq!(env["action"], "burn_liquidity_nft");
        assert_eq!(env["fields"]["nft"]["tokenId"], "7777");
        assert_eq!(env["fields"]["nft"]["kind"], "erc721");
    }

    #[test]
    fn route_request_resolver_handles_unknown_child_gracefully() {
        // Install ONLY the outer multicall bundle, then route a payload whose
        // inner sub-call uses an *unbridged* selector (0xdeadbeef — no test
        // ever installs it). The resolver MUST surface a clear error
        // (mapped into the top-level `map_failed` envelope) instead of
        // panicking or returning silent success.
        //
        // We use a fabricated selector rather than `burn(...)` so the test
        // is robust to test-suite ordering even if `route_request_resolves_
        // v3_nfpm_multicall_through_resolver` were to run earlier on the same
        // `thread_local!` state.
        let outer_out = declarative_install_json(NFPM_MULTICALL_BUNDLE_JSON.to_owned());
        let outer_parsed: Value = serde_json::from_str(&outer_out).unwrap();
        assert_eq!(outer_parsed["ok"], true, "{outer_parsed}");

        // Build a synthetic inner payload — selector 0xdeadbeef + 32 padding
        // bytes. The shape is enough for `multicall_recurse` to extract the
        // child key; we want the *bridge lookup* inside the resolver to fail.
        let mut inner = vec![0xde, 0xad, 0xbe, 0xef];
        inner.extend_from_slice(&[0u8; 32]);
        use alloy_dyn_abi::DynSolValue;
        let calldata = encode_calldata(
            "0xac9650d8",
            &[DynSolValue::Array(vec![DynSolValue::Bytes(inner)])],
        );
        let input = json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0xac9650d8",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "map_failed");
        let message = parsed["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("WasmChildResolver"),
            "expected WasmChildResolver error, got: {message}"
        );
        assert!(
            message.contains("no declarative mapper"),
            "expected no-mapper diagnostic, got: {message}"
        );
    }

    // ── declarative_plan_children_json ──────────────────────────────────────

    #[test]
    fn plan_children_lists_inner_callkey_for_multicall() {
        let outer_out = declarative_install_json(NFPM_MULTICALL_BUNDLE_JSON.to_owned());
        assert_eq!(
            serde_json::from_str::<Value>(&outer_out).unwrap()["ok"],
            true
        );

        let input = nfpm_multicall_route_input(4242);
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["decoder_id"],
            "declarative.uniswap/v3/nfpm-multicall"
        );
        let children = parsed["data"]["children"]
            .as_array()
            .expect("children array");
        assert_eq!(children.len(), 1, "{parsed}");
        assert_eq!(children[0]["chain_id"], 1);
        assert_eq!(children[0]["selector"], "0x42966c68");
        assert_eq!(
            children[0]["to"],
            "0xc36442b4a4522e871399cd717abdd847ab11fe88"
        );
    }

    #[test]
    fn plan_children_lists_multiple_children_in_order() {
        use alloy_dyn_abi::DynSolValue;
        declarative_install_json(NFPM_MULTICALL_BUNDLE_JSON.to_owned());

        // multicall([ burn(11), <selector 0xdeadbeef + 32B pad> ]) — the
        // planner extracts selectors regardless of whether a child has a
        // bundle, so a fabricated second selector exercises ordering.
        let mut second = vec![0xde, 0xad, 0xbe, 0xef];
        second.extend_from_slice(&[0u8; 32]);
        let calldata = encode_calldata(
            "0xac9650d8",
            &[DynSolValue::Array(vec![
                DynSolValue::Bytes(burn_calldata(11)),
                DynSolValue::Bytes(second),
            ])],
        );
        let input = json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0xac9650d8",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        let children = parsed["data"]["children"]
            .as_array()
            .expect("children array");
        assert_eq!(children.len(), 2, "{parsed}");
        // Order MUST mirror the `bytes[]` order.
        assert_eq!(children[0]["selector"], "0x42966c68");
        assert_eq!(children[1]["selector"], "0xdeadbeef");
    }

    #[test]
    fn plan_children_empty_for_non_recurse_bundle() {
        let burn_out = declarative_install_json(NFPM_BURN_BUNDLE_JSON.to_owned());
        assert_eq!(
            serde_json::from_str::<Value>(&burn_out).unwrap()["ok"],
            true
        );

        // Point at the burn callkey — a `single_emit` bundle, not multicall.
        // The planner returns early (children:[]) before decoding calldata.
        let input = json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0x42966c68",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": "0x42966c68"
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["decoder_id"], "declarative.uniswap/v3/burn");
        assert_eq!(
            parsed["data"]["children"].as_array().expect("array").len(),
            0
        );
    }

    #[test]
    fn plan_children_empty_when_no_bundle_mounted() {
        // A callkey no test ever installs — a bridge miss must yield an empty
        // child list (ok:true), NOT an error, so the caller skips the prefetch.
        let input = json!({
            "chain_id": 1,
            "to":       "0x0000000000000000000000000000000000009999",
            "selector": "0x99999999",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x0000000000000000000000000000000000009999",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": "0x99999999"
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["children"].as_array().expect("array").len(),
            0
        );
        assert_eq!(parsed["data"]["decoder_id"], "");
    }

    #[test]
    fn plan_children_rejects_invalid_json() {
        let out = declarative_plan_children_json("{not json".to_owned());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "invalid_input_json");
    }

    // ── Track B Fix 3a — UR cross-target child extraction ──────────────────

    /// UR V2 V4-aware `execute(bytes,bytes[],uint256)` on Base (chain 8453).
    /// Schema v2 — `chain_to_addresses` map. Used to validate Fix 3a's
    /// cross-target child callkey extraction (0x11/0x12/0x14 dispatch).
    const UR_EXECUTE_V2_BASE_BUNDLE_JSON: &str = r#"{
      "type": "adapter_function",
      "id": "uniswap/universal-router/execute-v2@1.0.0",
      "publisher": "uniswap.eth",
      "schema_version": "2",
      "match": {
        "chain_to_addresses": {
          "8453": ["0x6fF5693b99212Da76ad316178A184AB56D299b43"]
        },
        "selector": "0x3593564c"
      },
      "abi_fragment": {
        "function_name": "execute",
        "abi": {
          "name": "execute",
          "type": "function",
          "inputs": [
            { "name": "commands", "type": "bytes" },
            { "name": "inputs", "type": "bytes[]" },
            { "name": "deadline", "type": "uint256" }
          ]
        }
      },
      "emit": {
        "strategy": "opcode_stream_dispatch",
        "dispatcher_id": "universal_router",
        "mask": "0x7f",
        "allow_revert_bit": "0x80",
        "per_opcode_emit": {},
        "unknown_opcode_policy": "warn"
      },
      "requires": {
        "imperative": ["opcode-stream-dispatch@^1.0"],
        "adapter_capabilities": [],
        "host_capabilities": [],
        "extension": ">=0.1.0"
      }
    }"#;

    /// V4 PositionManager `modifyLiquidities(bytes,uint256)` on Base.
    /// dispatcher_id = "v4_position_manager" — no cross-target opcodes,
    /// only internal V4 action stream. Planner must return children: [].
    const V4_PM_MODIFY_LIQUIDITIES_BASE_BUNDLE_JSON: &str = r#"{
      "type": "adapter_function",
      "id": "uniswap/position-manager/modifyLiquidities@1.0.0",
      "publisher": "uniswap.eth",
      "schema_version": "2",
      "match": {
        "chain_to_addresses": {
          "8453": ["0x7C5f5A4bBd8fD63184577525326123B519429bDc"]
        },
        "selector": "0xdd46508f"
      },
      "abi_fragment": {
        "function_name": "modifyLiquidities",
        "abi": {
          "name": "modifyLiquidities",
          "type": "function",
          "inputs": [
            { "name": "unlockData", "type": "bytes" },
            { "name": "deadline", "type": "uint256" }
          ]
        }
      },
      "emit": {
        "strategy": "opcode_stream_dispatch",
        "dispatcher_id": "v4_position_manager",
        "mask": "0xff",
        "allow_revert_bit": "0x00",
        "per_opcode_emit": {},
        "unknown_opcode_policy": "warn"
      },
      "requires": {
        "imperative": ["opcode-stream-dispatch@^1.0"],
        "adapter_capabilities": [],
        "host_capabilities": [],
        "extension": ">=0.1.0"
      }
    }"#;

    /// Build a UR `execute(commands, inputs, deadline)` calldata. Each
    /// `inputs[i]` is wrapped raw bytes — the planner pattern-matches on
    /// `(Bytes, Array<Bytes>, _)` via `extract_commands_and_inputs`.
    fn ur_execute_calldata(commands: Vec<u8>, inputs: Vec<Vec<u8>>) -> String {
        use alloy_dyn_abi::DynSolValue;
        let inputs_values: Vec<DynSolValue> = inputs.into_iter().map(DynSolValue::Bytes).collect();
        encode_calldata(
            "0x3593564c",
            &[
                DynSolValue::Bytes(commands),
                DynSolValue::Array(inputs_values),
                DynSolValue::Uint(U256::from(1_900_000_000_u64), 256),
            ],
        )
    }

    /// Wrap an inner selector + 32B-pad word so it parses as a min-size
    /// inner call. Real cross-target inner blobs are longer but the planner
    /// only reads the first 4 bytes.
    fn padded_inner_calldata(selector: [u8; 4]) -> Vec<u8> {
        let mut b = selector.to_vec();
        b.extend_from_slice(&[0u8; 32]);
        b
    }

    #[test]
    fn plan_children_extracts_cross_target_for_ur_v2_execute_on_base() {
        // Mirrors the Tx 3 scenario: UR V2 V4-aware execute on Base 8453 with
        // commands `0x11 0x12 0x12 0x12 0x14` → 5 cross-target child callkeys.
        // The Fix 3a planner must extract each as `(8453, v3_pm | v4_pm, sel)`.
        declarative_install_json(UR_EXECUTE_V2_BASE_BUNDLE_JSON.to_owned());

        // Inner selectors mirroring Tx 3's actual payload.
        let nfpm_permit = padded_inner_calldata([0x7a, 0xc2, 0xff, 0x7b]);
        let nfpm_decrease = padded_inner_calldata([0x0c, 0x49, 0xcc, 0xbe]);
        let nfpm_collect = padded_inner_calldata([0xfc, 0x6f, 0x78, 0x65]);
        let nfpm_burn = padded_inner_calldata([0x42, 0x96, 0x6c, 0x68]);
        let v4_pm_modify = padded_inner_calldata([0xdd, 0x46, 0x50, 0x8f]);

        let calldata = ur_execute_calldata(
            vec![0x11, 0x12, 0x12, 0x12, 0x14],
            vec![
                nfpm_permit,
                nfpm_decrease,
                nfpm_collect,
                nfpm_burn,
                v4_pm_modify,
            ],
        );
        let input = json!({
            "chain_id": 8453,
            "to":       "0x6ff5693b99212da76ad316178a184ab56d299b43",
            "selector": "0x3593564c",
            "ctx": {
                "chain_id": 8453,
                "from": "0x676fa5b94067c2be14bc025df6c5c80dedf49a54",
                "to":   "0x6ff5693b99212da76ad316178a184ab56d299b43",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        let children = parsed["data"]["children"].as_array().expect("children");
        assert_eq!(children.len(), 5, "{parsed}");

        // children[0] = NFPM permit (0x11 → v3_pm)
        assert_eq!(children[0]["chain_id"], 8453);
        assert_eq!(
            children[0]["to"],
            "0x03a520b32c04bf3beef7beb72e919cf822ed34f1"
        );
        assert_eq!(children[0]["selector"], "0x7ac2ff7b");

        // children[1..=3] = NFPM decrease/collect/burn (0x12 → v3_pm)
        assert_eq!(
            children[1]["to"],
            "0x03a520b32c04bf3beef7beb72e919cf822ed34f1"
        );
        assert_eq!(children[1]["selector"], "0x0c49ccbe");
        assert_eq!(children[2]["selector"], "0xfc6f7865");
        assert_eq!(children[3]["selector"], "0x42966c68");

        // children[4] = V4 PM modifyLiquidities (0x14 → v4_pm)
        assert_eq!(
            children[4]["to"],
            "0x7c5f5a4bbd8fd63184577525326123b519429bdc"
        );
        assert_eq!(children[4]["selector"], "0xdd46508f");
    }

    #[test]
    fn plan_children_empty_for_v4_pm_modify_liquidities() {
        // V4 PM's modifyLiquidities is opcode_stream_dispatch but
        // dispatcher_id = "v4_position_manager" — internal action stream,
        // no cross-target opcodes. Planner returns children:[].
        declarative_install_json(V4_PM_MODIFY_LIQUIDITIES_BASE_BUNDLE_JSON.to_owned());

        // Minimal unlockData (bytes) + deadline.
        use alloy_dyn_abi::DynSolValue;
        let calldata = encode_calldata(
            "0xdd46508f",
            &[
                DynSolValue::Bytes(vec![0x00; 4]),
                DynSolValue::Uint(U256::from(1_900_000_000_u64), 256),
            ],
        );
        let input = json!({
            "chain_id": 8453,
            "to":       "0x7c5f5a4bbd8fd63184577525326123b519429bdc",
            "selector": "0xdd46508f",
            "ctx": {
                "chain_id": 8453,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x7c5f5a4bbd8fd63184577525326123b519429bdc",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["children"].as_array().expect("array").len(),
            0
        );
    }

    #[test]
    fn plan_children_empty_for_ur_execute_without_cross_target_opcodes() {
        // UR execute with only V2/V3 swap commands (0x00, 0x08, 0x09) — no
        // cross-target opcodes. Planner returns children:[].
        declarative_install_json(UR_EXECUTE_V2_BASE_BUNDLE_JSON.to_owned());

        let dummy = padded_inner_calldata([0x00, 0x00, 0x00, 0x00]);
        let calldata = ur_execute_calldata(
            vec![0x00, 0x08, 0x09],
            vec![dummy.clone(), dummy.clone(), dummy],
        );
        let input = json!({
            "chain_id": 8453,
            "to":       "0x6ff5693b99212da76ad316178a184ab56d299b43",
            "selector": "0x3593564c",
            "ctx": {
                "chain_id": 8453,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0x6ff5693b99212da76ad316178a184ab56d299b43",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_plan_children_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(
            parsed["data"]["children"].as_array().expect("array").len(),
            0,
            "no cross-target opcodes → empty children, got {parsed}"
        );
    }

    #[test]
    fn route_request_resolves_two_child_multicall() {
        use alloy_dyn_abi::DynSolValue;
        declarative_install_json(NFPM_BURN_BUNDLE_JSON.to_owned());
        declarative_install_json(NFPM_MULTICALL_BUNDLE_JSON.to_owned());

        // multicall([ burn(11), burn(22) ]) — both children resolve to the
        // installed burn mapper, so the route MUST emit two envelopes. This is
        // the unit-level analogue of the real Base NFPM multicall([mint,
        // refundETH]) the child-prefetch fix targets.
        let calldata = encode_calldata(
            "0xac9650d8",
            &[DynSolValue::Array(vec![
                DynSolValue::Bytes(burn_calldata(11)),
                DynSolValue::Bytes(burn_calldata(22)),
            ])],
        );
        let input = json!({
            "chain_id": 1,
            "to":       "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "selector": "0xac9650d8",
            "ctx": {
                "chain_id": 1,
                "from": "0x000000000000000000000000000000000000aaaa",
                "to":   "0xc36442b4a4522e871399cd717abdd847ab11fe88",
                "value_wei": "0",
                "block_timestamp": 1_700_000_000_u64
            },
            "calldata": calldata
        });
        let out = declarative_route_request_json(input.to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], true, "{parsed}");
        let envelopes = parsed["data"]["envelopes"].as_array().expect("array");
        assert_eq!(envelopes.len(), 2, "{parsed}");
        assert_eq!(envelopes[0]["fields"]["nft"]["tokenId"], "11");
        assert_eq!(envelopes[1]["fields"]["nft"]["tokenId"], "22");
    }

    // ──────────────────────────────────────────────────────────────────────
    // Phase 4B — declarative_route_request_v3_json (stub Unknown emitter)
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
        // `no_declarative_v3_mapper` so the SW caller can fall through to
        // the v1 path. This replaces the legacy "always emit Unknown" stub
        // — `ActionBody::Unknown` is no longer a route result, it's only a
        // catch-all variant the policy engine emits for un-classified
        // calls upstream.
        let out = declarative_route_request_v3_json(v3_route_input().to_string());
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["ok"], false, "{parsed}");
        assert_eq!(parsed["error"]["kind"], "no_declarative_v3_mapper", "{parsed}");
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
        // the bridge lookup is what fails. This is the M2 equivalent of the
        // pre-M2 "uses_defaults" assertion, narrowed to what's observable
        // when no v3 bundle is installed.
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
        assert_eq!(parsed["error"]["kind"], "no_declarative_v3_mapper", "{parsed}");
    }
}
