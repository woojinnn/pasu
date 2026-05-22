//! Per-type operator table.
//!
//! Each [`CedarType`] has a fixed set of operators. The generator consults
//! this table to (a) decide whether a [`Predicate`]'s `op` is legal for its
//! field and (b) emit the Cedar fragment for the left-hand expression and the
//! escaped operand(s).
//!
//! Adding a new operator means appending one [`Operator`] to the relevant
//! type's slice — no other generator code changes.
//!
//! [`Predicate`]: crate::types::Predicate

use crate::escape::{
    escape_decimal, escape_long, escape_string, normalize_decimal_input, normalize_long_input,
    EscapeError,
};
use crate::types::{CedarType, PredicateValue};
use thiserror::Error;

/// How many operands an operator takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorArity {
    /// Exactly one operand (e.g. `> 100`, `== "WETH"`).
    One,
    /// A list of operands (e.g. `in ["A","B"]`, `.containsAny([…])`).
    Many,
    /// No operand (e.g. `is true` / `is false`).
    None,
}

/// Emit failures bubbled from [`Operator::emit`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmitError {
    /// Predicate's `value` shape didn't match the operator's [`OperatorArity`].
    #[error("operator {op} expects {expected:?} operand(s), got {actual}")]
    ArityMismatch {
        /// Operator id.
        op: &'static str,
        /// Required arity.
        expected: OperatorArity,
        /// What was provided ("single", "multi", "none").
        actual: &'static str,
    },
    /// One of the operands failed Cedar-literal validation.
    #[error("operand for {op}: {source}")]
    BadOperand {
        /// Operator id.
        op: &'static str,
        /// Underlying escape error.
        #[source]
        source: EscapeError,
    },
}

/// One operator definition for one type.
#[derive(Debug, Clone, Copy)]
pub struct Operator {
    /// Stable id used inside [`crate::types::Predicate`].
    pub id: &'static str,
    /// Display label for UIs.
    pub label: &'static str,
    /// Required operand shape.
    pub arity: OperatorArity,
    emit_fn: fn(&str, &PredicateValue) -> Result<String, EmitError>,
}

impl Operator {
    /// Produce the Cedar fragment for this operator.
    ///
    /// `left` is the left-hand expression — typically `context.<path>`.
    ///
    /// # Errors
    ///
    /// Returns [`EmitError`] when the predicate value doesn't match the
    /// operator's arity or when an operand fails Cedar-literal validation.
    pub fn emit(&self, left: &str, value: &PredicateValue) -> Result<String, EmitError> {
        (self.emit_fn)(left, value)
    }
}

/// Look up an operator by `cedar_type` and `op` id.
#[must_use]
pub fn find(cedar_type: CedarType, op_id: &str) -> Option<&'static Operator> {
    operators_for(cedar_type).iter().find(|op| op.id == op_id)
}

/// All operators valid for the given Cedar type.
#[must_use]
pub const fn operators_for(cedar_type: CedarType) -> &'static [Operator] {
    match cedar_type {
        CedarType::Long => LONG_OPS,
        CedarType::String => STRING_OPS,
        CedarType::Bool => BOOL_OPS,
        CedarType::Decimal => DECIMAL_OPS,
        CedarType::SetOfString => SET_STRING_OPS,
        CedarType::SetOfLong => SET_LONG_OPS,
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn one_operand<'a>(op: &'static str, value: &'a PredicateValue) -> Result<&'a str, EmitError> {
    match value {
        PredicateValue::Single(s) => Ok(s.as_str()),
        PredicateValue::Multi(_) => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::One,
            actual: "multi",
        }),
        PredicateValue::None => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::One,
            actual: "none",
        }),
    }
}

