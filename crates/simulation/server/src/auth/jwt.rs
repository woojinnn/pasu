//! Stateless JWT issuance and verification (HS256).
//!
//! Tokens carry `(sub = user_id, email, iat, exp)`. The signing key is read
//! from the `JWT_SECRET` env var on first use and cached for the process
//! lifetime; rotating the secret requires a restart and invalidates every
//! outstanding token (intentional — there is no revocation list).
//!
//! Defaults: access tokens live 1 hour, refresh tokens live 30 days. Both
//! ride the same secret + algorithm; only the `typ` claim differs so an
//! access token can't be replayed as a refresh.

use std::sync::OnceLock;

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// User identifier baked into the `sub` claim. Always lowercase hex of a
/// short hash of the user's email — deterministic so re-logging-in returns
/// the same id.
pub type UserId = String;

/// Access tokens last one hour. Short enough to limit damage if leaked,
/// long enough to avoid friction during typical sessions; combine with
/// refresh tokens for longer continuity.
pub const ACCESS_TTL_SECS: i64 = 60 * 60;

/// Refresh tokens last 30 days. Used to mint new access tokens without
/// re-running OAuth.
pub const REFRESH_TTL_SECS: i64 = 60 * 60 * 24 * 30;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("JWT_SECRET env var not set")]
    MissingSecret,

    #[error("token expired")]
    Expired,

    #[error("invalid token: {0}")]
    Invalid(String),

    #[error("system clock error: {0}")]
    Clock(String),
}

/// Token kind — discriminates access vs refresh so they can't be confused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

/// JWT payload. `sub` follows the standard claim name; everything else is
/// scopeball-specific. `exp` and `iat` are unix seconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: UserId,
    pub email: String,
    #[serde(rename = "typ")]
    pub token_type: TokenType,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    #[must_use]
    pub fn is_access(&self) -> bool {
        matches!(self.token_type, TokenType::Access)
    }

    #[must_use]
    pub fn is_refresh(&self) -> bool {
        matches!(self.token_type, TokenType::Refresh)
    }
}

/// Encode a fresh token. `ttl_secs` overrides the default for the given
/// `token_type` so tests can issue short-lived tokens.
pub fn issue(
    user_id: &str,
    email: &str,
    token_type: TokenType,
    ttl_secs: Option<i64>,
) -> Result<String, AuthError> {
    let now = now_secs()?;
    let ttl = ttl_secs.unwrap_or(match token_type {
        TokenType::Access => ACCESS_TTL_SECS,
        TokenType::Refresh => REFRESH_TTL_SECS,
    });
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        token_type,
        iat: now,
        exp: now + ttl,
    };
    let key = encoding_key()?;
    encode(&Header::new(Algorithm::HS256), &claims, &key)
        .map_err(|e| AuthError::Invalid(e.to_string()))
}

/// Verify signature + expiry, return the decoded claims. Note: `jsonwebtoken`'s
/// `Validation` already rejects expired tokens by default, so a separate
/// expiry check is unnecessary.
pub fn verify(token: &str) -> Result<Claims, AuthError> {
    let key = decoding_key()?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 5; // 5s clock skew tolerance
    match decode::<Claims>(token, &key, &validation) {
        Ok(data) => Ok(data.claims),
        Err(e) => Err(match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::Expired,
            _ => AuthError::Invalid(e.to_string()),
        }),
    }
}

// ---------- internals ----------

/// Cache the secret bytes for the process lifetime — env reads on every
/// request would dominate the JWT cost.
static SECRET_CACHE: OnceLock<Vec<u8>> = OnceLock::new();

fn secret_bytes() -> Result<&'static [u8], AuthError> {
    if let Some(s) = SECRET_CACHE.get() {
        return Ok(s);
    }
    let raw = std::env::var("JWT_SECRET").map_err(|_| AuthError::MissingSecret)?;
    let bytes = raw.into_bytes();
    let _ = SECRET_CACHE.set(bytes);
    Ok(SECRET_CACHE
        .get()
        .expect("SECRET_CACHE just set or already populated"))
}

fn encoding_key() -> Result<EncodingKey, AuthError> {
    secret_bytes().map(EncodingKey::from_secret)
}

fn decoding_key() -> Result<DecodingKey, AuthError> {
    secret_bytes().map(DecodingKey::from_secret)
}

fn now_secs() -> Result<i64, AuthError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AuthError::Clock(e.to_string()))
        .and_then(|d| {
            i64::try_from(d.as_secs())
                .map_err(|_| AuthError::Clock("unix time overflows i64".into()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests share a single secret. Use a unique enough value that no other
    /// test crate happens to have set it.
    fn set_secret() {
        std::env::set_var(
            "JWT_SECRET",
            "test-secret-only-do-not-use-in-production-2026-05-31",
        );
    }

    #[test]
    fn issue_then_verify_round_trip() {
        set_secret();
        let token = issue("u_abc123", "alice@example.com", TokenType::Access, None).unwrap();
        let claims = verify(&token).unwrap();
        assert_eq!(claims.sub, "u_abc123");
        assert_eq!(claims.email, "alice@example.com");
        assert!(claims.is_access());
    }

    #[test]
    fn refresh_token_kind_is_preserved() {
        set_secret();
        let token = issue("u_x", "x@e.com", TokenType::Refresh, None).unwrap();
        let claims = verify(&token).unwrap();
        assert!(claims.is_refresh());
    }

    #[test]
    fn expired_token_rejected() {
        set_secret();
        // ttl = -10s ⇒ already expired when issued.
        let token = issue("u_x", "x@e.com", TokenType::Access, Some(-10)).unwrap();
        let err = verify(&token).unwrap_err();
        assert!(matches!(err, AuthError::Expired), "got {err:?}");
    }

    #[test]
    fn tampered_signature_rejected() {
        set_secret();
        let token = issue("u_x", "x@e.com", TokenType::Access, None).unwrap();
        // Flip a character in the signature segment (last `.` onward).
        let mut bytes = token.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'a' { b'b' } else { b'a' };
        let tampered = String::from_utf8(bytes).unwrap();
        let err = verify(&tampered).unwrap_err();
        assert!(matches!(err, AuthError::Invalid(_)), "got {err:?}");
    }

    #[test]
    fn garbage_token_rejected() {
        set_secret();
        let err = verify("not-a-jwt").unwrap_err();
        assert!(matches!(err, AuthError::Invalid(_)));
    }
}
