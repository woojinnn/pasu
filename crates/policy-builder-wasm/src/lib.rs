//! WASM bridge for `policy-builder`.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers:
//! - `list_actions()` — known action names.
//! - `get_action_schema_json(action)` — schema for one action, including the
//!   valid operators for each field (so the UI can drive its dropdowns from
//!   one source of truth without redeclaring the operator table in TS).
//! - `get_action_schema_with_overlay_json(input_json)` — same shape as
//!   `get_action_schema_json` but additionally merges user-supplied custom
//!   fields (e.g. those installed via the `/manifests/<action>` editor) on
//!   top of the bundled static schema, so the builder UI surfaces them
//!   without a Rust rebuild.
//! - `compile_policy_json(rule_json)` — `PolicyRule` -> Cedar text or
//!   structured error.
//! - `compile_policy_with_overlay_json(input_json)` /
//!   `validate_policy_with_overlay_json(input_json)` — same as the no-overlay
//!   variants but additionally accept the same `overlay` payload as
//!   `get_action_schema_with_overlay_json`. Callers MUST pass the same
//!   overlay they used to fetch the schema; otherwise the builder UI shows
//!   a field that compile/validate then rejects as unknown.
//! - `get_typed_paths_for_action_json(action)` — flat list of every
//!   selector path the action exposes, tagged with its Cedar type
//!   (scalar) or record alias. The manifest editor's selector picker
//!   uses this to filter the dropdown by the param's required type
//!   (Phase 8.5 / PR 4) so a `Long` slot doesn't surface String paths.
//!
//! All return values are wrapped in an `Envelope { ok, data?, error? }`
//! shape so the TS side has a single discriminant to branch on regardless
//! of which call produced the response.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use policy_builder::aliases::{record_leaves, AliasLeaf};
use policy_builder::escape::unscale_long_to_decimal;
use policy_builder::operators::{operators_for, OperatorArity};
use policy_builder::schemas;
use policy_builder::types::{ActionSchema, CedarType, FieldSpec, PolicyRule, PredicateValue};
use policy_builder::{compile, parse_cedar, validate, CompileError, ParseError, ValidationError};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Module init: forward Rust panics to the JS console.
#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<EnvelopeError>,
}

impl<T: Serialize> Envelope<T> {
    fn success(value: T) -> Self {
        Self {
            ok: true,
            data: Some(value),
            error: None,
        }
    }

    fn failure(error: EnvelopeError) -> Envelope<()> {
        Envelope {
            ok: false,
            data: None,
            error: Some(error),
        }
    }

    fn to_json(&self) -> String {
        // Serializing a fully-owned, well-formed enum can fail only on
        // recursion/cycles, which our shape doesn't have.
        serde_json::to_string(self).expect("envelope serialization is infallible")
    }
}

#[derive(Serialize)]
struct EnvelopeError {
    kind: String,
    message: String,
    /// Predicate index when applicable; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    predicate_index: Option<usize>,
}

/// List all action names registered in the bundled schema registry.
///
/// Returns an envelope whose `data` is a `Vec<String>` of action keys in
/// ascending order.
#[wasm_bindgen]
#[must_use]
pub fn list_actions() -> String {
    let actions: Vec<String> = schemas::registry().keys().cloned().collect();
    Envelope::success(actions).to_json()
}

/// Return the full schema for one action, augmented with operator metadata
/// per field so the UI can render dropdowns without duplicating tables.
///
/// `data` shape:
/// ```text
/// {
///   "action": "swap",
///   "principalType": "Wallet",
///   "resourceType": "Protocol",
///   "fields": [
///     {
///       "path": "totalInputUsd.value",
///       "type": "decimal",
///       "optional": false,
///       "parentPath": "totalInputUsd",
///       "parentOptional": true,
///       "label": "Total input USD",
///       "isCustom": true,
///       "operators": [
///         { "id": "gt", "label": ">", "arity": "one" },
///         …
///       ]
///     },
///     …
///   ]
/// }
/// ```
///
/// `isCustom: true` means the field lives under the optional
/// `context.custom` record (a manifest-enriched extension); `false` means it
/// lives directly under `context` (a calldata-derived base field). The
/// compiler emits `context.custom.<path>` and the appropriate guard cluster
/// (`context has custom && context.custom has <parent>`) automatically — UIs
/// should use the flag for visual grouping rather than rewriting paths.
#[wasm_bindgen]
#[must_use]
pub fn get_action_schema_json(action: String) -> String {
    let registry = schemas::registry();
    let Some(schema) = registry.get(&action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {action}"),
            predicate_index: None,
        })
        .to_json();
    };
    let dto = schema_to_dto(schema);
    Envelope::success(dto).to_json()
}

