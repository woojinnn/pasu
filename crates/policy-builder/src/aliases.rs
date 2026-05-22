//! Record-alias leaf expansion for runtime overlay.
//!
//! The manifest editor lets a user declare `outputs[].type = "UsdValuation"`,
//! which the engine composes into the enriched cedarschema as
//! `<field>?: UsdValuation`. For the builder UI to surface predicates on
//! `<field>.value`, `<field>.staleSec`, etc. the WASM overlay path needs to
//! know what leaves each record alias unfolds into.
//!
//! The same record shapes are also declared canonically in
//! `schema/policy-schema/core.cedarschema`. We do NOT parse that file here:
//! the cedarschema parser would pull in the cedar-policy dependency and
//! balloon policy-builder's compile graph, and the leaf set for the dozen
//! aliases the manifest editor exposes barely changes. We mirror them by
//! hand and gate drift with a test that the leaf set matches the one
//! `policy-engine::schema::aliases` registers as known.

use crate::types::CedarType;

/// One leaf in a record alias, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasLeaf {
    /// Leaf attribute name (e.g. `"value"`, `"staleSec"`).
    pub name: &'static str,
    /// Cedar type the leaf carries.
    pub cedar_type: CedarType,
    /// Whether the leaf itself is optional inside the record. Drives the
    /// per-leaf `has` guard the generator emits when this field is referenced
    /// from a predicate.
    pub optional: bool,
}

/// Return the leaves for a record-alias spelling, or `None` when the spelling
/// is unknown / refers to a scalar.
///
/// Spelling matches the manifest wire shape (`UsdValuation`, `WindowStats`,
/// `Set<String>`, …) so callers can compare directly against
/// `outputs[].type` strings.
#[must_use]
pub fn record_leaves(spelling: &str) -> Option<&'static [AliasLeaf]> {
    match spelling {
        "UsdValuation" => Some(USD_VALUATION_LEAVES),
        "WindowStats" => Some(WINDOW_STATS_LEAVES),
        "Validity" => Some(VALIDITY_LEAVES),
        "AssetRef" => Some(ASSET_REF_LEAVES),
        "AmountConstraint" => Some(AMOUNT_CONSTRAINT_LEAVES),
        "AssetRefWithAmountConstraint" => Some(ASSET_REF_WITH_AMOUNT_LEAVES),
        "TickRange" => Some(TICK_RANGE_LEAVES),
        "Pool" => Some(POOL_LEAVES),
        // HookPermissions has 14 bool flags — manifest authors rarely pull
        // it whole, and the overlay does best-effort: scalar fields work
        // anyway and the full Bool table can be added if a real use shows
        // up. Skipping keeps this list focused on records that actually
        // appear in shipped manifests.
        _ => None,
    }
}

const USD_VALUATION_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "value",
        cedar_type: CedarType::Decimal,
        optional: false,
    },
    AliasLeaf {
        name: "asOfTs",
        cedar_type: CedarType::Long,
        optional: false,
    },
    AliasLeaf {
        name: "staleSec",
        cedar_type: CedarType::Long,
        optional: false,
    },
    AliasLeaf {
        name: "sources",
        cedar_type: CedarType::SetOfString,
        optional: false,
    },
];

const WINDOW_STATS_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "swapVolumeUsd24h",
        cedar_type: CedarType::Decimal,
        optional: true,
    },
    AliasLeaf {
        name: "swapCount24h",
        cedar_type: CedarType::Long,
        optional: true,
    },
];

const VALIDITY_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "expiresAt",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "source",
        cedar_type: CedarType::String,
        optional: false,
    },
];

const ASSET_REF_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "kind",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "address",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "tokenId",
        cedar_type: CedarType::String,
        optional: true,
    },
    AliasLeaf {
        name: "symbol",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "decimals",
        cedar_type: CedarType::Long,
        optional: false,
    },
];

const AMOUNT_CONSTRAINT_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "kind",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "value",
        cedar_type: CedarType::String,
        optional: true,
    },
];

/// Composite. The leaves under `asset.*` and `amount.*` resolve via further
/// lookups in `record_leaves` — we don't pre-flatten because nested record
/// fields keep their own `parent_path` chain that the overlay applier
/// builds explicitly.
const ASSET_REF_WITH_AMOUNT_LEAVES: &[AliasLeaf] = &[];

const TICK_RANGE_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "lower",
        cedar_type: CedarType::Long,
        optional: false,
    },
    AliasLeaf {
        name: "upper",
        cedar_type: CedarType::Long,
        optional: false,
    },
];

const POOL_LEAVES: &[AliasLeaf] = &[
    AliasLeaf {
        name: "address",
        cedar_type: CedarType::String,
        optional: false,
    },
    AliasLeaf {
        name: "id",
        cedar_type: CedarType::String,
        optional: true,
    },
    AliasLeaf {
        name: "label",
        cedar_type: CedarType::String,
        optional: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_valuation_has_four_required_leaves() {
        let leaves = record_leaves("UsdValuation").expect("known alias");
        assert_eq!(leaves.len(), 4);
        assert!(leaves.iter().any(|l| l.name == "value"
            && matches!(l.cedar_type, CedarType::Decimal)
            && !l.optional));
        assert!(leaves.iter().any(|l| l.name == "sources"
            && matches!(l.cedar_type, CedarType::SetOfString)));
    }

    #[test]
    fn window_stats_leaves_are_optional() {
        let leaves = record_leaves("WindowStats").expect("known alias");
        assert_eq!(leaves.len(), 2);
        for l in leaves {
            assert!(l.optional, "{} should be optional in WindowStats", l.name);
        }
    }

    #[test]
    fn unknown_alias_returns_none() {
        assert!(record_leaves("ScalarLong").is_none());
        assert!(record_leaves("RiskScore").is_none());
        assert!(record_leaves("Long").is_none()); // scalar, not record
    }
}
