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
//!
//! `allowed_values` and `pattern` are **not declared inline here** — they are
//! sourced at build time from the upstream action-schema JSON via
//! [`super::generated`]. `enum_for` / `pattern_for` do the lookup so a JSON
//! edit (e.g. extending `swapMode` or tweaking the Address regex) flows
//! into this schema on the next `cargo build` with no hand-editing. Fields
//! without a JSON constraint get `None` automatically.
//!
//! `scale` is set on the token-native amount fields (`inputAmountNano`,
//! `outputAmountNano`) so the policy builder accepts user input in the
//! "0.5 ETH"/"100 USDC" form a DEX UI shows and emits the matching Long
//! literal (`500000000` / `100000000000`). The manifest enrichment is
//! responsible for pre-multiplying the raw on-chain amount by
//! `10^(9 - decimals)` before the engine sees it, so the same policy applies
//! identically to any token regardless of its decimals.

use super::generated::{action_field_enum, action_field_pattern};
use crate::types::{ActionSchema, CedarType, FieldSpec};
use std::collections::BTreeMap;

const ACTION: &str = "swap";

/// Decimal-point exponent used by `*AmountNano` custom fields. The manifest
/// rescales raw on-chain `amount.value` by `10^(9 - decimals)` so the
/// resulting Long is in the same Gwei-style unit regardless of the token's
/// own decimals. Matched here so policy builder literal rendering uses the
/// same shift.
const AMOUNT_NANO_SCALE: u8 = 9;

/// Look up the build-time-generated enum list for a path under this action.
/// Returns `None` for free-form fields (no JSON enum constraint), matching
/// the `FieldSpec::allowed_values` semantics.
fn enum_for(path: &str) -> Option<Vec<String>> {
    action_field_enum(ACTION, path).map(|s| s.iter().map(|v| (*v).to_string()).collect())
}

/// Look up the build-time-generated regex for a path under this action.
/// Returns `None` for fields whose JSON Schema declares no `pattern`,
/// matching the `FieldSpec::pattern` semantics.
fn pattern_for(path: &str) -> Option<String> {
    action_field_pattern(ACTION, path).map(str::to_string)
}

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
            allowed_values: enum_for("swapMode"),
            scale: None,
            pattern: pattern_for("swapMode"),
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
            allowed_values: None,
            scale: None,
            pattern: pattern_for("recipient"),
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
            allowed_values: None,
            scale: None,
            pattern: None,
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
            allowed_values: None,
            scale: None,
            pattern: pattern_for("validity.expiresAt"),
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
            allowed_values: enum_for("validity.source"),
            scale: None,
            pattern: pattern_for("validity.source"),
        },
    );

    // Token-native normalized amounts (Long with implicit 10⁻⁹ scale).
    // Promoted to BASE in Phase 8: the engine computes these directly from
    // `inputToken.amount.value` + `inputToken.asset.decimals` during the
    // lowering step (see `policy-engine::lowering::dex::swap::nano_amount`).
    // They no longer go through `context.custom.*` and no longer require
    // a manifest enrichment to populate them — a user can't break amount
    // policies by editing the manifest.
    //
    // Users still type "0.5" / "100" / "0.00003" — the same number a DEX
    // UI shows — and `scale: Some(9)` makes the compiler emit the matching
    // Long literal (`500000000` / `…`).
    for (path, label) in [
        ("inputAmountNano", "Input amount (token-native)"),
        ("outputAmountNano", "Output amount (token-native)"),
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
                is_custom: false,
                allowed_values: None,
                scale: Some(AMOUNT_NANO_SCALE),
                pattern: None,
            },
        );
    }

    // ─── CUSTOM FIELDS — REMOVED in Phase 8 ───────────────────────────────
    //
    // Previously this section declared seven hand-coded manifest-enriched
    // fields (`effectiveRateVsOracleBps`, `totalInputUsd.*`, `windowStats.*`,
    // etc.) that mirrored the bundled `swap.policy-rpc.json`. The mirror
    // was a maintenance trap: editing one without the other silently broke
    // the builder/runtime contract.
    //
    // The new architecture splits the world cleanly:
    //   - `swap.rs` (this file) declares ONLY base fields the engine
    //     populates from calldata. No knowledge of any manifest needed.
    //   - Manifest-installed custom fields surface via the WASM overlay
    //     path (`get_action_schema_with_overlay_json` + friends) — both
    //     scalar outputs (`Long`, `Bool`, …) and record outputs
    //     (`UsdValuation`, `WindowStats`, …) are supported by the
    //     `aliases::record_leaves` table.
    //
    // So users who install the bundled starter manifest (or write their
    // own) see the same predicate picker entries the old hardcoded list
    // produced, but the builder picks them up dynamically from the
    // engine's enriched schema instead of from this Rust source.
    //
    // `SwapCustomContext` itself stays declared in `swap.cedarschema` as
    // an empty record; manifests extend it at install time.

    ActionSchema {
        action: "swap".into(),
        principal_type: "Wallet".into(),
        resource_type: "Protocol".into(),
        fields,
    }
}