/// Return the schema for `action` augmented with caller-supplied custom
/// fields ("overlay"). Used by the builder UI to surface manifest-derived
/// custom fields the bundled static schema doesn't know about yet (e.g.
/// fields a user added via the manifest editor and installed at runtime).
///
/// Input shape:
/// ```text
/// {
///   "action": "swap",
///   "overlay": [
///     { "field": "myRiskScore", "cedarType": "Long" },
///     { "field": "userTag",     "cedarType": "String" }
///   ]
/// }
/// ```
///
/// Overlay rules (kept narrow on purpose so the v1 contract is predictable):
/// - `cedarType` accepts both scalar primitives (`Long`, `String`, `Bool`,
///   `decimal`, `Set<String>`, `Set<Long>`) AND record aliases declared in
///   [`policy_builder::aliases::record_leaves`] (`UsdValuation`,
///   `WindowStats`, `Validity`, `AssetRef`, `AmountConstraint`,
///   `AssetRefWithAmountConstraint`, `TickRange`, `Pool`). Record entries
///   are expanded into one [`FieldSpec`] per leaf with `parent_path` set
///   so the generator emits the matching `context.custom has <parent>`
///   guard. Unknown spellings are dropped silently.
/// - Dedup: an overlay entry is dropped if the static schema already
///   contains a field whose path equals `field` or starts with `<field>.`,
///   so a user re-adding `totalInputUsd` doesn't collide with previously
///   inserted leaves.
/// - Overlay fields are inserted as `is_custom: true`, `optional: true`,
///   `parent_path: None`. No label, allowed_values, scale, or pattern —
///   manifest-installed fields don't carry that metadata at runtime.
///
/// On invalid JSON or unknown action the envelope returns the same error
/// shapes as `get_action_schema_json`.
#[wasm_bindgen]
#[must_use]
pub fn get_action_schema_with_overlay_json(input_json: String) -> String {
    let input: OverlayInput = match serde_json::from_str(&input_json) {
        Ok(v) => v,
        Err(error) => {
            return Envelope::<()>::failure(EnvelopeError {
                kind: "invalid_input_json".to_string(),
                message: error.to_string(),
                predicate_index: None,
            })
            .to_json();
        }
    };
    let registry = schemas::registry();
    let Some(schema) = registry.get(&input.action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {}", input.action),
            predicate_index: None,
        })
        .to_json();
    };

    let merged = apply_overlay(schema, &input.overlay);
    Envelope::success(schema_to_dto(&merged)).to_json()
}

/// Clone `schema` and merge every overlay entry that resolves to a known
/// Cedar type and doesn't collide with an existing static path. Shared by
/// `get_action_schema_with_overlay_json`, `compile_policy_with_overlay_json`,
/// and `validate_policy_with_overlay_json` so all three see the same
/// merged view — without this shared helper the builder UI could show a
/// field that compile/validate then rejected as unknown.
///
/// Two resolution paths:
/// - **Scalar** (`Long`, `String`, `Bool`, `decimal`, `Set<String>`,
///   `Set<Long>`): insert one optional top-level field. This is the common
///   case for user-added `outputs[].type` entries.
/// - **Record alias** (`UsdValuation`, `WindowStats`, …): look the leaf list
///   up in `aliases::record_leaves` and insert one `FieldSpec` per leaf
///   with `parent_path = field`, `parent_optional = true`. The generator
///   emits the matching `context.custom has <field>` + per-leaf guards
///   automatically because it keys off these flags.
///
/// Unknown spellings are skipped silently — manifest authors can declare
/// record types we don't recognise (e.g. a future `HookPermissions`
/// expansion), and the builder UI is best-effort overlay; the policy still
/// installs and evaluates fine via the engine.
fn apply_overlay(schema: &ActionSchema, overlay: &[OverlayEntry]) -> ActionSchema {
    let mut merged = schema.clone();
    for entry in overlay {
        if static_schema_covers_path(&merged, &entry.field) {
            continue;
        }
        if let Some(cedar_type) = parse_overlay_cedar_type(&entry.cedar_type) {
            insert_scalar_overlay(&mut merged, &entry.field, cedar_type);
        } else if let Some(leaves) = record_leaves(&entry.cedar_type) {
            insert_record_overlay(&mut merged, &entry.field, leaves);
        }
        // else: unknown spelling, skip
    }
    merged
}

fn insert_scalar_overlay(schema: &mut ActionSchema, field: &str, cedar_type: CedarType) {
    schema.fields.insert(
        field.to_string(),
        FieldSpec {
            path: field.to_string(),
            cedar_type,
            optional: true,
            parent_path: None,
            parent_optional: false,
            label: None,
            is_custom: true,
            allowed_values: None,
            scale: None,
            pattern: None,
        },
    );
}

fn insert_record_overlay(schema: &mut ActionSchema, parent: &str, leaves: &[AliasLeaf]) {
    for leaf in leaves {
        let path = format!("{parent}.{}", leaf.name);
        if schema.fields.contains_key(&path) {
            // A previous overlay entry or static field already declares the
            // same leaf — keep the existing one (static metadata wins).
            continue;
        }
        schema.fields.insert(
            path.clone(),
            FieldSpec {
                path,
                cedar_type: leaf.cedar_type,
                optional: leaf.optional,
                parent_path: Some(parent.to_string()),
                // The parent record itself is always optional under
                // `context.custom.*` — the manifest enrichment may not
                // populate it, and the generator needs the
                // `context.custom has <parent>` guard before any leaf
                // access. The leaf's own `optional` decides whether the
                // additional `context.custom.<parent> has <leaf>` guard
                // also fires.
                parent_optional: true,
                label: None,
                is_custom: true,
                allowed_values: None,
                scale: None,
                pattern: None,
            },
        );
    }
}

#[derive(Deserialize)]
struct OverlayInput {
    action: String,
    #[serde(default)]
    overlay: Vec<OverlayEntry>,
}

