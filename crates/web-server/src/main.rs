//! HTTP server exposing the abi-resolver decode pipeline as a JSON API.
//!
//! Endpoints:
//! - `POST /api/decode` — decode arbitrary calldata
//! - `POST /api/event`  — receive an extracted RPC event (from the
//!   ScopeBall userscript / extension) and broadcast it to SSE subscribers
//! - `GET  /api/event/stream` — SSE feed of broadcast events
//! - `GET  /api/health` — liveness probe
//! - `GET  /` (and other paths) — serve the React build from `frontend/dist/`
//!   when present; otherwise return a hint message.
//!
//! Run with:
//!     cargo run -p web-server
//!
//! Defaults to `0.0.0.0:3000`. Override with `WEB_SERVER_ADDR=127.0.0.1:8080`.

use abi_resolver::decode::{format_value_named, DecodedArg};
use abi_resolver::openchain::{OpenchainIndex, SignatureCandidate};
use abi_resolver::resolver::{ResolveOutcome, Resolver, Source};
use abi_resolver::sourcify::SourcifyIndex;
use abi_resolver::sqlite_index::SqliteSourcifyIndex;
use abi_resolver::subdecode::enum_tagged::{try_dispatch as enum_try_dispatch, DecodedEnum};
use abi_resolver::subdecode::opcode_stream::OpcodeTable;
use abi_resolver::subdecode::opcode_stream::{
    dispatch as dispatch_opcode_stream, DecodedStep, StepDecodeError,
};
use abi_resolver::subdecode::protocols::balancer_v2::{
    BALANCER_V2_EXIT_KIND_STABLE, BALANCER_V2_EXIT_KIND_WEIGHTED, BALANCER_V2_JOIN_KIND,
    EXIT_POOL_SELECTOR, JOIN_POOL_SELECTOR,
};
use abi_resolver::subdecode::protocols::pancake_infinity::{
    extract_actions_and_params as extract_infi_actions_and_params, PANCAKE_INFI_TABLE,
};
use abi_resolver::subdecode::protocols::pancake_ur::{
    is_pancake_universal_router, PANCAKE_UR_TABLE,
};
use abi_resolver::subdecode::protocols::safe_multisend::{
    extract_transactions_bytes, parse_multisend_transactions, MULTISEND_SELECTOR,
};
use abi_resolver::subdecode::protocols::uniswap_v3::format_packed_path;
use abi_resolver::subdecode::protocols::universal_router::{
    extract_commands_and_inputs, is_uniswap_universal_router, is_universal_router_execute,
    v3_position_manager_address, v4_position_manager_address, UNISWAP_UR_TABLE,
};
use abi_resolver::subdecode::protocols::v4_router::{
    extract_actions_and_params as extract_v4_actions_and_params,
    extract_modify_liquidities_actions_and_params, is_v4_position_manager_modify_liquidities,
    V4_ROUTER_TABLE,
};
use abi_resolver::subdecode::recurse::{extract_children, lookup_recurse_rule};
use alloy_dyn_abi::DynSolValue;
use alloy_json_abi::Param;
use alloy_primitives::Address;
use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

mod etherscan;
use etherscan::EtherscanClient;

// ── /api/route ────────────────────────────────────────────────────────────────
// New unified entry point exposing the Phase 5 `request_router::route_request`
// pipeline. Keeps `/api/decode` and `/api/sign` available unchanged for
// backward compatibility; `/api/route` is the migration target for the
// extension and other consumers once Phase 8 lands.

#[derive(Deserialize)]
struct RouteRequest {
    method: String,
    params: serde_json::Value,
    chain_id: u64,
    #[serde(default)]
    block_timestamp: Option<u64>,
}

#[derive(Serialize)]
struct RouteResponse {
    actions: Vec<serde_json::Value>,
}

async fn route(State(state): State<AppState>, Json(req): Json<RouteRequest>) -> Response {
    let registries = state.route_registries.as_ref();
    let token_registry = mappers::EmptyTokenRegistry;
    let ctx = request_router::RouterContext {
        registries,
        token_registry: &token_registry,
        block_timestamp: req.block_timestamp,
    };
    match request_router::route_request(&ctx, &req.method, &req.params, req.chain_id) {
        Ok(envelopes) => {
            let mut actions = Vec::with_capacity(envelopes.len());
            for env in &envelopes {
                match serde_json::to_value(env) {
                    Ok(v) => actions.push(v),
                    Err(e) => {
                        return err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("envelope serialize: {e}"),
                        );
                    }
                }
            }
            Json(RouteResponse { actions }).into_response()
        }
        Err(e) => err(StatusCode::BAD_REQUEST, format!("route error: {e}")),
    }
}

