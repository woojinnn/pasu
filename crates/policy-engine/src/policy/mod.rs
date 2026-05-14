//! Cedar policy wrapper.
//!
//! Wraps the AWS `cedar-policy` crate to enforce our v0.1 conventions:
//!
//! 1. **Default-allow**: a baseline `permit(principal, action, resource);`
//!    policy is added to the policy set so that, in the absence of any
//!    matching `forbid`, Cedar returns `Allow`.
//! 2. **`@severity` annotation** on each `forbid` clause distinguishes `deny`
//!    from `warn`. Deny-overrides; warn-union otherwise.
//! 3. **Verdict aggregation**: we read Cedar diagnostics to discover which
//!    `forbid` clauses fired and which severity each carried, then collapse
//!    the result into our tri-state `Verdict`.

mod builder;
mod engine;
mod error;
mod request;
mod verdict;

pub use builder::PolicyEngineBuilder;
pub use engine::PolicyEngine;
pub use error::PolicyError;
pub use request::PolicyRequest;
pub use verdict::{MatchedPolicy, PolicyRequestOrigin, Severity, Verdict};
