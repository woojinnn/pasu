//! `swap` action schema.
//!
//! Mirrors the v1 `SwapContext` declared in
//! `schema/policy-schema/actions/DEX/swap.cedarschema` plus the manifest-driven
//! `SwapCustomContext` extension shape exemplified by
//! `schema/policy-schema/extensions/DEX/swap.policy-rpc.json`.
//!
//! Composite record fields (`inputToken`, `outputToken`, `totalInputUsd`,
//! `validity`, `windowStats`) are flattened into dotted leaf paths so each
//! addressable comparison gets its own [`FieldSpec`].
//!
//! Each field is tagged `is_custom = true` when it is manifest-enriched and
//! lives under `context.custom`, and `false` when it is calldata-derived and
//! lives directly under `context`. The generator and parser key off this flag.

use crate::types::{ActionSchema, CedarType, FieldSpec};
use std::collections::BTreeMap;

/// Build the `swap` schema. Called once by [`crate::schemas::registry`].
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn schema() -> ActionSchema {
    let mut fields = BTreeMap::new();

    // ─── BASE FIELDS (calldata-derived, addressed as `context.<path>`) ────

    // Required top-level leaves.
    insert(
        &mut fields,
        FieldSpec {
            path: "swapMode".into(),
            cedar_type: CedarType::String,
            optional: false,
            parent_path: None,
            parent_optional: false,
            label: Some("Swap mode".into()),
            is_custom: false,
        },
    );
    insert(
        &mut fields,
        FieldSpec {
            path: "recipient".into(),
            cedar_type: CedarType::String,
            optional: false,
            parent_path: None,
            parent_optional: false,
            label: Some("Recipient address".into()),
            is_custom: false,
        },
    );

    // inputToken / outputToken (required AssetRefWithAmountConstraint).
    for (parent, parent_label) in [
        ("inputToken", "Input token"),
        ("outputToken", "Output token"),
    ] {
        insert_asset_with_amount(&mut fields, parent, parent_label);
    }

    // feeBps (optional Long, base — declared inline in SwapContext).
    insert(
        &mut fields,
        FieldSpec {
            path: "feeBps".into(),
            cedar_type: CedarType::Long,
            optional: true,
            parent_path: None,
            parent_optional: false,
            label: Some("Fee (bps)".into()),
            is_custom: false,
        },
    );

    // validity (optional Validity record, required inner leaves) — base.
    insert(
        &mut fields,
        FieldSpec {
            path: "validity.expiresAt".into(),
            cedar_type: CedarType::String,
            optional: false,
            parent_path: Some("validity".into()),
            parent_optional: true,
            label: Some("Expires at".into()),
            is_custom: false,
        },
    );
    insert(
        &mut fields,
        FieldSpec {
            path: "validity.source".into(),
            cedar_type: CedarType::String,
            optional: false,
            parent_path: Some("validity".into()),
            parent_optional: true,
            label: Some("Validity source".into()),
            is_custom: false,
        },
    );

    // ─── CUSTOM FIELDS (enrichment, addressed as `context.custom.<path>`) ─

    // Optional Long enrichment leaves (top-level under SwapCustomContext).
    for (path, label) in [
        ("effectiveRateVsOracleBps", "Effective rate vs oracle (bps)"),
        (
            "totalInputFractionOfPortfolioBps",
            "Input fraction of portfolio (bps)",
        ),
        ("validityDeltaSec", "Validity delta (sec)"),
    ] {
        insert(
            &mut fields,
            FieldSpec {
                path: path.into(),
                cedar_type: CedarType::Long,
                optional: true,
                parent_path: None,
                parent_optional: false,
                label: Some(label.into()),
                is_custom: true,
            },
        );
    }

    // Optional Bool enrichment leaf.
    insert(
        &mut fields,
        FieldSpec {
            path: "recipientIsContract".into(),
            cedar_type: CedarType::Bool,
            optional: true,
            parent_path: None,
            parent_optional: false,
            label: Some("Recipient is contract".into()),
            is_custom: true,
        },
    );

    // USD valuations (optional UsdValuation records, four required inner
    // leaves each). UsdValuation is declared in core.cedarschema with required
    // leaves `value`, `asOfTs`, `staleSec`, `sources`. Each is addressable
    // independently once the parent `has` guard fires.
    for (parent, parent_label) in [
        ("totalInputUsd", "Total input USD"),
        ("totalMinOutputUsd", "Total min-output USD"),
    ] {
        insert(
            &mut fields,
            FieldSpec {
                path: format!("{parent}.value"),
                cedar_type: CedarType::Decimal,
                optional: false,
                parent_path: Some(parent.into()),
                parent_optional: true,
                label: Some(parent_label.into()),
                is_custom: true,
            },
        );
        insert(
            &mut fields,
            FieldSpec {
                path: format!("{parent}.staleSec"),
                cedar_type: CedarType::Long,
                optional: false,
                parent_path: Some(parent.into()),
                parent_optional: true,
                label: Some(format!("{parent_label} staleness (sec)")),
                is_custom: true,
            },
        );
        insert(
            &mut fields,
            FieldSpec {
                path: format!("{parent}.asOfTs"),
                cedar_type: CedarType::Long,
                optional: false,
                parent_path: Some(parent.into()),
                parent_optional: true,
                label: Some(format!("{parent_label} oracle timestamp")),
                is_custom: true,
            },
        );
        insert(
            &mut fields,
            FieldSpec {
                path: format!("{parent}.sources"),
                cedar_type: CedarType::SetOfString,
                optional: false,
                parent_path: Some(parent.into()),
                parent_optional: true,
                label: Some(format!("{parent_label} oracle sources")),
                is_custom: true,
            },
        );
    }

    // windowStats (optional WindowStats record, optional inner leaves).
    insert(
        &mut fields,
        FieldSpec {
            path: "windowStats.swapVolumeUsd24h".into(),
            cedar_type: CedarType::Decimal,
            optional: true,
            parent_path: Some("windowStats".into()),
            parent_optional: true,
            label: Some("24h swap volume USD".into()),
            is_custom: true,
        },
    );
    insert(
        &mut fields,
        FieldSpec {
            path: "windowStats.swapCount24h".into(),
            cedar_type: CedarType::Long,
            optional: true,
            parent_path: Some("windowStats".into()),
            parent_optional: true,
            label: Some("24h swap count".into()),
            is_custom: true,
        },
    );

    ActionSchema {
        action: "swap".into(),
        principal_type: "Wallet".into(),
        resource_type: "Protocol".into(),
        fields,
    }
}