// ── /api/sign ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SignDecodeRequest {
    method: String,
    params: serde_json::Value,
    chain_id: u64,
}

#[derive(Serialize)]
struct SignDecodeResponse {
    method: String,
    signer: String,
    chain_id: u64,
    payload: serde_json::Value,
}

fn sign_payload_to_json(payload: &sign_resolver::SignPayload) -> serde_json::Value {
    use serde_json::json;
    use sign_resolver::SignPayload;
    match payload {
        SignPayload::TypedData(v) => json!({ "kind": "typed_data", "data": v }),
        SignPayload::RawMessage(s) => json!({ "kind": "raw_message", "message": s }),
        SignPayload::RawHash(s) => json!({ "kind": "raw_hash", "hash": s }),
        SignPayload::Transaction(v) => json!({ "kind": "transaction", "tx": v }),
        SignPayload::UserOperation {
            user_op,
            entry_point,
        } => {
            json!({ "kind": "user_operation", "user_op": user_op, "entry_point": entry_point })
        }
        SignPayload::PermissionRequest(v) => json!({ "kind": "permission_request", "request": v }),
    }
}

async fn decode_sign(Json(req): Json<SignDecodeRequest>) -> Response {
    match sign_resolver::parse_sign_request(&req.method, &req.params, req.chain_id) {
        Ok(sign_req) => Json(SignDecodeResponse {
            method: sign_req.method.as_str().to_string(),
            signer: sign_req.signer,
            chain_id: sign_req.chain_id,
            payload: sign_payload_to_json(&sign_req.payload),
        })
        .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

const SOURCIFY_BUNDLE: &[u8] = include_bytes!("../../adapters/abi-resolver/data/sourcify.json");

/// Address-agnostic selector → signature seed list used as the third resolver
/// tier. Each entry is reached only when neither tier 1 (curated
/// `sourcify.json`) nor tier 2 (SQLite Sourcify dump) had `(chain, address,
/// selector)` for the request.
///
/// The seed exists to keep common DeFi entrypoints decodable even on contract
/// addresses that aren't on Sourcify yet — for example a freshly deployed
/// Universal Router that's only verified on Etherscan. Without it, calldata
/// against such addresses falls back to `NotFound`.
fn seed_signatures() -> &'static [([u8; 4], &'static str)] {
    &[
        ([0x09, 0x5e, 0xa7, 0xb3], "approve(address spender, uint256 amount)"),
        ([0xa9, 0x05, 0x9c, 0xbb], "transfer(address to, uint256 amount)"),
        (
            [0x23, 0xb8, 0x72, 0xdd],
            "transferFrom(address from, address to, uint256 amount)",
        ),
        ([0x70, 0xa0, 0x82, 0x31], "balanceOf(address account)"),
        (
            [0x41, 0x4b, 0xf3, 0x89],
            "exactInputSingle((address tokenIn, address tokenOut, uint24 fee, address recipient, uint256 deadline, uint256 amountIn, uint256 amountOutMinimum, uint160 sqrtPriceLimitX96) params)",
        ),
        (
            [0xc0, 0x4b, 0x8d, 0x59],
            "exactInput((bytes path, address recipient, uint256 deadline, uint256 amountIn, uint256 amountOutMinimum) params)",
        ),
        ([0xac, 0x96, 0x50, 0xd8], "multicall(bytes[] data)"),
        (
            [0x5a, 0xe4, 0x01, 0xdc],
            "multicall(uint256 deadline, bytes[] data)",
        ),
        (
            [0x38, 0xed, 0x17, 0x39],
            "swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x7f, 0xf3, 0x6a, 0xb5],
            "swapExactETHForTokens(uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0xb6, 0xf9, 0xde, 0x95],
            "swapExactETHForTokensSupportingFeeOnTransferTokens(uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x79, 0x1a, 0xc9, 0x47],
            "swapExactTokensForETHSupportingFeeOnTransferTokens(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x24, 0x85, 0x6b, 0xc3],
            "execute(bytes commands, bytes[] inputs)",
        ),
        (
            [0x35, 0x93, 0x56, 0x4c],
            "execute(bytes commands, bytes[] inputs, uint256 deadline)",
        ),
        (
            [0x61, 0x7b, 0xa0, 0x37],
            "supply(address asset, uint256 amount, address onBehalfOf, uint16 referralCode)",
        ),
        (
            [0x69, 0x32, 0x8d, 0xec],
            "withdraw(address asset, uint256 amount, address to)",
        ),
        (
            [0xa4, 0x15, 0xbc, 0xad],
            "borrow(address asset, uint256 amount, uint256 interestRateMode, uint16 referralCode, address onBehalfOf)",
        ),
        (
            [0x57, 0x3a, 0xde, 0x81],
            "repay(address asset, uint256 amount, uint256 interestRateMode, address onBehalfOf)",
        ),
        (
            [0x37, 0x4f, 0x43, 0x5d],
            "multicall((address target, bytes data, uint256 value, bool skipRevert, bytes32 callbackHash)[] calls)",
        ),
        (
            [0x8d, 0x80, 0xff, 0x0a],
            "multiSend(bytes transactions)",
        ),
        (
            [0x09, 0x7c, 0xd2, 0x32],
            "injectReward(uint256,uint256)",
        ),
    ]
}