fn many_operands<'a>(
    op: &'static str,
    value: &'a PredicateValue,
) -> Result<&'a [String], EmitError> {
    match value {
        PredicateValue::Multi(vs) => Ok(vs.as_slice()),
        PredicateValue::Single(_) => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::Many,
            actual: "single",
        }),
        PredicateValue::None => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::Many,
            actual: "none",
        }),
    }
}

fn no_operands(op: &'static str, value: &PredicateValue) -> Result<(), EmitError> {
    match value {
        PredicateValue::None => Ok(()),
        PredicateValue::Single(_) => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::None,
            actual: "single",
        }),
        PredicateValue::Multi(_) => Err(EmitError::ArityMismatch {
            op,
            expected: OperatorArity::None,
            actual: "multi",
        }),
    }
}

fn bad_operand(op: &'static str, source: EscapeError) -> EmitError {
    EmitError::BadOperand { op, source }
}

// ── emit functions ──────────────────────────────────────────────────────────

// Long
fn emit_long_cmp(
    symbol: &'static str,
    op: &'static str,
) -> impl Fn(&str, &PredicateValue) -> Result<String, EmitError> {
    move |left, value| {
        let raw = one_operand(op, value)?;
        // Coerce fractional-zero shapes (`"1.0"`, `"100.00"`) so users
        // who copy-paste integers from DEX UIs aren't told their Long
        // input is invalid. Non-zero fractional digits still fail —
        // we never silently round.
        let normalized = normalize_long_input(raw).map_err(|e| bad_operand(op, e))?;
        let rendered = escape_long(&normalized).map_err(|e| bad_operand(op, e))?;
        Ok(format!("{left} {symbol} {rendered}"))
    }
}

fn emit_long_gt(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp(">", "gt")(left, v)
}
fn emit_long_gte(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp(">=", "gte")(left, v)
}
fn emit_long_lt(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp("<", "lt")(left, v)
}
fn emit_long_lte(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp("<=", "lte")(left, v)
}
fn emit_long_eq(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp("==", "eq")(left, v)
}
fn emit_long_ne(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_long_cmp("!=", "ne")(left, v)
}

const LONG_OPS: &[Operator] = &[
    Operator {
        id: "gt",
        label: ">",
        arity: OperatorArity::One,
        emit_fn: emit_long_gt,
    },
    Operator {
        id: "gte",
        label: ">=",
        arity: OperatorArity::One,
        emit_fn: emit_long_gte,
    },
    Operator {
        id: "lt",
        label: "<",
        arity: OperatorArity::One,
        emit_fn: emit_long_lt,
    },
    Operator {
        id: "lte",
        label: "<=",
        arity: OperatorArity::One,
        emit_fn: emit_long_lte,
    },
    Operator {
        id: "eq",
        label: "==",
        arity: OperatorArity::One,
        emit_fn: emit_long_eq,
    },
    Operator {
        id: "ne",
        label: "!=",
        arity: OperatorArity::One,
        emit_fn: emit_long_ne,
    },
];

// String
fn emit_string_eq(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let raw = one_operand("eq", v)?;
    Ok(format!("{left} == {}", escape_string(raw)))
}

fn emit_string_ne(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let raw = one_operand("ne", v)?;
    Ok(format!("{left} != {}", escape_string(raw)))
}

fn emit_string_in(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("in", v)?;
    let body = xs
        .iter()
        .map(|s| escape_string(s))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("[{body}].contains({left})"))
}

const STRING_OPS: &[Operator] = &[
    Operator {
        id: "eq",
        label: "==",
        arity: OperatorArity::One,
        emit_fn: emit_string_eq,
    },
    Operator {
        id: "ne",
        label: "!=",
        arity: OperatorArity::One,
        emit_fn: emit_string_ne,
    },
    Operator {
        id: "in",
        label: "in",
        arity: OperatorArity::Many,
        emit_fn: emit_string_in,
    },
];

// Bool
fn emit_bool_true(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    no_operands("isTrue", v)?;
    Ok(left.to_string())
}

