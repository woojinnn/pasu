//! Authentication subsystem.
//!
//! Three pieces:
//! - [`jwt`]: stateless token issuance and verification (HS256, env-keyed).
//! - [`oauth`]: Google OAuth 2.0 callback that maps a Google account to a user.
//! - [`middleware`]: axum extractor for bearer tokens.

pub mod jwt;
pub mod middleware;
pub mod oauth;

pub use jwt::{issue, verify, AuthError, Claims, UserId};
pub use middleware::{require_auth, AuthUser};
pub use oauth::{google_callback, refresh_token, start_google_login};