#[derive(Clone)]
struct AppState {
    resolver: Arc<Resolver>,
    /// Optional Etherscan v2 fallback. Populated from `ETHERSCAN_API_KEY`
    /// at startup; `None` when unset, in which case the fourth resolver
    /// tier is silently skipped.
    etherscan: Option<Arc<EtherscanClient>>,
    event_tx: broadcast::Sender<String>,
    /// Phase 5 registries used by `POST /api/route`. Built once at startup
    /// from `request_router::DefaultRegistries::standard()`.
    route_registries: Arc<request_router::DefaultRegistries>,
}

fn build_resolver() -> Resolver {
    let sourcify = SourcifyIndex::load_bundle(SOURCIFY_BUNDLE)
        .expect("bundled sourcify.json should deserialize");
    let mut openchain = OpenchainIndex::empty();
    for (selector, signature) in seed_signatures() {
        openchain.insert(
            *selector,
            SignatureCandidate {
                signature: (*signature).into(),
                verified: true,
            },
        );
    }
    let mut resolver = Resolver::new(sourcify, openchain);
    if let Some(path) = sqlite_db_path() {
        match SqliteSourcifyIndex::open_read_only(&path) {
            Ok(db) => {
                tracing::info!("attached SQLite Sourcify dump at {}", path.display());
                resolver = resolver.with_sqlite(db);
            }
            Err(e) => {
                tracing::warn!(
                    "could not attach SQLite dump at {} ({e}); falling back to curated bundle only",
                    path.display()
                );
            }
        }
    } else {
        tracing::info!("no SQLite Sourcify dump configured (set SOURCIFY_SQLITE_PATH)");
    }
    resolver
}

fn sqlite_db_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SOURCIFY_SQLITE_PATH") {
        return Some(PathBuf::from(p));
    }
    let default = Path::new("/tmp/sourcify_dump/sourcify.sqlite");
    if default.exists() {
        Some(default.to_path_buf())
    } else {
        None
    }
}

#[derive(Deserialize)]
struct DecodeRequest {
    chain_id: u64,
    address: String,
    /// Hex string with or without `0x` prefix.
    calldata: String,
    /// Originating wallet RPC method (e.g. `eth_sendTransaction`). When
    /// supplied, gates the Etherscan-API fallback so it only fires for
    /// write/sign methods — read calls and wallet RPCs don't burn the
    /// 5 req/s free-tier budget. When omitted, the fallback is also
    /// skipped (strict default).
    #[serde(default)]
    rpc_method: Option<String>,
}

/// Whether `rpc_method` represents a wallet write or sign operation —
/// the cases where decoding accuracy actually matters because the user
/// is about to authorise a state-changing or signing action. Anything
/// else (read, wallet, connect, unknown) stays inside the local tiers.
fn rpc_method_warrants_etherscan(method: Option<&str>) -> bool {
    let Some(m) = method else { return false };
    match m {
        // Writes — calldata is about to be broadcast as a tx.
        "eth_sendTransaction" | "eth_sendRawTransaction" | "eth_signTransaction" => true,
        // Sign — typed-data / personal_sign / eth_sign all gate authorisations
        // (Permit2, EIP-2612, custom EIP-712).
        "personal_sign" | "eth_sign" => true,
        other if other.starts_with("eth_signTypedData") => true,
        _ => false,
    }
}

#[derive(Serialize)]
struct ApiArg {
    name: String,
    sol_type: String,
    /// Human-readable rendering (decimal for ints, lowercase hex for bytes/addresses).
    value: String,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum DecodeResponse {
    Resolved {
        source: &'static str,
        function_name: String,
        signature: String,
        selector: String,
        args: Vec<ApiArg>,
        /// Recursively decoded sub-calls. Populated when the outer call is one
        /// of the recognised self-call multicall wrappers (recursive); otherwise
        /// empty and omitted from JSON.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        children: Vec<DecodeResponse>,
    },
    NotFound {
        selector: String,
        message: &'static str,
        /// Same shape as on `Resolved` — a `NotFound` outer call cannot be
        /// inspected for sub-calls so this is always empty here, but the
        /// field exists so consumers can treat the variants uniformly.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        children: Vec<DecodeResponse>,
    },
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ApiError { error: msg.into() })).into_response()
}

