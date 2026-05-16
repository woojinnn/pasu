use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

const RESOLUTION_CACHE: &str = "public, max-age=300";

pub async fn handle(
    Path((chain, address)): Path<(u64, String)>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let bytes = match parse_address(&address) {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "malformed address").into_response(),
    };
    let Some((name, version)) = state.index.lookup(chain, bytes) else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CACHE_CONTROL, HeaderValue::from_static(RESOLUTION_CACHE))],
            Json(json!({ "error": "no adapter registered" })),
        )
            .into_response();
    };
    let manifest = match state.storage.read_manifest(&name, &version).await {
        Ok(m) => m,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        [(header::CACHE_CONTROL, HeaderValue::from_static(RESOLUTION_CACHE))],
        Json(json!({
            "manifest": manifest,
            "wasm_url": format!("/packages/{name}/v{version}/adapter.wasm"),
            "version": version,
            "sdk_version": manifest.sdk_version,
        })),
    )
        .into_response()
}

fn parse_address(s: &str) -> Option<[u8; 20]> {
    let stripped = s.strip_prefix("0x")?;
    if stripped.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    hex::decode_to_slice(stripped, &mut out).ok()?;
    Some(out)
}
