//! Action schema registry.
//!
//! Each module under this path defines one [`ActionSchema`]. The
//! [`registry`] function returns them all by action name; extending support
//! to a new action means adding a module and one entry here.
//!
//! [`ActionSchema`]: crate::types::ActionSchema

pub mod swap;

/// Build-time enum lookups extracted from `schema/action-schema/**/*.json`.
///
/// `build.rs` walks the upstream JSON Schema files, resolves `$ref` against
/// the common `$defs`, and renders `action_field_enum(action, path)` —
/// a `match` that returns the declared closed-set enum (or `None`). Action
/// schema modules call this from their `FieldSpec::allowed_values` so the
/// JSON remains the single source of truth; a hand-written list and the
/// JSON cannot drift.
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/generated_action_constraints.rs"));
}

use crate::types::ActionSchema;
use std::collections::BTreeMap;

/// Build the registry of all known action schemas.
///
/// Returned fresh each call so callers can mutate / extend per-instance
/// without sharing state. Cost is small (a handful of `BTreeMap` inserts).
#[must_use]
pub fn registry() -> BTreeMap<String, ActionSchema> {
    let mut out = BTreeMap::new();
    // The loop is intentionally over an array that will gain more entries
    // as new actions are registered (approve, transfer, …).
    #[allow(clippy::single_element_loop)]
    for schema in [swap::schema()] {
        out.insert(schema.action.clone(), schema);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_swap() {
        let r = registry();
        assert!(r.contains_key("swap"));
    }
}