/// Test-facing helper that returns the `swap` schema augmented with the
/// legacy hand-coded custom fields that lived in this file before Phase 8.
///
/// Production code MUST NOT use this — at runtime those fields come from
/// the WASM overlay path (`get_action_schema_with_overlay_json`) which
/// pulls them from the engine's enriched schema. The helper exists only
/// so generator/parser/validator + round-trip tests keep their original
/// fixtures without re-implementing manifest-style enrichment in every test.
///
/// The added fields mirror what `extensions/DEX/swap.policy-rpc.json`
/// declares as outputs, expanded through `aliases::record_leaves` for
/// the record-typed ones (`UsdValuation`, `WindowStats`).
///
/// Not `#[cfg(test)]`: `tests/roundtrip.rs` is an integration test (a
/// separate crate) that links `policy-builder` without `cfg(test)`, so a
/// `cfg(test)` item would be invisible to it. `#[doc(hidden)]` keeps this
/// helper off the public API surface instead.
#[doc(hidden)]
#[must_use]
pub fn schema_with_legacy_custom() -> ActionSchema {
    let mut s = schema();
    let custom = legacy_custom_fields();
    for spec in custom {
        s.fields.insert(spec.path.clone(), spec);
    }
    s
}

#[doc(hidden)]
fn legacy_custom_fields() -> Vec<FieldSpec> {
    use crate::aliases::record_leaves;
    let mut out = Vec::new();

    // Top-level scalar customs.
    for (path, ty, label) in [
        (
            "effectiveRateVsOracleBps",
            CedarType::Long,
            "Effective rate vs oracle (bps)",
        ),
        (
            "totalInputFractionOfPortfolioBps",
            CedarType::Long,
            "Input fraction of portfolio (bps)",
        ),
        ("validityDeltaSec", CedarType::Long, "Validity delta (sec)"),
        (
            "recipientIsContract",
            CedarType::Bool,
            "Recipient is contract",
        ),
    ] {
        out.push(FieldSpec {
            path: path.to_owned(),
            cedar_type: ty,
            optional: true,
            parent_path: None,
            parent_optional: false,
            label: Some(label.to_owned()),
            is_custom: true,
            allowed_values: None,
            scale: None,
            pattern: None,
        });
    }

    // Record customs: expand `UsdValuation`/`WindowStats` via the alias
    // leaf table so the test fixture mirrors what the overlay path
    // produces at runtime.
    for parent in ["totalInputUsd", "totalMinOutputUsd"] {
        for leaf in record_leaves("UsdValuation").expect("known alias") {
            out.push(FieldSpec {
                path: format!("{parent}.{}", leaf.name),
                cedar_type: leaf.cedar_type,
                optional: leaf.optional,
                parent_path: Some(parent.to_owned()),
                parent_optional: true,
                label: None,
                is_custom: true,
                allowed_values: None,
                scale: None,
                pattern: None,
            });
        }
    }
    for leaf in record_leaves("WindowStats").expect("known alias") {
        out.push(FieldSpec {
            path: format!("windowStats.{}", leaf.name),
            cedar_type: leaf.cedar_type,
            optional: leaf.optional,
            parent_path: Some("windowStats".to_owned()),
            parent_optional: true,
            label: None,
            is_custom: true,
            allowed_values: None,
            scale: None,
            pattern: None,
        });
    }
    out
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
        let path = format!("{asset_parent}.{leaf}");
        let allowed_values = enum_for(&path);
        let pattern = pattern_for(&path);
        insert(
            map,
            FieldSpec {
                path,
                cedar_type,
                optional,
                parent_path: Some(asset_parent.clone()),
                parent_optional: false,
                label: Some(format!("{parent_label} {label}")),
                is_custom: false,
                allowed_values,
                scale: None,
                pattern,
            },
        );
    }

    let amount_parent = format!("{parent}.amount");
    let amount_kind_path = format!("{amount_parent}.kind");
    let amount_kind_enum = enum_for(&amount_kind_path);
    let amount_kind_pattern = pattern_for(&amount_kind_path);
    insert(
        map,
        FieldSpec {
            path: amount_kind_path,
            cedar_type: CedarType::String,
            optional: false,
            parent_path: Some(amount_parent.clone()),
            parent_optional: false,
            label: Some(format!("{parent_label} amount kind")),
            is_custom: false,
            allowed_values: amount_kind_enum,
            scale: None,
            pattern: amount_kind_pattern,
        },
    );
    let amount_value_path = format!("{amount_parent}.value");
    let amount_value_pattern = pattern_for(&amount_value_path);
    insert(
        map,
        FieldSpec {
            path: amount_value_path,
            cedar_type: CedarType::String,
            optional: true,
            parent_path: Some(amount_parent),
            parent_optional: false,
            label: Some(format!("{parent_label} amount value")),
            is_custom: false,
            allowed_values: None,
            scale: None,
            pattern: amount_value_pattern,
        },
    );
}

