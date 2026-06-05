//! Google OAuth 2.0 login.
//!
//! Two endpoints, both unauthenticated:
//! - `GET /auth/google` — redirects the browser to Google's authorize URL.
//! - `GET /auth/google/callback?code=…&state=…` — receives the code,
//!   exchanges it for an `id_token`, decodes the email, upserts the user
//!   in [`GlobalDb`], mints a JWT pair, and 302s back to the dashboard
//!   with the access token in the URL fragment.
//!
//! The callback intentionally uses a URL **fragment** (`#access_token=…`)
//! rather than a query string so the token never reaches the server logs
//! of the dashboard host (browsers strip fragments from `Referer`).
//!
//! State token (CSRF): a short random string is base64-urlsafe encoded and
//! signed into a brief-lived JWT to avoid server-side session storage.
//! Google echoes it back; we re-verify on the callback. This trades a
//! tiny bit of complexity for stateless deploy.

use std::env;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use policy_db::GlobalDb;

use crate::auth::jwt::{self, TokenType};

/// `GET /auth/google` — bounce the user to Google's consent screen.
/// All config (`GOOGLE_CLIENT_ID`, `GOOGLE_REDIRECT_URI`) read at request
/// time so a missing env var surfaces as a clear 500 rather than a
/// startup-time panic.
pub async fn start_google_login() -> Response {
    let Ok(client_id) = env::var("GOOGLE_CLIENT_ID") else {
        return env_missing("GOOGLE_CLIENT_ID");
    };
    let Ok(redirect_uri) = env::var("GOOGLE_REDIRECT_URI") else {
        return env_missing("GOOGLE_REDIRECT_URI");
    };
    let state = match jwt::issue("oauth-state", "oauth-state", TokenType::Access, Some(300)) {
        Ok(s) => s,
        Err(e) => return server_error(&format!("state token: {e}")),
    };

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?response_type=code\
         &client_id={client_id}\
         &redirect_uri={redirect_uri}\
         &scope=openid+email\
         &state={state}\
         &access_type=online\
         &prompt=select_account",
        client_id = urlencoding::encode(&client_id),
        redirect_uri = urlencoding::encode(&redirect_uri),
        state = urlencoding::encode(&state),
    );
    Redirect::to(&url).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResp {
    id_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /auth/google/callback?code=…&state=…` — finish the OAuth dance.
pub async fn google_callback(
    State(global): State<GlobalDb>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    if let Some(err) = q.error {
        return user_error(&format!("Google denied login: {err}"));
    }
    let Some(code) = q.code else {
        return user_error("missing `code` parameter");
    };
    let Some(state) = q.state else {
        return user_error("missing `state` parameter");
    };
    // Verify CSRF state. Don't care about the claims, only that it's
    // ours and recent.
    if jwt::verify(&state).is_err() {
        return user_error("invalid or expired `state`");
    }

    let id_token = match exchange_code_for_id_token(&code).await {
        Ok(t) => t,
        Err(e) => return server_error(&format!("token exchange failed: {e}")),
    };
    let email = match decode_email_from_id_token(&id_token) {
        Ok(e) => e,
        Err(e) => return server_error(&format!("id_token decode: {e}")),
    };

    let user_id = match global.upsert_user(&email, "google").await {
        Ok(id) => id,
        Err(e) => return server_error(&format!("upsert_user: {e}")),
    };

    let access = match jwt::issue(&user_id, &email, TokenType::Access, None) {
        Ok(t) => t,
        Err(e) => return server_error(&format!("issue access: {e}")),
    };
    let refresh = match jwt::issue(&user_id, &email, TokenType::Refresh, None) {
        Ok(t) => t,
        Err(e) => return server_error(&format!("issue refresh: {e}")),
    };

    // Dashboard URL is configurable so the same server can serve a dev
    // dashboard (`127.0.0.1:5173`) or prod (`app.scopeball.com`).
    let dashboard = env::var("DASHBOARD_URL").unwrap_or_else(|_| "http://127.0.0.1:5173".into());
    let target = format!(
        "{dashboard}/auth/callback#access_token={access}&refresh_token={refresh}",
        access = urlencoding::encode(&access),
        refresh = urlencoding::encode(&refresh),
    );
    Redirect::to(&target).into_response()
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// `POST /auth/refresh` — rotate a refresh token and mint a new access
/// token. Token revocation can be layered underneath the same endpoint once
/// distributed session storage is introduced.
pub async fn refresh_token(Json(req): Json<RefreshRequest>) -> Response {
    let claims = match jwt::verify(&req.refresh_token) {
        Ok(c) if c.is_refresh() => c,
        Ok(_) => return user_error("access token cannot refresh a session"),
        Err(e) => return user_error(&format!("invalid refresh token: {e}")),
    };

    let access = match jwt::issue(&claims.sub, &claims.email, TokenType::Access, None) {
        Ok(t) => t,
        Err(e) => return server_error(&format!("issue access: {e}")),
    };
    let refresh = match jwt::issue(&claims.sub, &claims.email, TokenType::Refresh, None) {
        Ok(t) => t,
        Err(e) => return server_error(&format!("issue refresh: {e}")),
    };

    Json(json!({
        "access_token": access,
        "refresh_token": refresh,
    }))
    .into_response()
}

// ---------- internals ----------

/// POST `code` to Google's token endpoint, extract the `id_token`.
async fn exchange_code_for_id_token(code: &str) -> Result<String, String> {
    let client_id =
        env::var("GOOGLE_CLIENT_ID").map_err(|_| "GOOGLE_CLIENT_ID unset".to_string())?;
    let client_secret =
        env::var("GOOGLE_CLIENT_SECRET").map_err(|_| "GOOGLE_CLIENT_SECRET unset".to_string())?;
    let redirect_uri =
        env::var("GOOGLE_REDIRECT_URI").map_err(|_| "GOOGLE_REDIRECT_URI unset".to_string())?;

    let body = [
        ("code", code),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
        ("redirect_uri", &redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    let resp: TokenResp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("JSON decode: {e}"))?;
    if let Some(err) = resp.error {
        return Err(format!(
            "{err}: {}",
            resp.error_description.unwrap_or_default()
        ));
    }
    resp.id_token.ok_or_else(|| "id_token missing".into())
}

/// Pull the `email` field out of a Google `id_token` (a JWT signed by
/// Google). We trust the token because we just received it directly from
/// Google's token endpoint over HTTPS — no need to re-verify the
/// signature. (For higher assurance, switch to verifying against
/// `https://www.googleapis.com/oauth2/v3/certs`.)
fn decode_email_from_id_token(id_token: &str) -> Result<String, String> {
    use base64::Engine;
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return Err("id_token is not 3 segments".into());
    }
    let payload_b64 = parts[1];
    let payload_json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| format!("base64 decode: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&payload_json).map_err(|e| format!("json decode: {e}"))?;
    let email = value
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "email missing from id_token".to_string())?;
    Ok(email.to_string())
}

