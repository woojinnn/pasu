//! Authentication subsystem.
//!
//! Three pieces:
//! - [`jwt`]: stateless token issuance and verification (HS256, env-keyed).
//! - [`oauth`]: Google OAuth 2.0 callback that maps a Google account to a
//!   `user_id` and hands back a JWT (Phase 5.2).
//! - [`middleware`]: axum extractor that turns the `Authorization` header
//!   into an [`AuthUser`] every protected handler receives (Phase 5.1).
//!
//! Phase 4.1 ships [`jwt`] only — `oauth` and `middleware` are wired in the
//! next phase so this lands without changing any HTTP behaviour.

pub mod jwt;
pub mod middleware;
pub mod oauth;

pub use jwt::{issue, verify, AuthError, Claims, UserId};
pub use middleware::{require_auth, AuthUser};
pub use oauth::{google_callback, refresh_token, start_google_login};
