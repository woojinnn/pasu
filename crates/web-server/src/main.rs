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
use abi_resolver::subdecode::opcode_stream::{
    dispatch as dispatch_opcode_stream, DecodedStep, StepDecodeError,
};
use abi_resolver::subdecode::protocols::universal_router::{
    extract_commands_and_inputs, is_universal_router_execute, UNISWAP_UR_TABLE,
};
use abi_resolver::subdecode::protocols::v4_router::{
    extract_actions_and_params as extract_v4_actions_and_params, V4_ROUTER_TABLE,
};
use abi_resolver::subdecode::recurse::{extract_subcalls, is_self_multicall};
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

const SOURCIFY_BUNDLE: &[u8] = include_bytes!("../../abi-resolver/data/sourcify.json");

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
    ]
}

#[derive(Clone)]
struct AppState {
    resolver: Arc<Resolver>,
    event_tx: broadcast::Sender<String>,
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
        /// of the recognised self-call multicall wrappers (Cat A); otherwise
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

/// Cap the depth of Cat A recursion. Real multicall trees are 1–2 deep; this
/// guards against pathological input.
const MAX_SUBDECODE_DEPTH: u32 = 4;
/// Cap the number of sub-calls per node so a malicious payload can't fan out
/// the response.
const MAX_SUBDECODE_CHILDREN: usize = 64;

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

    let response = decode_recursive(
        state.resolver.as_ref(),
        req.chain_id,
        &address,
        &calldata,
        0,
    );
    Json(response).into_response()
}

/// Resolve `calldata` against the parent target, then if the function is a
/// recognised self-call multicall wrapper, recurse on each `bytes[]` entry up
/// to [`MAX_SUBDECODE_DEPTH`].
fn decode_recursive(
    resolver: &Resolver,
    chain_id: u64,
    target: &Address,
    calldata: &[u8],
    depth: u32,
) -> DecodeResponse {
    let selector_hex = format!("0x{}", hex::encode(&calldata[..4.min(calldata.len())]));
    if calldata.len() < 4 {
        return DecodeResponse::NotFound {
            selector: selector_hex,
            message: "calldata shorter than 4-byte selector",
            children: Vec::new(),
        };
    }

    let outcome = resolver.resolve(chain_id, target, calldata);
    match outcome {
        ResolveOutcome::Resolved(r) => {
            let mut selector_bytes = [0u8; 4];
            selector_bytes.copy_from_slice(&calldata[..4]);
            let children = if depth >= MAX_SUBDECODE_DEPTH {
                Vec::new()
            } else if is_self_multicall(&selector_bytes) {
                // Cat A — recurse on each entry of bytes[].
                extract_subcalls(&r.decoded)
                    .map(|subs| {
                        subs.into_iter()
                            .take(MAX_SUBDECODE_CHILDREN)
                            .map(|sub| {
                                decode_recursive(resolver, chain_id, target, &sub, depth + 1)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else if is_universal_router_execute(&selector_bytes) {
                // Cat B — dispatch each opcode against the Uniswap UR table.
                extract_commands_and_inputs(&r.decoded)
                    .map(|(commands, inputs)| {
                        let steps = dispatch_opcode_stream(&commands, &inputs, &UNISWAP_UR_TABLE);
                        steps
                            .into_iter()
                            .take(MAX_SUBDECODE_CHILDREN)
                            .map(|step| step_to_response(step, depth + 1))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            DecodeResponse::Resolved {
                source: source_label(r.source),
                function_name: r.decoded.function_name,
                signature: r.decoded.signature,
                selector: selector_hex,
                args: r.decoded.args.into_iter().map(arg_to_api).collect(),
                children,
            }
        }
        ResolveOutcome::NotFound => DecodeResponse::NotFound {
            selector: selector_hex,
            message: "no signature matched in any tier",
            children: Vec::new(),
        },
    }
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::Sourcify => "sourcify_curated",
        Source::SourcifyDb => "sourcify_db",
        Source::Openchain => "openchain",
    }
}

fn arg_to_api(a: DecodedArg) -> ApiArg {
    ApiArg {
        // Render with the parameter's component descriptors so tuple fields
        // surface as `(fieldName: value, …)` instead of `(value, value, …)`.
        value: format_value_named(&a.value, &a.components),
        name: a.name,
        sol_type: a.sol_type,
    }
}

/// Convert a Cat B opcode step into a synthetic `DecodeResponse::Resolved`
/// so the existing tree renderer can display it inline with real ABI calls.
///
/// `source = "ur_command"` flags it as a synthesised entry rather than a
/// signature-DB hit. When the step's input couldn't be ABI-decoded (unknown
/// opcode, no schema yet, decode failure), we surface the raw input as a
/// single fake arg so users still see the bytes.
///
/// When the step is `V4_SWAP` (UR opcode `0x10`) and was decoded against the
/// `(bytes actions, bytes[] params)` schema, we recurse one more level using
/// the V4Router action table so the inner SWAP_EXACT_IN / SETTLE / TAKE
/// sequence shows up as nested children in the tree.
fn step_to_response(step: DecodedStep, depth: u32) -> DecodeResponse {
    let selector = format!("0x{:02x}", step.opcode);
    let function_name = if step.allow_revert {
        format!("{} (allowRevert)", step.name)
    } else {
        step.name.to_string()
    };

    // Compute V4_SWAP nested children before consuming `step.args` below.
    let nested_children = if step.opcode == 0x10 && depth < MAX_SUBDECODE_DEPTH {
        extract_v4_actions_and_params(&step)
            .map(|(actions, params)| {
                let v4_steps = dispatch_opcode_stream(&actions, &params, &V4_ROUTER_TABLE);
                v4_steps
                    .into_iter()
                    .take(MAX_SUBDECODE_CHILDREN)
                    .map(|s| step_to_response(s, depth + 1))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let (signature, args) = match (step.args, step.error) {
        (Some(decoded_args), _) => (
            format!("{}(...)", step.name),
            decoded_args.into_iter().map(arg_to_api).collect(),
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
}

fn raw_input_arg(input: &[u8]) -> ApiArg {
    ApiArg {
        name: "raw_input".into(),
        sol_type: "bytes".into(),
        value: format!("0x{}", hex::encode(input)),
    }
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
    let state = AppState {
        resolver: Arc::new(build_resolver()),
        event_tx,
    };

    let mut app = Router::new()
        .route("/api/decode", post(decode))
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