/// Cap the depth of recursive sub-decoding. Real multicall trees are 1–2 deep; this
/// guards against pathological input.
const MAX_SUBDECODE_DEPTH: u32 = 4;
/// Cap the number of sub-calls per node so a malicious payload can't fan out
/// the response.
const MAX_SUBDECODE_CHILDREN: usize = 64;

/// Identifies which UR family produced the current opcode-stream step,
/// so [`step_to_response`] knows which nested action table to apply when
/// it sees opcode `0x10` — Uniswap UR's `V4_SWAP` (→ V4_ROUTER_TABLE) and
/// Pancake UR's `INFI_SWAP` (→ PANCAKE_INFI_TABLE) collide on the same
/// byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UrKind {
    Uniswap,
    Pancake,
}

/// Pick the right Universal Router opcode table for `(chain, target)` after
/// confirming the selector is one of the public `execute(...)` overloads.
///
/// Returns `Some((kind, table))` only when both:
/// - the selector matches a UR `execute` selector, AND
/// - the target address is on a router-specific allowlist (Uniswap or
///   Pancake).
///
/// Unknown UR-shaped routers (forks not yet registered) get `None` —
/// the orchestrator then leaves the inner `(commands, inputs)` as raw
/// bytes rather than risking a silent misdecode against the wrong opcode
/// table.
fn pick_ur_opcode_table(
    chain_id: u64,
    target: &Address,
    selector: &[u8; 4],
) -> Option<(UrKind, &'static OpcodeTable)> {
    if !is_universal_router_execute(selector) {
        return None;
    }
    if is_uniswap_universal_router(chain_id, target) {
        return Some((UrKind::Uniswap, &UNISWAP_UR_TABLE));
    }
    if is_pancake_universal_router(chain_id, target) {
        return Some((UrKind::Pancake, &PANCAKE_UR_TABLE));
    }
    None
}

async fn decode(State(state): State<AppState>, Json(req): Json<DecodeRequest>) -> Response {
    let address = match Address::from_str(req.address.trim()) {
        Ok(a) => a,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("invalid address: {e}")),
    };
    let stripped = req
        .calldata
        .trim()
        .strip_prefix("0x")
        .unwrap_or(&req.calldata);
    let calldata = match hex::decode(stripped) {
        Ok(b) => b,
        Err(e) => {
            return err(
                StatusCode::BAD_REQUEST,
                format!("invalid calldata hex: {e}"),
            )
        }
    };
    if calldata.len() < 4 {
        return err(
            StatusCode::BAD_REQUEST,
            format!(
                "calldata too short ({} bytes); need at least 4",
                calldata.len()
            ),
        );
    }

    // Etherscan v2 free tier is 5 req/s. Gate the fallback to write/sign
    // RPCs only — read/wallet/connect calls never burn the budget. When
    // `rpc_method` is missing we treat it as "not a write/sign" and skip
    // the fallback (strict default; manual API consumers can opt in by
    // setting `rpc_method` in the request body).
    let etherscan = if rpc_method_warrants_etherscan(req.rpc_method.as_deref()) {
        state.etherscan.as_deref()
    } else {
        None
    };
    let response = decode_recursive(
        state.resolver.as_ref(),
        etherscan,
        req.chain_id,
        &address,
        &calldata,
        0,
    )
    .await;
    Json(response).into_response()
}