#[derive(Deserialize)]
struct OverlayEntry {
    field: String,
    #[serde(rename = "cedarType")]
    cedar_type: String,
}

/// Accept only scalar Cedar spellings for overlay entries. The strings
/// mirror `policy_engine::schema::aliases` scalar names so a manifest
/// `outputs[].type` of `"Long"` ↔ overlay `cedarType: "Long"`.
fn parse_overlay_cedar_type(spelling: &str) -> Option<CedarType> {
    match spelling {
        "Long" => Some(CedarType::Long),
        "String" => Some(CedarType::String),
        "Bool" => Some(CedarType::Bool),
        "decimal" => Some(CedarType::Decimal),
        "Set<String>" => Some(CedarType::SetOfString),
        "Set<Long>" => Some(CedarType::SetOfLong),
        _ => None,
    }
}

/// `true` when the static schema already exposes either the path itself or
/// any nested leaf under it (e.g. overlay `totalInputUsd` collides with
/// `totalInputUsd.value`). Both cases mean the static metadata is more
/// authoritative — leave the overlay entry out.
fn static_schema_covers_path(schema: &ActionSchema, field: &str) -> bool {
    if schema.fields.contains_key(field) {
        return true;
    }
    let prefix = format!("{field}.");
    schema.fields.keys().any(|k| k.starts_with(&prefix))
}

/// Compile a `PolicyRule` (JSON) into Cedar policy text.
///
/// `data` on success is `{ "cedar_text": "…" }`. On failure, `error.kind` is
/// one of `invalid_input_json`, `validation`, or `emit`.
#[wasm_bindgen]
#[must_use]
pub fn compile_policy_json(rule_json: String) -> String {
    let rule: PolicyRule = match serde_json::from_str(&rule_json) {
        Ok(r) => r,
        Err(error) => {
            return Envelope::<()>::failure(EnvelopeError {
                kind: "invalid_input_json".to_string(),
                message: error.to_string(),
                predicate_index: None,
            })
            .to_json();
        }
    };
    let registry = schemas::registry();
    let Some(schema) = registry.get(&rule.action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {}", rule.action),
            predicate_index: None,
        })
        .to_json();
    };

    match compile(&rule, schema) {
        Ok(text) => Envelope::success(CompileSuccess { cedar_text: text }).to_json(),
        Err(error) => Envelope::<()>::failure(compile_error_to_envelope(&error)).to_json(),
    }
}

/// Compile a rule against the static schema merged with caller-supplied
/// overlay fields. Same payload shape as
/// `get_action_schema_with_overlay_json` plus a `rule` slot:
///
/// ```text
/// { "action": "swap", "rule": {...}, "overlay": [{...}] }
/// ```
///
/// The `action` field is duplicated (also lives inside `rule.action`) so
/// the lookup mirrors the schema fetch path one-for-one and the input is
/// trivially shape-compatible with the schema-fetch payload. We validate
/// they match before doing work — an unintentional mismatch would let the
/// caller compile against the wrong schema.
///
/// Returns the same envelope shapes as `compile_policy_json`. Use this
/// whenever the builder UI showed the user any overlay fields — without
/// it, picking one of those fields and pressing Compile dead-ends with
/// `unknown_field` because the static registry doesn't know about them.
#[wasm_bindgen]
#[must_use]
pub fn compile_policy_with_overlay_json(input_json: String) -> String {
    let input: OverlayCompileInput = match serde_json::from_str(&input_json) {
        Ok(v) => v,
        Err(error) => {
            return Envelope::<()>::failure(EnvelopeError {
                kind: "invalid_input_json".to_string(),
                message: error.to_string(),
                predicate_index: None,
            })
            .to_json();
        }
    };
    if input.rule.action != input.action {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "invalid_input_json".to_string(),
            message: format!(
                "action mismatch: payload action={:?}, rule.action={:?}",
                input.action, input.rule.action
            ),
            predicate_index: None,
        })
        .to_json();
    }
    let registry = schemas::registry();
    let Some(schema) = registry.get(&input.action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {}", input.action),
            predicate_index: None,
        })
        .to_json();
    };
    let merged = apply_overlay(schema, &input.overlay);
    match compile(&input.rule, &merged) {
        Ok(text) => Envelope::success(CompileSuccess { cedar_text: text }).to_json(),
        Err(error) => Envelope::<()>::failure(compile_error_to_envelope(&error)).to_json(),
    }
}

/// Combined input for the compile/validate-with-overlay entry points.
///
/// Carries `action` redundantly with `rule.action` so the
/// schema-lookup step matches the no-overlay path bit-for-bit — easier to
/// read than picking the action out of the nested rule. The two are
/// cross-checked before any work happens.
#[derive(Deserialize)]
struct OverlayCompileInput {
    action: String,
    rule: PolicyRule,
    #[serde(default)]
    overlay: Vec<OverlayEntry>,
}

