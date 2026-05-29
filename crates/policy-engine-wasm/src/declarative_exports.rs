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
    build_action_body, build_multicall_from_opcode_stream, UnknownOpcodePolicy as V3UnknownOpcodePolicy,
    V3MapContext,
};
use mappers::declarative::eval::args_to_json;
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

/// Typed-data bridge key: `(chain_id, verifying_contract_lowercase, primary_type)`.
///
/// Parallel to [`BridgeKey`] but for the off-chain EIP-712 path. `verifying_contract`
/// is normalised to lowercase hex (no checksum) so the lookup is case-insensitive
/// — manifests may carry checksummed addresses while the orchestrator side sends
/// whatever the wallet surfaced. `primary_type` is the EIP-712 `primaryType` string
/// (e.g. `"PermitSingle"`, `"PermitWitnessTransferFrom"`,
/// `"HyperliquidTransaction:UsdSend"`) — kept verbatim (NOT lowered) since it is the
/// exact discriminator the wallet's `eth_signTypedData` payload carries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TypedDataBridgeKey {
    chain_id: u64,
    verifying_contract: String,
    primary_type: String,
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

        // Phase A.1 — off-chain EIP-712 typed-data bridge. Manifests carrying
        // a `match.typed_data` block additionally register a
        // `(chain_id, verifying_contract, primary_type)` → bundle_id mapping so
        // [`declarative_route_typed_data_v3_json`] can route an off-chain
        // signature payload to the same emit-rule the calldata path uses.
        // calldata-only manifests omit `typed_data` and skip this entirely.
        let typed_data_route: Option<(String, String)> = match_value
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
                Some((vc, pt))
            });

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
            if let Some((ref vc, ref pt)) = typed_data_route {
                for (chain_id, _to) in bundle_match.entries() {
                    let key = TypedDataBridgeKey {
                        chain_id,
                        verifying_contract: vc.clone(),
                        primary_type: pt.clone(),
                    };
                    state.typed_data_bridge.insert(key, bundle_id.clone());
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
        // normalisation; `primary_type` is kept verbatim (it is the exact
        // EIP-712 discriminator). A miss surfaces `no_typed_data_mapper` so the
        // SW caller can surface the gap.
        let key = TypedDataBridgeKey {
            chain_id: input.chain_id,
            verifying_contract: input.verifying_contract.to_ascii_lowercase(),
            primary_type: input.primary_type.clone(),
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
                        "no typed-data mapper bridged for chain_id={} verifying_contract={} primary_type={}",
                        input.chain_id, input.verifying_contract, input.primary_type
                    ),
                )
            })?;

        let emit = bundle_value.get("emit").ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit".to_string())
        })?;
        let strategy = emit
            .get("strategy")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EngineErrorDto::new("invalid_bundle", "missing emit.strategy".to_string())
            })?;

        // Typed-data is single_emit only — opcode_stream_dispatch is a
        // calldata-stream construct (no off-chain sig analogue).
        if strategy != "single_emit" {
            return Err(EngineErrorDto::new(
                "unsupported_strategy_for_typed_data",
                format!(
                    "emit.strategy {strategy:?} is calldata-only; typed-data routing supports single_emit only"
                ),
            ));
        }

        // ── Build args_json from the EIP-712 message (WRAP RULE) ───────────
        let args_json = build_typed_data_args_json(bundle_value.pointer("/abi_fragment/abi"), &input.primary_type, &input.message);

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

        let ctx = V3MapContext {
            chain: chain.clone(),
            tx_to: verifying_contract,
            tx_from: submitter,
            value: V3U256::ZERO,
            submitted_at,
            args_json: &args_json,
            resolved,
            derived: BTreeMap::new(),
            inputs: None,
        };

        let body_template = emit.get("body").ok_or_else(|| {
            EngineErrorDto::new("invalid_bundle", "missing emit.body".to_string())
        })?;
        let live_inputs_template = emit.get("live_inputs");
        let body = build_action_body(&ctx, body_template, live_inputs_template).map_err(|error| {
            EngineErrorDto::new("build_action_body_failed", error.to_string())
        })?;

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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(parsed["error"]["kind"], "no_declarative_v3_mapper", "{parsed}");
    }
}