/// Resolve `calldata` against the parent target, then if the function is a
/// recognised self-call multicall wrapper, recurse on each `bytes[]` entry up
/// to [`MAX_SUBDECODE_DEPTH`].
///
/// Returns a boxed future so the function can be called recursively from
/// itself and from [`step_to_response`] without `async fn` recursion errors.
fn decode_recursive<'a>(
    resolver: &'a Resolver,
    etherscan: Option<&'a EtherscanClient>,
    chain_id: u64,
    target: &'a Address,
    calldata: &'a [u8],
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = DecodeResponse> + Send + 'a>> {
    Box::pin(async move {
        let selector_hex = format!("0x{}", hex::encode(&calldata[..4.min(calldata.len())]));
        if calldata.len() < 4 {
            return DecodeResponse::NotFound {
                selector: selector_hex,
                message: "calldata shorter than 4-byte selector",
                children: Vec::new(),
            };
        }

        // Try local tiers first; on a miss, fall back to Etherscan when
        // the client is configured. The Etherscan path returns just a
        // `DecodedCall`, so we synthesize a `Resolved` with a synthetic
        // source label for the response shape.
        let resolved = match resolver.resolve(chain_id, target, calldata) {
            ResolveOutcome::Resolved(r) => Some((source_label(r.source), r.decoded)),
            ResolveOutcome::NotFound => {
                if let Some(client) = etherscan {
                    client
                        .try_resolve(chain_id, target, calldata)
                        .await
                        .map(|d| ("etherscan", d))
                } else {
                    None
                }
            }
        };

        let Some((source, decoded)) = resolved else {
            return DecodeResponse::NotFound {
                selector: selector_hex,
                message: "no signature matched in any tier",
                children: Vec::new(),
            };
        };

        let mut selector_bytes = [0u8; 4];
        selector_bytes.copy_from_slice(&calldata[..4]);
        let children = if depth >= MAX_SUBDECODE_DEPTH {
            Vec::new()
        } else if let Some(rule) = lookup_recurse_rule(&selector_bytes) {
            // Recursive — generalized recursion (self-multicall, named-target,
            // address-bytes tuples, fixed-array-of-tuples).
            match extract_children(&decoded, rule, *target) {
                Some(children) => {
                    let mut out = Vec::with_capacity(children.len().min(MAX_SUBDECODE_CHILDREN));
                    for c in children.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        out.push(
                            decode_recursive(
                                resolver,
                                etherscan,
                                chain_id,
                                &c.target,
                                &c.calldata,
                                depth + 1,
                            )
                            .await,
                        );
                    }
                    out
                }
                None => Vec::new(),
            }
        } else if let Some((ur_kind, table)) =
            pick_ur_opcode_table(chain_id, target, &selector_bytes)
        {
            // Opcode dispatch —  each opcode against the matching UR table.
            // Role-gated: Uniswap UR vs Pancake UR vs (unknown UR fork
            // → no dispatch). Avoids silent misdecode where two routers
            // share the `execute(...)` selector but disagree on opcode
            // semantics.
            match extract_commands_and_inputs(&decoded) {
                Some((commands, inputs)) => {
                    let steps = dispatch_opcode_stream(&commands, &inputs, table);
                    let mut out = Vec::with_capacity(steps.len().min(MAX_SUBDECODE_CHILDREN));
                    for step in steps.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        out.push(
                            step_to_response(
                                step,
                                depth + 1,
                                resolver,
                                etherscan,
                                chain_id,
                                ur_kind,
                            )
                            .await,
                        );
                    }
                    out
                }
                None => Vec::new(),
            }
        } else if is_v4_position_manager_modify_liquidities(&selector_bytes)
            && v4_position_manager_address(chain_id).as_ref() == Some(target)
        {
            // V4 PositionManager.modifyLiquidities(...) — outer entrypoint
            // whose unlockData is the same `(bytes actions, bytes[] params)`
            // pair that V4_SWAP carries. Role-gated by the chain's V4 PM
            // address allowlist so we only apply V4_ROUTER_TABLE to known
            // V4 PM deployments.
            match extract_modify_liquidities_actions_and_params(&decoded) {
                Some((actions, params)) => {
                    let steps = dispatch_opcode_stream(&actions, &params, &V4_ROUTER_TABLE);
                    let mut out = Vec::with_capacity(steps.len().min(MAX_SUBDECODE_CHILDREN));
                    for step in steps.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        out.push(
                            step_to_response(
                                step,
                                depth + 1,
                                resolver,
                                etherscan,
                                chain_id,
                                UrKind::Uniswap,
                            )
                            .await,
                        );
                    }
                    out
                }
                None => Vec::new(),
            }
        } else if selector_bytes == MULTISEND_SELECTOR {
            // Safe multiSend(bytes transactions) — packed sub-tx list.
            // Each sub-tx carries its own (to, data) so we recurse via the
            // resolver. Plain ETH transfers (data.len() < 4) are skipped
            // since there is no calldata to decode.
            match extract_transactions_bytes(&decoded) {
                Some(tx_bytes) => {
                    let sub_txs = parse_multisend_transactions(&tx_bytes);
                    let mut out = Vec::with_capacity(sub_txs.len().min(MAX_SUBDECODE_CHILDREN));
                    for sub_tx in sub_txs.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        if sub_tx.data.len() >= 4 {
                            out.push(
                                decode_recursive(
                                    resolver,
                                    etherscan,
                                    chain_id,
                                    &sub_tx.to,
                                    &sub_tx.data,
                                    depth + 1,
                                )
                                .await,
                            );
                        }
                    }
                    out
                }
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        DecodeResponse::Resolved {
            source,
            function_name: decoded.function_name,
            signature: decoded.signature,
            selector: selector_hex,
            args: decoded
                .args
                .into_iter()
                .map(|a| arg_to_api(a, Some(&selector_bytes)))
                .collect(),
            children,
        }
    })
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::Sourcify => "sourcify_curated",
        Source::SourcifyDb => "sourcify_db",
        Source::Openchain => "openchain",
    }
}