fn emit_bool_false(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    no_operands("isFalse", v)?;
    Ok(format!("!{left}"))
}

const BOOL_OPS: &[Operator] = &[
    Operator {
        id: "isTrue",
        label: "is true",
        arity: OperatorArity::None,
        emit_fn: emit_bool_true,
    },
    Operator {
        id: "isFalse",
        label: "is false",
        arity: OperatorArity::None,
        emit_fn: emit_bool_false,
    },
];

// Decimal
fn emit_decimal_method(
    method: &'static str,
    op: &'static str,
) -> impl Fn(&str, &PredicateValue) -> Result<String, EmitError> {
    move |left, value| {
        let raw = one_operand(op, value)?;
        // Coerce UI-friendly shapes (`"1"`, `".5"`, `"1."`) into Cedar's
        // strict `<digits>.<frac>` form before validating. Without this,
        // the user has to remember to type the trailing `.0` themselves —
        // a footgun for any field typed as `decimal` (USD valuations,
        // ratios, etc.).
        let normalized = normalize_decimal_input(raw).map_err(|e| bad_operand(op, e))?;
        let rendered = escape_decimal(&normalized).map_err(|e| bad_operand(op, e))?;
        Ok(format!("{left}.{method}({rendered})"))
    }
}

fn emit_decimal_gt(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_decimal_method("greaterThan", "gt")(left, v)
}
fn emit_decimal_gte(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_decimal_method("greaterThanOrEqual", "gte")(left, v)
}
fn emit_decimal_lt(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_decimal_method("lessThan", "lt")(left, v)
}
fn emit_decimal_lte(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    emit_decimal_method("lessThanOrEqual", "lte")(left, v)
}

const DECIMAL_OPS: &[Operator] = &[
    Operator {
        id: "gt",
        label: ">",
        arity: OperatorArity::One,
        emit_fn: emit_decimal_gt,
    },
    Operator {
        id: "gte",
        label: ">=",
        arity: OperatorArity::One,
        emit_fn: emit_decimal_gte,
    },
    Operator {
        id: "lt",
        label: "<",
        arity: OperatorArity::One,
        emit_fn: emit_decimal_lt,
    },
    Operator {
        id: "lte",
        label: "<=",
        arity: OperatorArity::One,
        emit_fn: emit_decimal_lte,
    },
];

// Set<String>
fn emit_set_string_contains(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let raw = one_operand("contains", v)?;
    Ok(format!("{left}.contains({})", escape_string(raw)))
}

fn emit_set_string_contains_any(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("containsAny", v)?;
    let body = xs
        .iter()
        .map(|s| escape_string(s))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("{left}.containsAny([{body}])"))
}

fn emit_set_string_contains_all(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("containsAll", v)?;
    let body = xs
        .iter()
        .map(|s| escape_string(s))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("{left}.containsAll([{body}])"))
}

const SET_STRING_OPS: &[Operator] = &[
    Operator {
        id: "contains",
        label: "contains",
        arity: OperatorArity::One,
        emit_fn: emit_set_string_contains,
    },
    Operator {
        id: "containsAny",
        label: "contains any of",
        arity: OperatorArity::Many,
        emit_fn: emit_set_string_contains_any,
    },
    Operator {
        id: "containsAll",
        label: "contains all of",
        arity: OperatorArity::Many,
        emit_fn: emit_set_string_contains_all,
    },
];

// Set<Long>
//
// All three Set<Long> emitters share the same fractional-zero
// tolerance Long comparison gained above — keeps `"1, 2.0, 3"` from
// failing on the middle element when the user's source UI rendered
// integers with a trailing `.0`.
fn emit_set_long_contains(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let raw = one_operand("contains", v)?;
    let normalized = normalize_long_input(raw).map_err(|e| bad_operand("contains", e))?;
    let rendered = escape_long(&normalized).map_err(|e| bad_operand("contains", e))?;
    Ok(format!("{left}.contains({rendered})"))
}

