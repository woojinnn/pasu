use crate::manifest::Manifest;
use crate::AppState;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn handle(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let mut manifest_raw: Option<String> = None;
    let mut wasm: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(&format!("multipart error: {e}")))?
    {
        match field.name() {
            Some("manifest") => {
                manifest_raw = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| bad_request(&format!("manifest text: {e}")))?,
                );
            }
            Some("wasm") => {
                wasm = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| bad_request(&format!("wasm bytes: {e}")))?
                        .to_vec(),
                );
            }
            other => {
                return Err(bad_request(&format!(
                    "unexpected field: {}",
                    other.unwrap_or("<unnamed>")
                )));
            }
        }
    }

    let manifest_raw = manifest_raw.ok_or_else(|| bad_request("missing manifest"))?;
    let wasm = wasm.ok_or_else(|| bad_request("missing wasm"))?;

    let manifest: Manifest = serde_json::from_str(&manifest_raw)
        .map_err(|e| bad_request(&format!("manifest parse: {e}")))?;
    manifest
        .validate()
        .map_err(|e| bad_request(&format!("manifest invalid: {e}")))?;

    if !wasm.starts_with(b"\0asm") {
        return Err(bad_request("wasm header missing"));
    }

    state
        .storage
        .write_version(&manifest, &wasm)
        .await
        .map_err(|e| internal(&format!("storage write: {e}")))?;

    state.index.add(&manifest);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "name": manifest.name,
            "version": manifest.version,
            "urls": {
                "wasm": format!("/packages/{}/v{}/adapter.wasm", manifest.name, manifest.version),
                "manifest": format!("/packages/{}/v{}/manifest.json", manifest.name, manifest.version),
            }
        })),
    ))
}

fn bad_request(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn internal(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": msg })))
}
