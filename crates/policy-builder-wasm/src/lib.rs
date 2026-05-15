//! WASM bridge for `policy-builder`.
//!
//! The bridge exposes a JSON-string boundary for TypeScript callers:
//! - `list_actions()` — known action names.
//! - `get_action_schema_json(action)` — schema for one action, including the
//!   valid operators for each field (so the UI can drive its dropdowns from
//!   one source of truth without redeclaring the operator table in TS).
//! - `compile_policy_json(rule_json)` — `PolicyRule` -> Cedar text or
//!   structured error.
//!
//! All return values are wrapped in an `Envelope { ok, data?, error? }`
//! shape so the TS side has a single discriminant to branch on regardless
//! of which call produced the response.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use policy_builder::operators::{operators_for, OperatorArity};
use policy_builder::schemas;
use policy_builder::types::{ActionSchema, CedarType, FieldSpec, PolicyRule};
use policy_builder::{compile, validate, CompileError, ValidationError};
use serde::Serialize;
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
///       "operators": [
///         { "id": "gt", "label": ">", "arity": "one" },
///         …
///       ]
///     },
///     …
///   ]
/// }
/// ```
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