/// Parse Cedar policy text back to a `PolicyRule` (narrow subset, Phase 2).
///
/// `data` on success is the JSON-serialized `PolicyRule`. On failure,
/// `error.kind` is `parse_error` and `message` carries the underlying
/// [`ParseError`] description. Use this to attempt Code → Builder
/// round-trip after the user opted into raw Cedar editing.
#[wasm_bindgen]
#[must_use]
pub fn parse_cedar_json(cedar_text: String) -> String {
    match parse_cedar(&cedar_text) {
        Ok(mut rule) => {
            // Round-trip the inverse of the compile-time scale: when a
            // predicate names a scaled field (e.g. `inputAmountNano`,
            // scale = 9), the Cedar literal is `30000` but the user-facing
            // form is `0.00003`. Without this step a Cedar→Builder
            // round-trip would surface the raw rescaled integer and
            // confuse anyone reading the form.
            //
            // Look up the schema in the bundled registry rather than
            // re-deriving from the parsed action: parse_cedar already
            // populated `rule.action`, and an unknown action means there
            // is no schema to consult — we leave the rule untouched in
            // that case and let downstream validation report the issue.
            if let Some(schema) = schemas::registry().get(&rule.action) {
                unscale_predicate_values(&mut rule, schema);
            }
            Envelope::success(rule).to_json()
        }
        Err(error) => Envelope::<()>::failure(EnvelopeError {
            kind: parse_error_kind(&error).to_string(),
            message: error.to_string(),
            predicate_index: None,
        })
        .to_json(),
    }
}

/// For each predicate referencing a `Some(scale)` field, divide the Long
/// literal back into a decimal-shaped operand string. Silent no-op for
/// fields without a scale or operands the helper can't parse — the
/// downstream validator catches malformed values with field-aware errors.
fn unscale_predicate_values(rule: &mut PolicyRule, schema: &ActionSchema) {
    for predicate in &mut rule.predicates {
        let Some(field_spec) = schema.fields.get(&predicate.field) else {
            continue;
        };
        let Some(scale) = field_spec.scale else {
            continue;
        };
        match &mut predicate.value {
            PredicateValue::Single(v) => {
                if let Ok(decimal) = unscale_long_to_decimal(v, scale) {
                    *v = decimal;
                }
            }
            PredicateValue::Multi(vs) => {
                for v in vs {
                    if let Ok(decimal) = unscale_long_to_decimal(v, scale) {
                        *v = decimal;
                    }
                }
            }
            PredicateValue::None => {}
        }
    }
}

const fn parse_error_kind(error: &ParseError) -> &'static str {
    match error {
        ParseError::MissingAnnotation(_) => "missing_annotation",
        ParseError::InvalidSeverity(_) => "invalid_severity",
        ParseError::MalformedHead => "malformed_head",
        ParseError::MissingAction => "missing_action",
        ParseError::MalformedWhen => "malformed_when",
        ParseError::UnsupportedShape(_) => "unsupported_shape",
        ParseError::Escape(_) => "invalid_escape",
    }
}

/// Validate a rule without emitting Cedar — useful for live form feedback.
///
/// `data` on success is `null`. On failure, `error.kind` carries the
/// validation error code.
#[wasm_bindgen]
#[must_use]
pub fn validate_policy_json(rule_json: String) -> String {
    let rule: PolicyRule = match serde_json::from_str(&rule_json) {
        Ok(r) => r,
        Err(error) => {
            return Envelope::<()>::failure(EnvelopeError {
                kind: "invalid_input_json".to_string(),
                message: error.to_string(),
                predicate_index: None,
            })
            .to_json();
        }
    };
    let registry = schemas::registry();
    let Some(schema) = registry.get(&rule.action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {}", rule.action),
            predicate_index: None,
        })
        .to_json();
    };
    match validate(&rule, schema) {
        Ok(()) => Envelope::<Option<()>>::success(None).to_json(),
        Err(error) => Envelope::<()>::failure(validation_error_to_envelope(&error)).to_json(),
    }
}

/// Return every selector path the action exposes, tagged with its
/// Cedar type (for scalars) or record alias (for composite intermediate
/// paths).
///
/// Payload shape:
/// ```text
/// {
///   "action": "swap",
///   "scalars": [
///     { "path": "$.root.chain_id", "cedarType": "long" },
///     { "path": "$.action.inputToken.asset.address", "cedarType": "string" },
///     ...
///   ],
///   "records": [
///     { "path": "$.action.inputToken.asset", "cedarAlias": "AssetRef" },
///     { "path": "$.action.inputToken.amount", "cedarAlias": "AmountConstraint" },
///     ...
///   ]
/// }
/// ```
///
/// `$.root.*` paths are fixed across actions (`RootInput` in
/// `policy_engine::policy_rpc::manifest`); the action-specific list
/// comes from the bundled `swap.rs` / `swap::record_paths` pair so
/// the dashboard's selector picker can filter dropdowns to paths
/// whose type matches the catalog's declared param type — closes
/// the type-safety gap where a user could wire a `Long` selector
/// into a `String` slot and only learn at install time.
///
/// Custom fields (`is_custom == true`) are excluded; they live under
/// `context.custom.*`, not the calldata-derived `$.action.*` view this
/// endpoint exposes.
#[wasm_bindgen]
#[must_use]
pub fn get_typed_paths_for_action_json(action: String) -> String {
    let registry = schemas::registry();
    let Some(schema) = registry.get(&action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {action}"),
            predicate_index: None,
        })
        .to_json();
    };

    let mut scalars: Vec<TypedPathScalarDto> = Vec::new();
    let mut records: Vec<TypedPathRecordDto> = Vec::new();

    // $.root.* — `RootInput` shape, the same on every action. Listed
    // here rather than pulled from a schema because policy-builder
    // doesn't model the envelope (it only models per-action fields).
    for (path, cedar_type) in [
        ("$.root.chain_id", "long"),
        ("$.root.from", "string"),
        ("$.root.to", "string"),
        ("$.root.value_wei", "string"),
        ("$.root.block_timestamp", "long"),
    ] {
        scalars.push(TypedPathScalarDto {
            path: path.into(),
            cedar_type,
        });
    }

    // $.action.* — every leaf in the static schema, prefixed with the
    // `$.action.` root.
    for spec in schema.fields.values() {
        if spec.is_custom {
            continue;
        }
        scalars.push(TypedPathScalarDto {
            path: format!("$.action.{}", spec.path),
            cedar_type: cedar_type_str(spec.cedar_type),
        });
    }

    // $.action.* composite paths that wrap a known Cedar record alias.
    // Action-specific — pulled from the action's own `record_paths()`.
    for (path, cedar_alias) in record_paths_for_action(&action) {
        records.push(TypedPathRecordDto {
            path: format!("$.action.{path}"),
            cedar_alias,
        });
    }

    Envelope::success(TypedPathsDto {
        action: action.clone(),
        scalars,
        records,
    })
    .to_json()
}

