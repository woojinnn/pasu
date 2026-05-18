//! Whitelist of Cedar types allowed as manifest-derived custom context fields.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Distinguishes scalar Cedar types from named record types declared in the
/// shipped core schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasKind {
    /// Scalar Cedar type such as `String`, `Long`, `Bool`, `decimal`.
    Scalar,
    /// Named record type declared in `core.cedarschema`.
    Record,
}

/// One entry in the base alias table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasEntry {
    /// Whether the alias names a scalar or a record type.
    pub kind: AliasKind,
    /// Exact spelling used in Cedar schema source.
    pub cedar_spelling: &'static str,
}

static TABLE: OnceLock<BTreeMap<&'static str, AliasEntry>> = OnceLock::new();

/// Return the base alias table.
///
/// Keys are the names accepted in manifest `outputs[].type` and
/// `context_extensions` entries. Values record how each alias renders in
/// Cedar source.
#[must_use]
pub fn base_alias_table() -> &'static BTreeMap<&'static str, AliasEntry> {
    TABLE.get_or_init(|| {
        let mut m = BTreeMap::new();
        for s in ["String", "Long", "Bool", "decimal", "Set<String>"] {
            m.insert(
                s,
                AliasEntry {
                    kind: AliasKind::Scalar,
                    cedar_spelling: s,
                },
            );
        }
        for r in [
            "AssetRefWithAmountConstraint",
            "AssetRef",
            "AmountConstraint",
            "Validity",
            "UsdValuation",
            "WindowStats",
            "Pool",
            "HookPermissions",
            "TickRange",
        ] {
            m.insert(
                r,
                AliasEntry {
                    kind: AliasKind::Record,
                    cedar_spelling: r,
                },
            );
        }
        m
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_are_present() {
        let t = base_alias_table();
        assert!(t.contains_key("String"));
        assert!(t.contains_key("Long"));
        assert!(t.contains_key("Bool"));
        assert!(t.contains_key("decimal"));
        assert!(t.contains_key("Set<String>"));
    }

    #[test]
    fn core_records_are_present() {
        let t = base_alias_table();
        for name in [
            "AssetRefWithAmountConstraint",
            "AssetRef",
            "AmountConstraint",
            "Validity",
            "UsdValuation",
            "WindowStats",
            "Pool",
            "HookPermissions",
            "TickRange",
        ] {
            assert!(t.contains_key(name), "missing {name}");
        }
    }

    #[test]
    fn unknown_alias_is_absent() {
        assert!(!base_alias_table().contains_key("RiskScore"));
    }
}
