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

use crate::escape::{escape_decimal, escape_long, escape_string, EscapeError};
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
        let rendered = escape_long(raw).map_err(|e| bad_operand(op, e))?;
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
        let rendered = escape_decimal(raw).map_err(|e| bad_operand(op, e))?;
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
fn emit_set_long_contains(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let raw = one_operand("contains", v)?;
    let rendered = escape_long(raw).map_err(|e| bad_operand("contains", e))?;
    Ok(format!("{left}.contains({rendered})"))
}

fn emit_set_long_contains_any(left: &str, v: &PredicateValue) -> Result<String, EmitError> {
    let xs = many_operands("containsAny", v)?;
    let mut rendered_parts = Vec::with_capacity(xs.len());
    for x in xs {
        rendered_parts.push(escape_long(x).map_err(|e| bad_operand("containsAny", e))?);
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
        rendered_parts.push(escape_long(x).map_err(|e| bad_operand("containsAll", e))?);
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
}