fn arg_to_api(a: DecodedArg, selector: Option<&[u8; 4]>) -> ApiArg {
    ApiArg {
        value: format_value_with_packed(&a.value, &a.components, &a.name, selector),
        name: a.name,
        sol_type: a.sol_type,
    }
}

/// Render a decoded value, with sub-format enrichment for known non-standard
/// payloads. Currently:
///
/// - **packed**: a `bytes` arg (or tuple field) named `path` is parsed as a
///   V3 packed `[token20][fee3][token20]…` path and rendered as a friendly
///   `0x… --[fee=N]--> 0x…` chain when it parses. [`format_packed_path`]
///   returns `None` for bytes that don't satisfy the strict V3 path length
///   constraint, so non-V3 bytes named `path` fall back to raw hex.
///
/// - **enum-tagged**: a `bytes` arg named `userData` inside the Balancer V2
///   `joinPool` / `exitPool` calls is decoded against the protocol's
///   `JoinKind` / `ExitKind` table — pool-type-aware (`Weighted` / `Stable`)
///   for the exit case via `try_dispatch`.
///
/// Recurses into tuples and arrays so the enrichment fires regardless of
/// nesting depth.
fn format_value_with_packed(
    value: &DynSolValue,
    components: &[Param],
    parent_name: &str,
    selector_ctx: Option<&[u8; 4]>,
) -> String {
    // V3 packed path detection.
    if parent_name == "path" {
        if let DynSolValue::Bytes(bytes) = value {
            if let Some(friendly) = format_packed_path(bytes) {
                return friendly;
            }
        }
    }

    // Balancer V2 userData enum-tagged decode.
    if parent_name == "userData" {
        if let (Some(sel), DynSolValue::Bytes(bytes)) = (selector_ctx, value) {
            let tables: &[&_] = match *sel {
                JOIN_POOL_SELECTOR => &[&BALANCER_V2_JOIN_KIND],
                EXIT_POOL_SELECTOR => &[
                    &BALANCER_V2_EXIT_KIND_WEIGHTED,
                    &BALANCER_V2_EXIT_KIND_STABLE,
                ],
                _ => &[],
            };
            if !tables.is_empty() {
                if let Some(decoded) = enum_try_dispatch(bytes, tables) {
                    return format_decoded_enum(&decoded);
                }
            }
        }
    }

    match value {
        DynSolValue::Tuple(items) => {
            let inner: Vec<String> = items
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let child = components.get(i);
                    let child_components = child.map(|c| c.components.as_slice()).unwrap_or(&[]);
                    let child_name = child.map(|c| c.name.as_str()).unwrap_or("");
                    let formatted =
                        format_value_with_packed(v, child_components, child_name, selector_ctx);
                    match child {
                        Some(c) if !c.name.is_empty() => format!("{}: {}", c.name, formatted),
                        _ => formatted,
                    }
                })
                .collect();
            format!("({})", inner.join(", "))
        }
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => {
            // Element type metadata is on the parent param itself; pass an
            // empty parent name so we don't accidentally treat the array's
            // own name as if it applied to each element.
            let inner: Vec<String> = items
                .iter()
                .map(|v| format_value_with_packed(v, components, "", selector_ctx))
                .collect();
            format!("[{}]", inner.join(", "))
        }
        // For everything else, the alloy-aware formatter is fine.
        _ => format_value_named(value, components),
    }
}

/// Render a decoded enum-tagged payload as `KIND_NAME(field1=value1, …)`.
/// Used by [`format_value_with_packed`] when it identifies a known userData
/// blob (Balancer V2 join/exit kinds today).
fn format_decoded_enum(decoded: &DecodedEnum) -> String {
    // Skip the leading `kind` field — it's already part of the prefix.
    let body: Vec<String> = decoded
        .args
        .iter()
        .skip(1)
        .map(|a| format!("{}={}", a.name, format_value_named(&a.value, &a.components)))
        .collect();
    if body.is_empty() {
        format!("{} ({})", decoded.kind_name, decoded.table_name)
    } else {
        format!(
            "{}({})  // {}",
            decoded.kind_name,
            body.join(", "),
            decoded.table_name,
        )
    }
}