fn emit_set_long_contains_any(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("containsAny", v)?;
    let mut rendered_parts = Vec::with_capacity(xs.len());
    for x in xs {
        let normalized = normalize_long_input(x).map_err(|e| bad_operand("containsAny", e))?;
        rendered_parts.push(escape_long(&normalized).map_err(|e| bad_operand("containsAny", e))?);
    }
    Ok(format!(
        "{left}.containsAny([{}])",
        rendered_parts.join(", ")
    ))
}

fn emit_set_long_contains_all(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("containsAll", v)?;
    let mut rendered_parts = Vec::with_capacity(xs.len());
    for x in xs {
        let normalized = normalize_long_input(x).map_err(|e| bad_operand("containsAll", e))?;
        rendered_parts.push(escape_long(&normalized).map_err(|e| bad_operand("containsAll", e))?);
    }
    Ok(format!(
        "{left}.containsAll([{}])",
        rendered_parts.join(", ")
    ))
}

const SET_LONG_OPS: &[Operator] = &[
    Operator {
        id: "contains",
        label: "contains",
        arity: OperatorArity::One,
        emit_fn: emit_set_long_contains,
    },
    Operator {
        id: "containsAny",
        label: "contains any of",
        arity: OperatorArity::Many,
        emit_fn: emit_set_long_contains_any,
    },
    Operator {
        id: "containsAll",
        label: "contains all of",
        arity: OperatorArity::Many,
        emit_fn: emit_set_long_contains_all,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_gt_emits_plain_comparison() {
        let op = find(CedarType::Long, "gt").unwrap();
        let out = op
            .emit("context.feeBps", &PredicateValue::Single("100".into()))
            .unwrap();
        assert_eq!(out, "context.feeBps > 100");
    }

    #[test]
    fn decimal_gt_emits_method_call() {
        // The left-hand expression is the generator's responsibility; the
        // operator just substitutes whatever it's given. In v1, custom
        // fields arrive prefixed with `context.custom.` — exercise that
        // shape here so the regression bar is visible.
        let op = find(CedarType::Decimal, "gt").unwrap();
        let out = op
            .emit(
                "context.custom.totalInputUsd.value",
                &PredicateValue::Single("100.00".into()),
            )
            .unwrap();
        assert_eq!(
            out,
            r#"context.custom.totalInputUsd.value.greaterThan(decimal("100.00"))"#
        );
    }

    #[test]
    fn decimal_gt_accepts_integer_input_via_normalization() {
        // Previously a user typing `1` into a decimal field hit
        // "invalid decimal literal: 1" because Cedar's `decimal()`
        // parser demands a fractional part. The normalizer in
        // `emit_decimal_method` now coerces `1` → `1.0` so the same
        // input compiles to the same literal as if the user had typed
        // `1.0` explicitly.
        let op = find(CedarType::Decimal, "gt").unwrap();
        let out = op
            .emit(
                "context.custom.totalInputUsd.value",
                &PredicateValue::Single("1".into()),
            )
            .unwrap();
        assert_eq!(
            out,
            r#"context.custom.totalInputUsd.value.greaterThan(decimal("1.0"))"#
        );

        // Other loose UI shapes the normalizer covers — make sure they
        // all reach the same operator emission path without erroring.
        for (raw, expected_literal) in [
            (".5", "0.5"),
            ("1.", "1.0"),
            ("-1", "-1.0"),
            ("-.25", "-0.25"),
        ] {
            let emitted = op
                .emit(
                    "context.custom.totalInputUsd.value",
                    &PredicateValue::Single(raw.into()),
                )
                .unwrap();
            assert_eq!(
                emitted,
                format!(
                    r#"context.custom.totalInputUsd.value.greaterThan(decimal("{expected_literal}"))"#
                ),
                "raw={raw}"
            );
        }
    }

    #[test]
    fn string_eq_quotes_value() {
        let op = find(CedarType::String, "eq").unwrap();
        let out = op
            .emit(
                "context.swapMode",
                &PredicateValue::Single("exact_in".into()),
            )
            .unwrap();
        assert_eq!(out, r#"context.swapMode == "exact_in""#);
    }

    #[test]
    fn string_in_emits_array_membership() {
        let op = find(CedarType::String, "in").unwrap();
        let out = op
            .emit(
                "context.swapMode",
                &PredicateValue::Multi(vec!["exact_in".into(), "market".into()]),
            )
            .unwrap();
        assert_eq!(out, r#"["exact_in", "market"].contains(context.swapMode)"#);
    }

    #[test]
    fn bool_true_emits_bare_field() {
        let op = find(CedarType::Bool, "isTrue").unwrap();
        let out = op
            .emit("context.recipientIsContract", &PredicateValue::None)
            .unwrap();
        assert_eq!(out, "context.recipientIsContract");
    }

    #[test]
    fn bool_false_negates_field() {
        let op = find(CedarType::Bool, "isFalse").unwrap();
        let out = op
            .emit("context.recipientIsContract", &PredicateValue::None)
            .unwrap();
        assert_eq!(out, "!context.recipientIsContract");
    }

    #[test]
    fn set_string_contains_any() {
        // Exercise a custom-prefixed left-hand expression — set operators
        // are field-agnostic but the v1 wire shape is `context.custom.<path>`.
        let op = find(CedarType::SetOfString, "containsAny").unwrap();
        let out = op
            .emit(
                "context.custom.totalInputUsd.sources",
                &PredicateValue::Multi(vec!["chainlink".into(), "pyth".into()]),
            )
            .unwrap();
        assert_eq!(
            out,
            r#"context.custom.totalInputUsd.sources.containsAny(["chainlink", "pyth"])"#
        );
    }

    #[test]
    fn arity_mismatch_is_reported() {
        let op = find(CedarType::Long, "gt").unwrap();
        let err = op
            .emit("context.x", &PredicateValue::Multi(vec!["1".into()]))
            .unwrap_err();
        assert!(matches!(err, EmitError::ArityMismatch { .. }));
    }

    #[test]
    fn bad_long_operand_is_reported() {
        let op = find(CedarType::Long, "gt").unwrap();
        let err = op
            .emit("context.x", &PredicateValue::Single("abc".into()))
            .unwrap_err();
        assert!(matches!(err, EmitError::BadOperand { .. }));
    }

    #[test]
    fn long_gt_accepts_fractional_zero_via_normalization() {
        // Mirror of the decimal fix: `100.0` should compile the same
        // as `100`. Without normalization the user gets "invalid Long
        // literal: 100.0" — a confusing error for a value that's
        // semantically an integer.
        let op = find(CedarType::Long, "gt").unwrap();
        let out = op
            .emit("context.feeBps", &PredicateValue::Single("100.0".into()))
            .unwrap();
        assert_eq!(out, "context.feeBps > 100");

        // Non-zero fraction must still fail — silent rounding would
        // be worse than a clear error.
        let err = op
            .emit("context.feeBps", &PredicateValue::Single("100.5".into()))
            .unwrap_err();
        assert!(matches!(err, EmitError::BadOperand { .. }));
    }

    #[test]
    fn set_long_contains_normalizes_each_element() {
        // Every operand in the set goes through the same normalizer,
        // so `"1, 2.0, 3"` from a UI that adds trailing `.0` on
        // integers compiles cleanly.
        let op = find(CedarType::SetOfLong, "containsAny").unwrap();
        let out = op
            .emit(
                "context.allowedFeeBps",
                &PredicateValue::Multi(vec!["1".into(), "2.0".into(), "3".into()]),
            )
            .unwrap();
        assert_eq!(out, "context.allowedFeeBps.containsAny([1, 2, 3])");
    }
}