fn record_paths_for_action(action: &str) -> Vec<(&'static str, &'static str)> {
    match action {
        "swap" => policy_builder::schemas::swap::record_paths(),
        _ => Vec::new(),
    }
}

#[derive(Serialize)]
struct TypedPathsDto {
    action: String,
    scalars: Vec<TypedPathScalarDto>,
    records: Vec<TypedPathRecordDto>,
}

#[derive(Serialize)]
struct TypedPathScalarDto {
    path: String,
    #[serde(rename = "cedarType")]
    cedar_type: &'static str,
}

#[derive(Serialize)]
struct TypedPathRecordDto {
    path: String,
    #[serde(rename = "cedarAlias")]
    cedar_alias: &'static str,
}

/// Validate a rule against the static schema merged with overlay fields.
/// Same input shape as `compile_policy_with_overlay_json`. Used for live
/// form feedback when the builder is rendering overlay fields — the
/// no-overlay variant would surface `unknown_field` on every keystroke.
#[wasm_bindgen]
#[must_use]
pub fn validate_policy_with_overlay_json(input_json: String) -> String {
    let input: OverlayCompileInput = match serde_json::from_str(&input_json) {
        Ok(v) => v,
        Err(error) => {
            return Envelope::<()>::failure(EnvelopeError {
                kind: "invalid_input_json".to_string(),
                message: error.to_string(),
                predicate_index: None,
            })
            .to_json();
        }
    };
    if input.rule.action != input.action {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "invalid_input_json".to_string(),
            message: format!(
                "action mismatch: payload action={:?}, rule.action={:?}",
                input.action, input.rule.action
            ),
            predicate_index: None,
        })
        .to_json();
    }
    let registry = schemas::registry();
    let Some(schema) = registry.get(&input.action) else {
        return Envelope::<()>::failure(EnvelopeError {
            kind: "unknown_action".to_string(),
            message: format!("no schema registered for action: {}", input.action),
            predicate_index: None,
        })
        .to_json();
    };
    let merged = apply_overlay(schema, &input.overlay);
    match validate(&input.rule, &merged) {
        Ok(()) => Envelope::<Option<()>>::success(None).to_json(),
        Err(error) => Envelope::<()>::failure(validation_error_to_envelope(&error)).to_json(),
    }
}

#[derive(Serialize)]
struct CompileSuccess {
    cedar_text: String,
}

#[derive(Serialize)]
struct ActionSchemaDto {
    action: String,
    #[serde(rename = "principalType")]
    principal_type: String,
    #[serde(rename = "resourceType")]
    resource_type: String,
    fields: Vec<FieldDto>,
}

#[derive(Serialize)]
struct FieldDto {
    path: String,
    #[serde(rename = "type")]
    cedar_type: &'static str,
    optional: bool,
    #[serde(rename = "parentPath", skip_serializing_if = "Option::is_none")]
    parent_path: Option<String>,
    #[serde(rename = "parentOptional")]
    parent_optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    /// `true` when the field lives under `context.custom.<path>` (a
    /// manifest-enriched extension), `false` for base calldata fields under
    /// `context.<path>`. UIs use this to group/style custom fields
    /// distinctly without re-deriving the split from the cedarschema.
    #[serde(rename = "isCustom")]
    is_custom: bool,
    /// Closed-set enum constraint for this field's operand value(s). When
    /// present, the UI should render a `<select>` (arity=one) or multi-select
    /// (arity=many) of these literals instead of a free-form input. Omitted
    /// when the field accepts any well-typed value.
    #[serde(rename = "allowedValues", skip_serializing_if = "Option::is_none")]
    allowed_values: Option<Vec<String>>,
    /// Implicit `10^scale` exponent for Long fields whose runtime value the
    /// manifest pre-rescaled (e.g. token-native amount fields at scale 9).
    /// When present, the UI should accept decimal input from the user and
    /// the WASM compiler will emit `value × 10^scale` as the Long literal.
    /// Hint for placeholders, validation feedback, and back-display.
    #[serde(skip_serializing_if = "Option::is_none")]
    scale: Option<u8>,
    /// Regex the operand string must match (e.g. `^0x[0-9a-fA-F]{40}$` for
    /// EVM address fields, sourced from the upstream action-schema JSON's
    /// `"pattern"` keyword). The WASM validator enforces it and surfaces
    /// `kind: "pattern_mismatch"` on violation; UIs can additionally use
    /// it for live form feedback (red border on out-of-shape input).
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
    operators: Vec<OperatorDto>,
}