/// Convert a opcode step into a synthetic `DecodeResponse::Resolved`
/// so the existing tree renderer can display it inline with real ABI calls.
///
/// `source = "ur_command"` flags it as a synthesised entry rather than a
/// signature-DB hit. When the step's input couldn't be ABI-decoded (unknown
/// opcode, no schema yet, decode failure), we surface the raw input as a
/// single fake arg so users still see the bytes.
///
/// Nested-decode cases:
/// - `V4_SWAP` (UR opcode `0x10`) re-dispatches its inner
///   `(bytes actions, bytes[] params)` against [`V4_ROUTER_TABLE`].
/// - `EXECUTE_SUB_PLAN` (UR opcode `0x21`) re-dispatches the inner
///   `(bytes commands, bytes[] inputs)` against the same Uniswap UR table.
/// - `V3_POSITION_MANAGER_PERMIT` / `V3_POSITION_MANAGER_CALL` (UR opcodes
///   `0x11` / `0x12`) — input is a complete calldata for the V3 NPM
///   contract; we recurse via the resolver against the chain's NPM address
///   so the call is decoded against the NPM ABI.
/// - `V4_POSITION_MANAGER_CALL` (UR opcode `0x14`) — same as above against
///   the V4 PositionManager.
fn step_to_response<'a>(
    step: DecodedStep,
    depth: u32,
    resolver: &'a Resolver,
    etherscan: Option<&'a EtherscanClient>,
    chain_id: u64,
    ur_kind: UrKind,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = DecodeResponse> + Send + 'a>> {
    Box::pin(async move {
        let selector = format!("0x{:02x}", step.opcode);
        let function_name = if step.allow_revert {
            format!("{} (allowRevert)", step.name)
        } else {
            step.name.to_string()
        };

        // Compute nested children before consuming `step.args` below.
        let nested_children = if depth >= MAX_SUBDECODE_DEPTH {
            Vec::new()
        } else if step.opcode == 0x10 {
            // Both UR families dispatch on opcode 0x10 but to different action
            // tables: Uniswap UR's V4_SWAP → V4_ROUTER_TABLE; Pancake UR's
            // INFI_SWAP → PANCAKE_INFI_TABLE.
            let (extracted, table) = match ur_kind {
                UrKind::Uniswap => (extract_v4_actions_and_params(&step), &V4_ROUTER_TABLE),
                UrKind::Pancake => (extract_infi_actions_and_params(&step), &PANCAKE_INFI_TABLE),
            };
            match extracted {
                Some((actions, params)) => {
                    let sub_steps = dispatch_opcode_stream(&actions, &params, table);
                    let mut out = Vec::with_capacity(sub_steps.len().min(MAX_SUBDECODE_CHILDREN));
                    for s in sub_steps.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        out.push(
                            step_to_response(s, depth + 1, resolver, etherscan, chain_id, ur_kind)
                                .await,
                        );
                    }
                    out
                }
                None => Vec::new(),
            }
        } else if step.opcode == 0x21 {
            // EXECUTE_SUB_PLAN — same `(bytes commands, bytes[] inputs)` shape as
            // the outer execute(...) entrypoint, dispatched recursively against
            // the *same* UR table family the parent step came from.
            let table = match ur_kind {
                UrKind::Uniswap => &UNISWAP_UR_TABLE,
                UrKind::Pancake => &PANCAKE_UR_TABLE,
            };
            match extract_v4_actions_and_params(&step) {
                Some((commands, inputs)) => {
                    let sub_steps = dispatch_opcode_stream(&commands, &inputs, table);
                    let mut out = Vec::with_capacity(sub_steps.len().min(MAX_SUBDECODE_CHILDREN));
                    for s in sub_steps.into_iter().take(MAX_SUBDECODE_CHILDREN) {
                        out.push(
                            step_to_response(s, depth + 1, resolver, etherscan, chain_id, ur_kind)
                                .await,
                        );
                    }
                    out
                }
                None => Vec::new(),
            }
        } else if matches!(step.opcode, 0x11 | 0x12 | 0x14) && !step.raw_input.is_empty() {
            // V3 / V4 PositionManager call — input is full calldata that the UR
            // contract dispatches via `address(NPM/PM).call(inputs)`. Recurse
            // against the chain-specific PositionManager address so the resolver
            // matches the NPM/PM ABI rather than UR's.
            let pm_addr = if step.opcode == 0x14 {
                v4_position_manager_address(chain_id)
            } else {
                v3_position_manager_address(chain_id)
            };
            match pm_addr {
                Some(addr) => vec![
                    decode_recursive(
                        resolver,
                        etherscan,
                        chain_id,
                        &addr,
                        &step.raw_input,
                        depth + 1,
                    )
                    .await,
                ],
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };

        let (signature, args) = match (step.args, step.error) {
            (Some(decoded_args), _) => (
                format!("{}(...)", step.name),
                decoded_args
                    .into_iter()
                    .map(|a| step_arg_to_api(step.opcode, a))
                    .collect(),
            ),
            (None, Some(StepDecodeError::UnknownOpcode)) => (
                format!("UNKNOWN(raw 0x{} bytes)", hex::encode(&step.raw_input)),
                vec![raw_input_arg(&step.raw_input)],
            ),
            (None, Some(StepDecodeError::NoSchema)) => (
                format!("{}(... schema TBD)", step.name),
                vec![raw_input_arg(&step.raw_input)],
            ),
            (None, Some(StepDecodeError::AbiDecode(msg))) => (
                format!("{}(... ABI decode failed: {msg})", step.name),
                vec![raw_input_arg(&step.raw_input)],
            ),
            (None, Some(StepDecodeError::BadSignature(msg))) => (
                format!("{}(bad table signature: {msg})", step.name),
                vec![raw_input_arg(&step.raw_input)],
            ),
            (None, None) => (
                format!("{}(...)", step.name),
                vec![raw_input_arg(&step.raw_input)],
            ),
        };
        DecodeResponse::Resolved {
            source: "ur_command",
            function_name,
            signature,
            selector,
            args,
            children: nested_children,
        }
    })
}

