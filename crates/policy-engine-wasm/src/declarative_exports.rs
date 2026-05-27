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
use std::collections::HashMap;
use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::subdecode::protocols::universal_router::{
    v3_position_manager_address, v4_position_manager_address, UNISWAP_UR_MASK,
};
use abi_resolver::{CallMatchKey, DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::{I256, U256};
use mappers::declarative::multicall::extract_self_array_bytes;
use mappers::declarative::opcode_stream::{
    extract_ur_commands_and_inputs, DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
    DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER, DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER,
    DISPATCHER_ID_UNIVERSAL_ROUTER, DISPATCHER_ID_V4_POSITION_MANAGER,
};
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
    DeclarativeRouteRequestResultDto, DecodedArgDto, DecodedCallDto, DecodedValueDto,
    EngineErrorDto, Envelope,
};
use crate::exports::check_input_size;

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
}
