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
//!     orchestrator entry. Caller passes `(chain_id, to, selector, decoded,
//!     ctx)`; we look up the matching declarative decoder via the bridge,
//!     fall through to `MapperError`-style miss when nothing matches, and
//!     otherwise run `mapper.map(ctx, decoded)`. Returns the same
//!     `{ ok, envelopes | error }` envelope as `declarative_lookup_json`.
//!
//! Wire shape (input/output) is documented inline next to each export. This
//! module forms the contract that the Phase 1B + Phase 6 TS bridges consume.

use std::cell::RefCell;
use std::collections::HashMap;
use std::str::FromStr as _;
use std::sync::Arc;

use abi_resolver::{CallMatchKey, DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::{I256, U256};
use mappers::declarative::{AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{ChildResolver, MapContext, Mapper, MapperError};
use mappers::token_registry::EmptyTokenRegistry;
use policy_engine::action::{Address, DecimalString};
use wasm_bindgen::prelude::*;

use crate::dto::{
    DecodedArgDto, DecodedCallDto, DecodedValueDto, DeclarativeInstallResultDto,
    DeclarativeLookupInputDto, DeclarativeRouteRequestInputDto, DeclarativeRouteRequestResultDto,
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

/// Expand `bundle.match.{chain_ids × to × selector}` into bridge entries.
/// Existing entries for the same callkey are replaced (re-install semantics).
fn register_bridge_entries(bundle: &AdapterFunctionBundle, decoder_id: &str, state: &mut DeclarativeState) {
    let selector = bundle.match_.selector.to_ascii_lowercase();
    for &chain_id in &bundle.match_.chain_ids {
        for to in &bundle.match_.to {
            let key = BridgeKey {
                chain_id,
                to: to.to_ascii_lowercase(),
                selector: selector.clone(),
            };
            state.bridge.insert(key, decoder_id.to_string());
        }
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
/// `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §4.1 (see
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
        let bundle: AdapterFunctionBundle = serde_json::from_str(&bundle_json).map_err(|error| {
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
            state
                .mappers
                .insert(decoder_id.clone(), Arc::new(mapper));
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
/// Input JSON shape (see [`DeclarativeLookupInputDto`]):
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

        let mapper = DECLARATIVE_STATE
            .with(|state| state.borrow().mappers.get(&input.decoder_id).cloned());
        let mapper = mapper.ok_or_else(|| {
            EngineErrorDto::new(
                "decoder_id_not_installed",
                format!(
                    "no declarative mapper installed for decoder_id {:?}",
                    input.decoder_id
                ),
            )
        })?;

        let from =
            Address::from_str(&input.ctx.from).map_err(|message| {
                EngineErrorDto::new("invalid_from", format!("invalid ctx.from: {message}"))
            })?;
        let to = Address::from_str(&input.ctx.to)
            .map_err(|message| EngineErrorDto::new("invalid_to", format!("invalid ctx.to: {message}")))?;
        let value_wei = input
            .ctx
            .value_wei
            .as_deref()
            .unwrap_or("0");
        let value = DecimalString::from_str(value_wei).map_err(|message| {
            EngineErrorDto::new("invalid_value_wei", format!("invalid ctx.value_wei: {message}"))
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
/// Input JSON shape (see [`DeclarativeRouteRequestInputDto`]):
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
///   "decoded": { ... }   // same shape as declarative_lookup_json
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
        let input: DeclarativeRouteRequestInputDto = serde_json::from_str(&input_json)
            .map_err(|error| {
                EngineErrorDto::new(
                    "invalid_input_json",
                    format!("invalid input json: {error}"),
                )
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
            state
                .bridge
                .get(&key)
                .and_then(|decoder_id| {
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

        let mut decoded = decoded_call_from_dto(input.decoded)?;
        // Override the decoder_id the caller supplied with the canonical one
        // the bridge resolved. `DeclarativeMapper::accepts` is strict on
        // decoder_id equality — orchestrators that don't (yet) know the
        // declarative id can pass whatever they have (or even the static
        // selector-derived id) and the route entry normalises it.
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

    fn v2_route_input() -> Value {
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
            "decoded": {
                "decoder_id": "static.uniswap-v2/swapExactTokensForTokens",
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
    /// calldata. The outer call's `decoded` is constructed manually because
    /// the DTO layer expects `kind: "bytes"` for `bytes[]` elements.
    fn nfpm_multicall_route_input(token_id: u64) -> Value {
        let inner = burn_calldata(token_id);
        let inner_hex = format!("0x{}", hex::encode(&inner));
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
            "decoded": {
                "decoder_id": "fallback/0xac9650d8",
                "function_signature": "multicall(bytes[])",
                "args": [
                    { "name": "data", "abi_type": "bytes[]",
                      "value": { "kind": "array", "value": [
                          { "kind": "bytes", "value": inner_hex }
                      ] } }
                ]
            }
        })
    }

    #[test]
    fn route_request_resolves_v3_nfpm_multicall_through_resolver() {
        // Install both bundles. Burn is the inner mapper; nfpm-multicall is
        // the outer with `multicall_recurse`.
        let burn_out = declarative_install_json(NFPM_BURN_BUNDLE_JSON.to_owned());
        let burn_parsed: Value = serde_json::from_str(&burn_out).unwrap();
        assert_eq!(burn_parsed["ok"], true, "{burn_parsed}");
        assert_eq!(burn_parsed["data"]["decoder_id"], "declarative.uniswap/v3/burn");

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
        let inner_hex = format!("0x{}", hex::encode(&inner));
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
            "decoded": {
                "decoder_id": "fallback/0xac9650d8",
                "function_signature": "multicall(bytes[])",
                "args": [
                    { "name": "data", "abi_type": "bytes[]",
                      "value": { "kind": "array", "value": [
                          { "kind": "bytes", "value": inner_hex }
                      ] } }
                ]
            }
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
}