// ---------- error helpers ----------

fn env_missing(var: &str) -> Response {
    server_error(&format!("server misconfigured: env {var} not set"))
}

fn server_error(reason: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "server_error", "reason": reason })),
    )
        .into_response()
}

fn user_error(reason: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "bad_request", "reason": reason })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn encode_id_token(payload: &serde_json::Value) -> String {
        let header = "eyJhbGciOiJSUzI1NiJ9"; // {"alg":"RS256"} — not validated by us
        let payload_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        format!("{header}.{payload_b64}.signature-not-verified")
    }

    #[test]
    fn decode_email_happy_path() {
        let tok = encode_id_token(&json!({
            "email": "alice@example.com",
            "email_verified": true,
            "sub": "1234567890"
        }));
        let email = decode_email_from_id_token(&tok).unwrap();
        assert_eq!(email, "alice@example.com");
    }

    #[test]
    fn decode_email_missing_field_errors() {
        let tok = encode_id_token(&json!({ "sub": "1234567890" }));
        let err = decode_email_from_id_token(&tok).unwrap_err();
        assert!(err.contains("email missing"));
    }

    #[test]
    fn decode_email_malformed_token_errors() {
        let err = decode_email_from_id_token("not.a.jwt.token").unwrap_err();
        assert!(err.contains("not 3 segments"), "got: {err}");
    }
}