fn insert_asset_with_amount(
    map: &mut BTreeMap<String, FieldSpec>,
    parent: &str,
    parent_label: &str,
) {
    let asset_parent = format!("{parent}.asset");
    for (leaf, cedar_type, optional, label) in [
        ("kind", CedarType::String, false, "asset kind"),
        ("address", CedarType::String, false, "asset address"),
        ("tokenId", CedarType::String, true, "asset token id"),
        ("symbol", CedarType::String, false, "asset symbol"),
        ("decimals", CedarType::Long, false, "asset decimals"),
    ] {
        insert(
            map,
            FieldSpec {
                path: format!("{asset_parent}.{leaf}"),
                cedar_type,
                optional,
                parent_path: Some(asset_parent.clone()),
                parent_optional: false,
                label: Some(format!("{parent_label} {label}")),
                is_custom: false,
            },
        );
    }

    let amount_parent = format!("{parent}.amount");
    insert(
        map,
        FieldSpec {
            path: format!("{amount_parent}.kind"),
            cedar_type: CedarType::String,
            optional: false,
            parent_path: Some(amount_parent.clone()),
            parent_optional: false,
            label: Some(format!("{parent_label} amount kind")),
            is_custom: false,
        },
    );
    insert(
        map,
        FieldSpec {
            path: format!("{amount_parent}.value"),
            cedar_type: CedarType::String,
            optional: true,
            parent_path: Some(amount_parent),
            parent_optional: false,
            label: Some(format!("{parent_label} amount value")),
            is_custom: false,
        },
    );
}

fn insert(map: &mut BTreeMap<String, FieldSpec>, spec: FieldSpec) {
    map.insert(spec.path.clone(), spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_includes_required_and_nested_fields() {
        let s = schema();
        assert_eq!(s.action, "swap");
        assert!(s.fields.contains_key("swapMode"));
        assert!(s.fields.contains_key("inputToken.asset.address"));
        assert!(s.fields.contains_key("inputToken.asset.symbol"));
        assert!(s.fields.contains_key("inputToken.amount.value"));
        assert!(s.fields.contains_key("outputToken.asset.address"));
        assert!(s.fields.contains_key("totalInputUsd.value"));
        assert!(s.fields.contains_key("recipientIsContract"));
        assert!(s.fields.contains_key("windowStats.swapCount24h"));
    }

    #[test]
    fn token_field_has_required_parent_no_guard() {
        let s = schema();
        let f = s.fields.get("inputToken.asset.address").unwrap();
        assert_eq!(f.parent_path.as_deref(), Some("inputToken.asset"));
        assert!(!f.parent_optional);
    }

    #[test]
    fn token_fields_carry_required_parent() {
        let s = schema();
        let f = s.fields.get("inputToken.asset.decimals").unwrap();
        assert!(!f.optional);
        assert!(!f.parent_optional);
        assert_eq!(f.parent_path.as_deref(), Some("inputToken.asset"));
    }

    #[test]
    fn usd_valuation_parent_is_optional() {
        let s = schema();
        let f = s.fields.get("totalInputUsd.value").unwrap();
        assert_eq!(f.parent_path.as_deref(), Some("totalInputUsd"));
        assert!(f.parent_optional);
    }

    #[test]
    fn base_field_is_not_custom() {
        let s = schema();
        // Calldata-derived fields live under context.<path>, not context.custom.
        for path in [
            "swapMode",
            "recipient",
            "feeBps",
            "inputToken.asset.address",
            "outputToken.amount.value",
            "validity.expiresAt",
        ] {
            let f = s
                .fields
                .get(path)
                .unwrap_or_else(|| panic!("missing {path}"));
            assert!(!f.is_custom, "expected base (not custom) for {path}");
        }
    }

    #[test]
    fn enrichment_fields_are_custom() {
        let s = schema();
        // Manifest-contributed fields live under context.custom.<path>.
        for path in [
            "totalInputUsd.value",
            "totalInputUsd.staleSec",
            "totalInputUsd.sources",
            "totalMinOutputUsd.value",
            "validityDeltaSec",
            "effectiveRateVsOracleBps",
            "totalInputFractionOfPortfolioBps",
            "recipientIsContract",
            "windowStats.swapCount24h",
            "windowStats.swapVolumeUsd24h",
        ] {
            let f = s
                .fields
                .get(path)
                .unwrap_or_else(|| panic!("missing {path}"));
            assert!(f.is_custom, "expected custom for {path}");
        }
    }
}