#[derive(Serialize)]
struct OperatorDto {
    id: &'static str,
    label: &'static str,
    arity: &'static str,
}

fn schema_to_dto(schema: &ActionSchema) -> ActionSchemaDto {
    let fields = schema.fields.values().map(field_to_dto).collect();
    ActionSchemaDto {
        action: schema.action.clone(),
        principal_type: schema.principal_type.clone(),
        resource_type: schema.resource_type.clone(),
        fields,
    }
}

fn field_to_dto(spec: &FieldSpec) -> FieldDto {
    let operators = operators_for(spec.cedar_type)
        .iter()
        .map(|op| OperatorDto {
            id: op.id,
            label: op.label,
            arity: arity_str(op.arity),
        })
        .collect();
    FieldDto {
        path: spec.path.clone(),
        cedar_type: cedar_type_str(spec.cedar_type),
        optional: spec.optional,
        parent_path: spec.parent_path.clone(),
        parent_optional: spec.parent_optional,
        label: spec.label.clone(),
        is_custom: spec.is_custom,
        allowed_values: spec.allowed_values.clone(),
        scale: spec.scale,
        pattern: spec.pattern.clone(),
        operators,
    }
}

const fn cedar_type_str(t: CedarType) -> &'static str {
    match t {
        CedarType::Long => "long",
        CedarType::String => "string",
        CedarType::Bool => "bool",
        CedarType::Decimal => "decimal",
        CedarType::SetOfString => "set_of_string",
        CedarType::SetOfLong => "set_of_long",
    }
}

const fn arity_str(a: OperatorArity) -> &'static str {
    match a {
        OperatorArity::One => "one",
        OperatorArity::Many => "many",
        OperatorArity::None => "none",
    }
}

fn validation_error_to_envelope(error: &ValidationError) -> EnvelopeError {
    let (kind, predicate_index) = match error {
        ValidationError::UnknownAction(_) => ("unknown_action", None),
        ValidationError::EmptyId => ("empty_id", None),
        ValidationError::UnknownField { index, .. } => ("unknown_field", Some(*index)),
        ValidationError::UnknownOperator { index, .. } => ("unknown_operator", Some(*index)),
        ValidationError::ArityMismatch { index, .. } => ("arity_mismatch", Some(*index)),
        ValidationError::EmptyOperandList { index, .. } => ("empty_operand_list", Some(*index)),
        ValidationError::DisallowedValue { index, .. } => ("disallowed_value", Some(*index)),
        ValidationError::PatternMismatch { index, .. } => ("pattern_mismatch", Some(*index)),
        ValidationError::InvalidPattern { index, .. } => ("invalid_pattern", Some(*index)),
    };
    EnvelopeError {
        kind: kind.to_string(),
        message: error.to_string(),
        predicate_index,
    }
}