fn raw_input_arg(input: &[u8]) -> ApiArg {
    ApiArg {
        name: "raw_input".into(),
        sol_type: "bytes".into(),
        value: format!("0x{}", hex::encode(input)),
    }
}

/// Same as [`arg_to_api`], but with awareness of which opcode produced the
/// step. Used to enrich specific args that carry non-standard encodings
/// (e.g. the `path` bytes on V3_SWAP_EXACT_IN / V3_SWAP_EXACT_OUT, which is
/// a packed token-fee-token sequence — Cat C1).
fn step_arg_to_api(opcode: u8, a: DecodedArg) -> ApiArg {
    // V3_SWAP_EXACT_IN / V3_SWAP_EXACT_OUT both carry a packed `path` bytes
    // arg. When we recognise it, surface the friendly token-fee chain.
    if matches!(opcode, 0x00 | 0x01) && a.name == "path" {
        if let DynSolValue::Bytes(ref bytes) = a.value {
            if let Some(friendly) = format_packed_path(bytes) {
                return ApiArg {
                    name: a.name,
                    sol_type: a.sol_type,
                    value: friendly,
                };
            }
        }
    }
    // Opcode steps don't carry a parent function selector context — they
    // ARE the dispatch context. Pass `None` so format_value_with_packed
    // skips selector-keyed lookups (Balancer userData, etc.).
    arg_to_api(a, None)
}

#[derive(Serialize)]
struct Health {
    ok: bool,
}

async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

/// Receive an extracted RPC event from the userscript and broadcast it to
/// any SSE subscribers (the React frontend).
///
/// The body is forwarded through as-is; we only validate that it parses as
/// JSON so SSE consumers don't see garbage.
async fn post_event(State(state): State<AppState>, body: String) -> Response {
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&body) {
        return err(StatusCode::BAD_REQUEST, format!("invalid JSON: {e}"));
    }
    let _ = state.event_tx.send(body); // err only when no subscribers
    StatusCode::NO_CONTENT.into_response()
}

/// Server-Sent Events stream of broadcast RPC events.
async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(payload) => Some(Ok::<_, Infallible>(Event::default().data(payload))),
        // Lagged: subscriber fell behind — drop the missed event silently.
        Err(_) => None,
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn frontend_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FRONTEND_DIST") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/dist");
    if here.exists() {
        Some(here)
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "web_server=info,tower_http=info".into()),
        )
        .init();

    let (event_tx, _) = broadcast::channel::<String>(64);
    let etherscan = EtherscanClient::from_env().map(Arc::new);
    if etherscan.is_some() {
        tracing::info!("Etherscan v2 fallback enabled (ETHERSCAN_API_KEY present)");
    } else {
        tracing::info!(
            "ETHERSCAN_API_KEY not set — Etherscan fallback disabled (decoder will only use local tiers)"
        );
    }
    let state = AppState {
        resolver: Arc::new(build_resolver()),
        etherscan,
        event_tx,
        route_registries: Arc::new(request_router::DefaultRegistries::standard()),
    };

    let mut app = Router::new()
        .route("/api/decode", post(decode))
        .route("/api/sign", post(decode_sign))
        .route("/api/route", post(route))
        .route("/api/event", post(post_event))
        .route("/api/event/stream", get(event_stream))
        .route("/api/health", get(health))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    if let Some(dir) = frontend_dir() {
        tracing::info!("serving frontend build from {}", dir.display());
        app = app.fallback_service(ServeDir::new(dir));
    } else {
        tracing::info!(
            "frontend/dist not found — only API endpoints exposed (run `npm run dev` separately)"
        );
    }

    let addr_str = std::env::var("WEB_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let addr: SocketAddr = addr_str
        .parse()
        .expect("WEB_SERVER_ADDR must be a host:port");
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}
