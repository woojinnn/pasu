//! axum middleware that turns `Authorization: Bearer <jwt>` into an
//! [`AuthUser`] every protected handler can extract.
//! Wire it once at the router builder (`.layer(from_fn(require_auth))`)
//! and protected handlers add `Extension(user): Extension<AuthUser>` to
//! their signature. Missing / invalid / expired tokens short-circuit with
//! `401 Unauthorized` and a small JSON body.
//! The middleware does NOT touch the DB — it only validates the token.
//! Mapping `user_id → DB store` happens in the handler via the
//! `MultiUserStore` carried in `AppState`.

use axum::extract::Request;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::auth::jwt::{self, Claims};

/// Trimmed identity carried through every authorised request.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: String,
    pub email: String,
}

impl From<Claims> for AuthUser {
    fn from(c: Claims) -> Self {
        Self {
            user_id: c.sub,
            email: c.email,
        }
    }
}

/// `axum::middleware::from_fn(require_auth)` — wraps a route so every
/// downstream handler can rely on `Extension<AuthUser>` being present.
/// Token resolution order:
/// 1. `Authorization: Bearer <jwt>` header (preferred — used by all
///    standard fetch/HTTP clients).
/// 2. `?token=<jwt>` query string (fallback — the browser `EventSource`
///    API cannot set custom headers, so SSE callers pass the token here).
pub async fn require_auth(mut req: Request, next: Next) -> Response {
    let user = match extract_user(&req) {
        Ok(u) => u,
        Err(resp) => return *resp,
    };
    req.extensions_mut().insert(user);
    next.run(req).await
}

/// Pulls the bearer token out of either `Authorization` or `?token=…` and
/// verifies it. Only access tokens are accepted — refresh tokens reach
/// `/auth/refresh` instead.
fn extract_user(req: &Request) -> Result<AuthUser, Box<Response>> {
    // Prefer the header; fall back to query string for SSE callers.
    let token = match token_from_header(req.headers()) {
        Some(Ok(t)) => t,
        Some(Err(resp)) => return Err(Box::new(resp)),
        None => match token_from_query(req.uri().query()) {
            Some(t) => t,
            None => {
                return Err(Box::new(reject(
                    "missing Authorization header and `token` query param",
                )))
            }
        },
    };

    let claims = jwt::verify(&token).map_err(|e| Box::new(reject(&e.to_string())))?;
    if !claims.is_access() {
        return Err(Box::new(reject(
            "refresh token cannot be used as an access token",
        )));
    }
    Ok(AuthUser::from(claims))
}

/// Returns `Some(Ok(token))` if `Authorization` is set, `Some(Err)` if
/// it's set but malformed (so we can surface the right reason), or
/// `None` if absent (fall through to the query string).
fn token_from_header(headers: &HeaderMap) -> Option<Result<String, Response>> {
    let raw = headers.get(header::AUTHORIZATION)?;
    Some((|| {
        let s = raw
            .to_str()
            .map_err(|_| reject("Authorization header is not valid UTF-8"))?;
        s.strip_prefix("Bearer ")
            .map(String::from)
            .ok_or_else(|| reject("Authorization header must start with `Bearer `"))
    })())
}

/// Pull `token=<jwt>` out of the query string. Returns `None` if the
/// query is absent or no `token` key is found. URL-decoded.
fn token_from_query(query: Option<&str>) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        if let Some(rest) = pair.strip_prefix("token=") {
            return Some(urlencoding::decode(rest).ok()?.into_owned());
        }
    }
    None
}

fn reject(reason: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized", "reason": reason })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::HeaderValue;

    use crate::auth::jwt::{issue, TokenType};

    fn set_secret() {
        std::env::set_var(
            "JWT_SECRET",
            "test-secret-only-do-not-use-in-production-2026-05-31",
        );
    }

    fn req_with(headers: HeaderMap, uri: &str) -> Request {
        let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        *req.headers_mut() = headers;
        req
    }

    #[test]
    fn missing_token_rejected() {
        let req = req_with(HeaderMap::new(), "/wallets");
        let err = extract_user(&req).unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn malformed_header_rejected() {
        set_secret();
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Token abc.def.ghi"),
        );
        let err = extract_user(&req_with(h, "/wallets")).unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn valid_access_token_yields_user() {
        set_secret();
        let token = issue("u_abc", "a@e.com", TokenType::Access, None).unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        let user = extract_user(&req_with(h, "/wallets")).unwrap();
        assert_eq!(user.user_id, "u_abc");
        assert_eq!(user.email, "a@e.com");
    }

    #[test]
    fn refresh_token_rejected_as_access() {
        set_secret();
        let token = issue("u_abc", "a@e.com", TokenType::Refresh, None).unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        let err = extract_user(&req_with(h, "/wallets")).unwrap_err();
        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn token_via_query_string_accepted() {
        set_secret();
        let token = issue("u_eve", "eve@e.com", TokenType::Access, None).unwrap();
        let user = extract_user(&req_with(
            HeaderMap::new(),
            &format!("/events/stream?token={token}"),
        ))
        .unwrap();
        assert_eq!(user.user_id, "u_eve");
    }

    #[test]
    fn header_takes_precedence_over_query() {
        set_secret();
        let header_tok = issue("u_header", "h@e.com", TokenType::Access, None).unwrap();
        let query_tok = issue("u_query", "q@e.com", TokenType::Access, None).unwrap();
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {header_tok}")).unwrap(),
        );
        let user =
            extract_user(&req_with(h, &format!("/events/stream?token={query_tok}"))).unwrap();
        assert_eq!(user.user_id, "u_header");
    }
}