fn compile_error_to_envelope(error: &CompileError) -> EnvelopeError {
    match error {
        CompileError::Validation(v) => validation_error_to_envelope(v),
        CompileError::Emit { index, .. } => EnvelopeError {
            kind: "emit".to_string(),
            message: error.to_string(),
            predicate_index: Some(*index),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod overlay_tests {
    use super::*;
    use serde_json::Value;

    fn call_overlay(input: &str) -> Value {
        let raw = get_action_schema_with_overlay_json(input.to_string());
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn scalar_overlay_field_is_appended_with_is_custom() {
        let out = call_overlay(
            r#"{"action":"swap","overlay":[{"field":"myRiskScore","cedarType":"Long"}]}"#,
        );
        assert_eq!(out["ok"], Value::Bool(true));
        let fields = out["data"]["fields"].as_array().unwrap();
        let added = fields
            .iter()
            .find(|f| f["path"] == "myRiskScore")
            .expect("overlay field present");
        assert_eq!(added["type"], "long");
        assert_eq!(added["isCustom"], Value::Bool(true));
        assert_eq!(added["optional"], Value::Bool(true));
        // Long operators (gt/lt/…) come through the same operators_for path
        // the static fields use, so the UI gets matching dropdowns.
        assert!(added["operators"].as_array().unwrap().iter().any(|o| o["id"] == "gt"));
    }

    #[test]
    fn record_alias_in_overlay_expands_to_leaves() {
        // Post-Phase 8: overlay accepts record aliases too. Adding a new
        // UsdValuation-typed field via the manifest editor should surface
        // all four leaves in the builder picker, each rooted under
        // `<field>.<leaf>` with the optional-custom guard cluster.
        let out = call_overlay(
            r#"{"action":"swap","overlay":[{"field":"customUsd","cedarType":"UsdValuation"}]}"#,
        );
        assert_eq!(out["ok"], Value::Bool(true), "got: {out:?}");
        let fields = out["data"]["fields"].as_array().unwrap();
        for leaf in ["value", "asOfTs", "staleSec", "sources"] {
            let expected_path = format!("customUsd.{leaf}");
            let leaf_field = fields
                .iter()
                .find(|f| f["path"] == expected_path)
                .unwrap_or_else(|| panic!("missing leaf {expected_path}"));
            assert_eq!(leaf_field["isCustom"], Value::Bool(true));
            assert_eq!(leaf_field["parentPath"], "customUsd");
            assert_eq!(leaf_field["parentOptional"], Value::Bool(true));
        }
        // The top-level `customUsd` itself doesn't materialise as its own
        // field — only its leaves do, mirroring how `totalInputUsd` works
        // in the static schema.
        assert!(!fields.iter().any(|f| f["path"] == "customUsd"));
    }

    #[test]
    fn unknown_alias_in_overlay_is_silently_skipped() {
        // A spelling that's neither scalar nor a known record (e.g. a
        // typo or a future alias) gets dropped quietly — the policy can
        // still install/evaluate via the engine, the builder UI just
        // can't surface it.
        let out = call_overlay(
            r#"{"action":"swap","overlay":[{"field":"x","cedarType":"NotARealType"}]}"#,
        );
        assert_eq!(out["ok"], Value::Bool(true));
        let fields = out["data"]["fields"].as_array().unwrap();
        assert!(!fields.iter().any(|f| f["path"] == "x"));
    }

    #[test]
    fn overlay_skips_when_path_collides_with_static_base() {
        // `inputToken` is a base record with static leaves (`asset.address`,
        // `amount.value`, etc.). Dropping an overlay `inputToken: Long`
        // would shadow that record — static schema metadata always wins
        // because it carries the richer label/enum/pattern data the
        // overlay can't reproduce. Same dedup guard also protects
        // `feeBps`, `recipient`, etc. when an overlay accidentally
        // duplicates them.
        let out = call_overlay(
            r#"{"action":"swap","overlay":[{"field":"inputToken","cedarType":"Long"}]}"#,
        );
        let fields = out["data"]["fields"].as_array().unwrap();
        // Static leaves still there; no synthetic Long shadow.
        assert!(fields
            .iter()
            .any(|f| f["path"] == "inputToken.asset.address" && f["type"] == "string"));
        let leaked = fields
            .iter()
            .find(|f| f["path"] == "inputToken" && f["type"] == "long");
        assert!(
            leaked.is_none(),
            "overlay leaked into static-covered path: {leaked:?}"
        );
    }

    #[test]
    fn overlay_does_not_shadow_static_scalar() {
        // Direct scalar collision: `feeBps` is a base Long. Overlaying it
        // as anything else should be a no-op.
        let out = call_overlay(
            r#"{"action":"swap","overlay":[{"field":"feeBps","cedarType":"String"}]}"#,
        );
        let fields = out["data"]["fields"].as_array().unwrap();
        let fee = fields
            .iter()
            .find(|f| f["path"] == "feeBps")
            .expect("static feeBps still present");
        assert_eq!(fee["type"], "long", "overlay must not retype a base field");
        assert_eq!(fee["isCustom"], Value::Bool(false));
    }

    #[test]
    fn empty_overlay_matches_plain_schema_fetch() {
        let plain: Value =
            serde_json::from_str(&get_action_schema_json("swap".into())).unwrap();
        let overlaid = call_overlay(r#"{"action":"swap","overlay":[]}"#);
        assert_eq!(plain["data"], overlaid["data"]);
    }

    #[test]
    fn unknown_action_surfaces_envelope_error() {
        let out = call_overlay(r#"{"action":"no-such","overlay":[]}"#);
        assert_eq!(out["ok"], Value::Bool(false));
        assert_eq!(out["error"]["kind"], "unknown_action");
    }

    fn call_compile_overlay(input: &str) -> Value {
        let raw = compile_policy_with_overlay_json(input.to_string());
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn compile_with_overlay_emits_cedar_for_overlay_scalar_field() {
        // The whole point of overlay-in-compile: a user adds `myRiskScore: Long`
        // via the manifest editor, picks it in the builder dropdown, and
        // presses Compile. Without compile-side overlay this dead-ends with
        // unknown_field; with overlay it should emit a Cedar comparison
        // under `context.custom.myRiskScore`.
        let input = serde_json::json!({
            "action": "swap",
            "rule": {
                "id": "user/risk",
                "action": "swap",
                "severity": "deny",
                "reason": "risk score too high",
                "predicates": [{
                    "field": "myRiskScore",
                    "op": "gt",
                    "value": "80"
                }]
            },
            "overlay": [{ "field": "myRiskScore", "cedarType": "Long" }]
        })
        .to_string();
        let out = call_compile_overlay(&input);
        assert_eq!(out["ok"], Value::Bool(true), "got: {out:?}");
        let cedar_text = out["data"]["cedar_text"].as_str().unwrap();
        assert!(
            cedar_text.contains("context.custom.myRiskScore > 80"),
            "expected scaled overlay comparison, got:\n{cedar_text}"
        );
        // Optional-custom guard cluster should still fire for the
        // user-added field (it lands as is_custom=true, optional=true).
        assert!(cedar_text.contains("context has custom"));
        assert!(cedar_text.contains("context.custom has myRiskScore"));
    }

    #[test]
    fn compile_with_overlay_action_mismatch_rejected() {
        // Defence-in-depth: if the wrapper accidentally puts a different
        // action on the envelope than the rule carries, we'd compile
        // against the wrong schema and silently produce invalid policies.
        let input = serde_json::json!({
            "action": "swap",
            "rule": {
                "id": "user/x",
                "action": "wrong-action",
                "severity": "deny",
                "reason": "",
                "predicates": []
            },
            "overlay": []
        })
        .to_string();
        let out = call_compile_overlay(&input);
        assert_eq!(out["ok"], Value::Bool(false));
        assert_eq!(out["error"]["kind"], "invalid_input_json");
    }

    #[test]
    fn compile_with_overlay_no_overlay_matches_plain_compile() {
        // A no-op overlay must round-trip identically — otherwise callers
        // that always go through the overlay path (e.g. BuilderView) would
        // see different Cedar text from no-overlay callers.
        let rule = serde_json::json!({
            "id": "user/fee",
            "action": "swap",
            "severity": "deny",
            "reason": "",
            "predicates": [{
                "field": "feeBps",
                "op": "gt",
                "value": "100"
            }]
        });
        let plain: Value =
            serde_json::from_str(&compile_policy_json(rule.to_string())).unwrap();
        let overlay_in = serde_json::json!({
            "action": "swap",
            "rule": rule,
            "overlay": []
        });
        let overlaid = call_compile_overlay(&overlay_in.to_string());
        assert_eq!(plain["data"], overlaid["data"]);
    }

    fn call_typed_paths(action: &str) -> Value {
        let raw = get_typed_paths_for_action_json(action.to_string());
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn typed_paths_lists_root_envelope_constants() {
        let out = call_typed_paths("swap");
        assert_eq!(out["ok"], Value::Bool(true));
        let scalars = out["data"]["scalars"].as_array().unwrap();
        let by_path: std::collections::HashMap<&str, &str> = scalars
            .iter()
            .map(|s| {
                (
                    s["path"].as_str().unwrap(),
                    s["cedarType"].as_str().unwrap(),
                )
            })
            .collect();
        // $.root.* shape is the same on every action — assert the
        // canonical set.
        assert_eq!(by_path.get("$.root.chain_id"), Some(&"long"));
        assert_eq!(by_path.get("$.root.from"), Some(&"string"));
        assert_eq!(by_path.get("$.root.value_wei"), Some(&"string"));
        assert_eq!(by_path.get("$.root.block_timestamp"), Some(&"long"));
    }

    #[test]
    fn typed_paths_lists_action_scalars_with_correct_types() {
        let out = call_typed_paths("swap");
        let scalars = out["data"]["scalars"].as_array().unwrap();
        // Just spot-check a couple — `inputToken.asset.address` is String,
        // `inputToken.asset.decimals` is Long. Picker uses these to
        // decide which slots a path is eligible for.
        let by_path: std::collections::HashMap<&str, &str> = scalars
            .iter()
            .map(|s| {
                (
                    s["path"].as_str().unwrap(),
                    s["cedarType"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            by_path.get("$.action.inputToken.asset.address"),
            Some(&"string"),
        );
        assert_eq!(
            by_path.get("$.action.inputToken.asset.decimals"),
            Some(&"long"),
        );
        assert_eq!(by_path.get("$.action.feeBps"), Some(&"long"));
    }

    #[test]
    fn typed_paths_lists_composite_records_with_alias() {
        // The Phase 8.5 / PR 4 point: a `param.type = "AssetRef"` slot
        // needs paths like `$.action.inputToken.asset` (whole record)
        // surfaced — not the String leaves under it. The record-paths
        // list is what supplies them.
        let out = call_typed_paths("swap");
        let records = out["data"]["records"].as_array().unwrap();
        let by_path: std::collections::HashMap<&str, &str> = records
            .iter()
            .map(|s| {
                (
                    s["path"].as_str().unwrap(),
                    s["cedarAlias"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            by_path.get("$.action.inputToken.asset"),
            Some(&"AssetRef"),
        );
        assert_eq!(
            by_path.get("$.action.outputToken.asset"),
            Some(&"AssetRef"),
        );
        assert_eq!(
            by_path.get("$.action.inputToken"),
            Some(&"AssetRefWithAmountConstraint"),
        );
        assert_eq!(
            by_path.get("$.action.validity"),
            Some(&"Validity"),
        );
    }

    #[test]
    fn typed_paths_unknown_action_returns_error() {
        let out = call_typed_paths("no-such-action");
        assert_eq!(out["ok"], Value::Bool(false));
        assert_eq!(out["error"]["kind"], "unknown_action");
    }

    #[test]
    fn validate_with_overlay_accepts_overlay_field() {
        // Mirror of compile-side test for the validator: feedback during
        // form edits must not flag an overlay field as unknown.
        let input = serde_json::json!({
            "action": "swap",
            "rule": {
                "id": "user/risk",
                "action": "swap",
                "severity": "deny",
                "reason": "x",
                "predicates": [{
                    "field": "myRiskScore",
                    "op": "gt",
                    "value": "80"
                }]
            },
            "overlay": [{ "field": "myRiskScore", "cedarType": "Long" }]
        })
        .to_string();
        let raw = validate_policy_with_overlay_json(input);
        let out: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(out["ok"], Value::Bool(true), "got: {out:?}");
    }
}