/// Composite (intermediate) paths inside `$.action.*` that resolve to a
/// known Cedar record alias. Used by the manifest editor's type-aware
/// selector picker (Phase 8.5 / PR 4) so when a method param declares
/// `type: "AssetRef"`, the picker can offer just these two paths
/// (`inputToken.asset`, `outputToken.asset`) instead of every String
/// leaf under them.
///
/// Per-action and hand-coded because the leaves alone don't tell the
/// builder which alias their parent matches — Cedar lets you name two
/// different records with the same leaf shape, and the alias choice
/// is a schema-author decision. swap's mapping comes straight from
/// `core.cedarschema`'s `type SwapContext = { inputToken:
/// AssetRefWithAmountConstraint, ... }`.
#[must_use]
pub fn record_paths() -> Vec<(&'static str, &'static str)> {
    vec![
        ("inputToken", "AssetRefWithAmountConstraint"),
        ("inputToken.asset", "AssetRef"),
        ("inputToken.amount", "AmountConstraint"),
        ("outputToken", "AssetRefWithAmountConstraint"),
        ("outputToken.asset", "AssetRef"),
        ("outputToken.amount", "AmountConstraint"),
        ("validity", "Validity"),
    ]
}

fn insert(map: &mut BTreeMap<String, FieldSpec>, spec: FieldSpec) {
    map.insert(spec.path.clone(), spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_includes_required_and_nested_fields() {
        // Post-Phase-8: this test still covers the legacy union (base +
        // formerly-bundled custom) via the test fixture, since both still
        // appear in the runtime view of the schema once overlay merges
        // them in. Asserts the path table the builder UI ends up with
        // is complete.
        let s = schema_with_legacy_custom();
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
        // UsdValuation leaves carry `parent_optional = true` so the
        // generator emits `context.custom has totalInputUsd` before any
        // `.value`/`.staleSec`/etc. access. The overlay-driven path
        // (mirrored by `schema_with_legacy_custom`) maintains the same
        // contract record-by-record via `aliases::record_leaves`.
        let s = schema_with_legacy_custom();
        let f = s.fields.get("totalInputUsd.value").unwrap();
        assert_eq!(f.parent_path.as_deref(), Some("totalInputUsd"));
        assert!(f.parent_optional);
    }

    #[test]
    fn base_field_is_not_custom() {
        let s = schema();
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
        // Phase 8: these fields no longer live in the static `schema()` —
        // they're populated at runtime by the WASM overlay path from the
        // engine's enriched schema. The test fixture replays that path
        // so the contract (`is_custom = true` + `parent_optional` chain)
        // is still exercised. `inputAmountNano` / `outputAmountNano` were
        // promoted to BASE and are covered separately by
        // `nano_amount_fields_are_base_scaled_longs`.
        let s = schema_with_legacy_custom();
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

    #[test]
    fn swap_mode_has_enum() {
        let s = schema();
        let f = s.fields.get("swapMode").unwrap();
        let allowed = f.allowed_values.as_ref().expect("swapMode must be enum");
        assert_eq!(
            allowed,
            &vec![
                "exact_in".to_string(),
                "exact_out".to_string(),
                "market".to_string(),
                "unknown".to_string(),
            ]
        );
    }

    #[test]
    fn asset_kind_has_enum_on_both_tokens() {
        let s = schema();
        for path in ["inputToken.asset.kind", "outputToken.asset.kind"] {
            let f = s.fields.get(path).unwrap();
            let allowed = f
                .allowed_values
                .as_ref()
                .unwrap_or_else(|| panic!("{path} must be enum"));
            assert!(allowed.contains(&"erc20".to_string()));
            assert!(allowed.contains(&"native".to_string()));
        }
    }

    #[test]
    fn amount_kind_has_enum_on_both_tokens() {
        let s = schema();
        for path in ["inputToken.amount.kind", "outputToken.amount.kind"] {
            let f = s.fields.get(path).unwrap();
            let allowed = f
                .allowed_values
                .as_ref()
                .unwrap_or_else(|| panic!("{path} must be enum"));
            assert!(allowed.contains(&"exact".to_string()));
            assert!(allowed.contains(&"unlimited".to_string()));
        }
    }

    #[test]
    fn validity_source_has_enum() {
        let s = schema();
        let f = s.fields.get("validity.source").unwrap();
        let allowed = f
            .allowed_values
            .as_ref()
            .expect("validity.source must be enum");
        assert!(allowed.contains(&"tx-deadline".to_string()));
    }

    #[test]
    fn free_form_fields_have_no_enum() {
        let s = schema_with_legacy_custom();
        for path in [
            "recipient",
            "feeBps",
            "inputToken.asset.address",
            "inputToken.amount.value",
            "validity.expiresAt",
            "totalInputUsd.value",
        ] {
            let f = s.fields.get(path).unwrap();
            assert!(
                f.allowed_values.is_none(),
                "{path} should not be a closed enum"
            );
        }
    }

    #[test]
    fn nano_amount_fields_are_base_scaled_longs() {
        // Phase 8: engine-computed (lowering step), no manifest dependency.
        // `is_custom == false` flips the compile-emit path to `context.<x>`
        // and the optional `has` guard checks `context has <x>` instead of
        // `context.custom has <x>`. Scale stays so the literal rescale at
        // compile time still mirrors the engine-side computation.
        let s = schema();
        for path in ["inputAmountNano", "outputAmountNano"] {
            let f = s
                .fields
                .get(path)
                .unwrap_or_else(|| panic!("missing {path}"));
            assert!(matches!(f.cedar_type, CedarType::Long));
            assert!(
                !f.is_custom,
                "{path} is base (engine-computed) post Phase 8"
            );
            assert_eq!(f.scale, Some(AMOUNT_NANO_SCALE));
            assert!(f.optional, "amount value or decimals may be absent");
            assert!(f.parent_path.is_none());
        }
    }

    #[test]
    fn non_scaled_fields_have_none_scale() {
        let s = schema_with_legacy_custom();
        for path in [
            "swapMode",
            "feeBps",
            "inputToken.amount.value",
            "totalInputUsd.value",
            "validityDeltaSec",
        ] {
            let f = s.fields.get(path).unwrap();
            assert!(f.scale.is_none(), "{path} should not have a scale");
        }
    }

    #[test]
    fn address_fields_carry_evm_pattern() {
        // Recipient and both asset addresses share the standard EVM hex shape.
        let s = schema();
        for path in [
            "recipient",
            "inputToken.asset.address",
            "outputToken.asset.address",
        ] {
            let f = s.fields.get(path).unwrap();
            let pat = f
                .pattern
                .as_deref()
                .unwrap_or_else(|| panic!("{path} should have a pattern"));
            assert_eq!(pat, "^0x[0-9a-fA-F]{40}$");
        }
    }

    #[test]
    fn decimal_string_fields_carry_digits_pattern() {
        let s = schema();
        for path in [
            "validity.expiresAt",
            "inputToken.asset.tokenId",
            "outputToken.asset.tokenId",
            "inputToken.amount.value",
            "outputToken.amount.value",
        ] {
            let f = s.fields.get(path).unwrap();
            assert_eq!(f.pattern.as_deref(), Some("^[0-9]+$"));
        }
    }

    #[test]
    fn fields_without_pattern_constraint_are_none() {
        let s = schema_with_legacy_custom();
        for path in [
            "swapMode", // enum, but no regex
            "feeBps",   // numeric, not a String pattern
            "inputAmountNano",
            "totalInputUsd.value",
            "recipientIsContract",
        ] {
            let f = s.fields.get(path).unwrap();
            assert!(
                f.pattern.is_none(),
                "{path} should not carry a regex pattern"
            );
        }
    }
}
