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

fn seed_signatures() -> &'static [([u8; 4], &'static str)] {
    &[
        ([0x09, 0x5e, 0xa7, 0xb3], "approve(address,uint256)"),
        ([0xa9, 0x05, 0x9c, 0xbb], "transfer(address,uint256)"),
        (
            [0x23, 0xb8, 0x72, 0xdd],
            "transferFrom(address,address,uint256)",
        ),
        ([0x70, 0xa0, 0x82, 0x31], "balanceOf(address)"),
        (
            [0x41, 0x4b, 0xf3, 0x89],
            "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
        ),
        (
            [0xc0, 0x4b, 0x8d, 0x59],
            "exactInput((bytes,address,uint256,uint256,uint256))",
        ),
        ([0xac, 0x96, 0x50, 0xd8], "multicall(bytes[])"),
        ([0x5a, 0xe4, 0x01, 0xdc], "multicall(uint256,bytes[])"),
        (
            [0x38, 0xed, 0x17, 0x39],
            "swapExactTokensForTokens(uint256,uint256,address[],address,uint256)",
        ),
        (
            [0x7f, 0xf3, 0x6a, 0xb5],
            "swapExactETHForTokens(uint256,address[],address,uint256)",
        ),
        (
            [0xb6, 0xf9, 0xde, 0x95],
            "swapExactETHForTokensSupportingFeeOnTransferTokens(uint256,address[],address,uint256)",
        ),
        (
            [0x79, 0x1a, 0xc9, 0x47],
            "swapExactTokensForETHSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)",
        ),
        ([0x24, 0x85, 0x6b, 0xc3], "execute(bytes,bytes[])"),
        (
            [0x35, 0x93, 0x56, 0x4c],
            "execute(bytes,bytes[],uint256)",
        ),
        (
            [0x61, 0x7b, 0xa0, 0x37],
            "supply(address,uint256,address,uint16)",
        ),
        ([0x69, 0x32, 0x8d, 0xec], "withdraw(address,uint256,address)"),
        (
            [0xa4, 0x15, 0xbc, 0xad],
            "borrow(address,uint256,uint256,uint16,address)",
        ),
        (
            [0x57, 0x3a, 0xde, 0x81],
            "repay(address,uint256,uint256,address)",
        ),
        (
            [0x37, 0x4f, 0x43, 0x5d],
            "multicall((address,bytes,uint256,bool,bytes32)[])",
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
            let children = if depth < MAX_SUBDECODE_DEPTH && is_self_multicall(&selector_bytes) {
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
