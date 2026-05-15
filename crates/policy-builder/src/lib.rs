//! Structured rule -> Cedar policy text compiler.
//!
//! Users compose a [`PolicyRule`] (action + severity + predicates) and this
//! crate emits a Cedar policy string suitable for `install_policies_json`.
//!
//! Schemas are data, not code: adding a new action means registering a new
//! [`ActionSchema`] in [`schemas`]. The generator is schema-agnostic.
//!
//! Entry points:
//! - [`compile`] — `PolicyRule` -> Cedar text.
//! - [`mod@validate`] — check a rule against its action schema without emitting.
//! - [`schemas::registry`] — built-in action schemas (swap today, more later).

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

pub mod escape;
pub mod generator;
pub mod operators;
pub mod schemas;
pub mod types;
pub mod validate;

pub use generator::{compile, CompileError};
pub use operators::{Operator, OperatorArity};
pub use schemas::registry;
pub use types::{
    ActionSchema, CedarType, FieldSpec, PolicyRule, Predicate, PredicateValue, Severity,
};
pub use validate::{validate, ValidationError};
