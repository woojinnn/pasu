//! Rule validation against an action schema.
//!
//! Runs before [`crate::generator::compile`] to surface user errors with
//! field-level paths intact, rather than letting them turn into low-level
//! emit failures further down.

use crate::operators;
use crate::types::{ActionSchema, PolicyRule, PredicateValue};
use regex::Regex;
use thiserror::Error;

/// Validation failure modes. Each variant identifies which predicate index
/// and which field/op was at fault so a UI can highlight the offending row.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    /// Rule referenced an action no registered schema describes.
    #[error("unknown action: {0}")]
    UnknownAction(String),
    /// Rule's `id` is empty — required for policy identification.
    #[error("rule id must not be empty")]
    EmptyId,
    /// Predicate referenced a field path absent from the schema.
    #[error("predicate {index}: unknown field path: {field}")]
    UnknownField {
        /// Position in `rule.predicates`.
        index: usize,
        /// The bad path.
        field: String,
    },
    /// Operator id isn't defined for the field's Cedar type.
    #[error("predicate {index}: operator {op} not valid for field {field}")]
    UnknownOperator {
        /// Position in `rule.predicates`.
        index: usize,
        /// The bad operator id.
        op: String,
        /// Field path the operator was applied to.
        field: String,
    },
    /// Value shape didn't match the operator's arity (e.g. `Single` for `containsAny`).
    #[error("predicate {index}: operator {op} expects {expected}, got {got}")]
    ArityMismatch {
        /// Position in `rule.predicates`.
        index: usize,
        /// Operator id.
        op: String,
        /// Expected arity description.
        expected: &'static str,
        /// Actual arity description.
        got: &'static str,
    },
    /// `Multi` value list was empty for an operator that needs ≥1 operand.
    #[error("predicate {index}: operator {op} requires a non-empty list of operands")]
    EmptyOperandList {
        /// Position in `rule.predicates`.
        index: usize,
        /// Operator id.
        op: String,
    },
    /// Operand wasn't in the field's `allowed_values` enum set. Surfaces UI
    /// dropdown bypass attempts (CodeView edits, programmatic input) before
    /// they reach the Cedar generator and produce a syntactically valid but
    /// semantically dead policy that silently never matches.
    #[error(
        "predicate {index}: value {value:?} not in allowed set for field {field} (allowed: {allowed:?})"
    )]
    DisallowedValue {
        /// Position in `rule.predicates`.
        index: usize,
        /// Field path the operand was applied to.
        field: String,
        /// The offending value.
        value: String,
        /// The full closed set this value should have been drawn from.
        allowed: Vec<String>,
    },
    /// Operand didn't match the field's `pattern` regex (e.g. typo'd EVM
    /// address like `"0x52"` against `^0x[0-9a-fA-F]{40}$`). Without this
    /// check, the policy would compile to syntactically valid Cedar but
    /// the bad literal would never match the decoder's well-formed
    /// `context.recipient`, producing a silent no-op.
    #[error(
        "predicate {index}: value {value:?} doesn't match pattern {pattern:?} for field {field}"
    )]
    PatternMismatch {
        /// Position in `rule.predicates`.
        index: usize,
        /// Field path the operand was applied to.
        field: String,
        /// The offending value.
        value: String,
        /// The regex the value should have matched.
        pattern: String,
    },
    /// A field's declared `pattern` regex didn't compile. This is a schema
    /// authoring bug, not user input — surface it so the UI/tests can
    /// pinpoint the bad schema entry instead of silently skipping the
    /// pattern check.
    #[error("predicate {index}: field {field} declares an invalid regex {pattern:?}: {message}")]
    InvalidPattern {
        /// Position in `rule.predicates`.
        index: usize,
        /// Field path the bad regex was attached to.
        field: String,
        /// The regex source string.
        pattern: String,
        /// Underlying regex compile error message.
        message: String,
    },
}

/// Verify `rule` is internally consistent with `schema`.
///
/// Does not emit Cedar text — that's [`crate::generator::compile`]'s job.
///
/// # Errors
///
/// Returns the first [`ValidationError`] encountered. Validation is single-pass
/// and short-circuits.
pub fn validate(rule: &PolicyRule, schema: &ActionSchema) -> Result<(), ValidationError> {
    if rule.id.trim().is_empty() {
        return Err(ValidationError::EmptyId);
    }
    if rule.action != schema.action {
        return Err(ValidationError::UnknownAction(rule.action.clone()));
    }

    for (index, predicate) in rule.predicates.iter().enumerate() {
        let field_spec =
            schema
                .fields
                .get(&predicate.field)
                .ok_or_else(|| ValidationError::UnknownField {
                    index,
                    field: predicate.field.clone(),
                })?;

        let op = operators::find(field_spec.cedar_type, &predicate.op).ok_or_else(|| {
            ValidationError::UnknownOperator {
                index,
                op: predicate.op.clone(),
                field: predicate.field.clone(),
            }
        })?;

        // Cheap arity check up-front so we don't fall through to escape-level
        // errors with vague messages.
        let got = value_shape(&predicate.value);
        let arity_ok = matches!(
            (op.arity, &predicate.value),
            (operators::OperatorArity::One, PredicateValue::Single(_))
                | (operators::OperatorArity::Many, PredicateValue::Multi(_))
                | (operators::OperatorArity::None, PredicateValue::None)
        );
        if !arity_ok {
            return Err(ValidationError::ArityMismatch {
                index,
                op: predicate.op.clone(),
                expected: arity_label(op.arity),
                got,
            });
        }

        if let PredicateValue::Multi(values) = &predicate.value {
            if values.is_empty() {
                return Err(ValidationError::EmptyOperandList {
                    index,
                    op: predicate.op.clone(),
                });
            }
        }

        // Enum constraint: when the field carries an `allowed_values` set,
        // every operand literal must come from it. Applies to Single and
        // Multi; None arity has no operand to check.
        if let Some(allowed) = field_spec.allowed_values.as_ref() {
            match &predicate.value {
                PredicateValue::Single(v) => {
                    if !allowed.iter().any(|a| a == v) {
                        return Err(ValidationError::DisallowedValue {
                            index,
                            field: predicate.field.clone(),
                            value: v.clone(),
                            allowed: allowed.clone(),
                        });
                    }
                }
                PredicateValue::Multi(vs) => {
                    for v in vs {
                        if !allowed.iter().any(|a| a == v) {
                            return Err(ValidationError::DisallowedValue {
                                index,
                                field: predicate.field.clone(),
                                value: v.clone(),
                                allowed: allowed.clone(),
                            });
                        }
                    }
                }
                PredicateValue::None => {}
            }
        }

        // Pattern constraint: regex from the upstream JSON Schema's
        // `"pattern"` (e.g. EVM address shape). Compile lazily so fields
        // without a pattern pay nothing. A regex that fails to compile is
        // a schema bug — surface it instead of silently skipping the
        // check.
        if let Some(pat) = field_spec.pattern.as_ref() {
            let re = match Regex::new(pat) {
                Ok(r) => r,
                Err(err) => {
                    return Err(ValidationError::InvalidPattern {
                        index,
                        field: predicate.field.clone(),
                        pattern: pat.clone(),
                        message: err.to_string(),
                    });
                }
            };
            match &predicate.value {
                PredicateValue::Single(v) => {
                    if !re.is_match(v) {
                        return Err(ValidationError::PatternMismatch {
                            index,
                            field: predicate.field.clone(),
                            value: v.clone(),
                            pattern: pat.clone(),
                        });
                    }
                }
                PredicateValue::Multi(vs) => {
                    for v in vs {
                        if !re.is_match(v) {
                            return Err(ValidationError::PatternMismatch {
                                index,
                                field: predicate.field.clone(),
                                value: v.clone(),
                                pattern: pat.clone(),
                            });
                        }
                    }
                }
                PredicateValue::None => {}
            }
        }
    }

    Ok(())
}

const fn value_shape(value: &PredicateValue) -> &'static str {
    match value {
        PredicateValue::Single(_) => "single",
        PredicateValue::Multi(_) => "multi",
        PredicateValue::None => "none",
    }
}

const fn arity_label(arity: operators::OperatorArity) -> &'static str {
    match arity {
        operators::OperatorArity::One => "single",
        operators::OperatorArity::Many => "multi",
        operators::OperatorArity::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::swap;
    use crate::types::{Predicate, Severity};

    fn base_rule(predicates: Vec<Predicate>) -> PolicyRule {
        PolicyRule {
            id: "test/rule".into(),
            action: "swap".into(),
            severity: Severity::Deny,
            reason: "test".into(),
            predicates,
        }
    }

    #[test]
    fn valid_long_predicate_passes() {
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "feeBps".into(),
            op: "gt".into(),
            value: PredicateValue::Single("100".into()),
        }]);
        assert!(validate(&rule, &schema).is_ok());
    }

    #[test]
    fn empty_id_rejected() {
        let schema = swap::schema();
        let mut rule = base_rule(vec![]);
        rule.id = String::new();
        assert_eq!(validate(&rule, &schema), Err(ValidationError::EmptyId));
    }

    #[test]
    fn action_mismatch_rejected() {
        let schema = swap::schema();
        let mut rule = base_rule(vec![]);
        rule.action = "approve".into();
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::UnknownAction(_))
        ));
    }

    #[test]
    fn unknown_field_rejected() {
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "nonexistent".into(),
            op: "gt".into(),
            value: PredicateValue::Single("1".into()),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::UnknownField { .. })
        ));
    }

    #[test]
    fn wrong_operator_for_type_rejected() {
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "swapMode".into(), // String — `gt` not valid
            op: "gt".into(),
            value: PredicateValue::Single("x".into()),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::UnknownOperator { .. })
        ));
    }

    #[test]
    fn arity_mismatch_rejected() {
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "feeBps".into(),
            op: "gt".into(),
            value: PredicateValue::Multi(vec!["1".into()]),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::ArityMismatch { .. })
        ));
    }

    #[test]
    fn empty_multi_operand_rejected() {
        // Uses a legacy custom field (`totalInputUsd.sources` is SetOfString)
        // because empty-operand-list errors are most visible on multi-arity
        // operators, and SetOfString is the canonical such type. The base
        // schema doesn't carry a SetOfString field — the legacy-custom
        // fixture restores `totalInputUsd.*` for this scenario.
        let schema = swap::schema_with_legacy_custom();
        let rule = base_rule(vec![Predicate {
            field: "totalInputUsd.sources".into(),
            op: "containsAny".into(),
            value: PredicateValue::Multi(vec![]),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::EmptyOperandList { .. })
        ));
    }

    #[test]
    fn enum_value_in_set_passes() {
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "swapMode".into(),
            op: "eq".into(),
            value: PredicateValue::Single("exact_in".into()),
        }]);
        assert!(validate(&rule, &schema).is_ok());
    }

    #[test]
    fn enum_value_outside_set_rejected() {
        // The exact common-case footgun the enum check exists to prevent:
        // hyphen typo in a swap mode literal — Cedar would happily compile
        // `context.swapMode == "exact-in"`, then the policy would silently
        // never match at runtime against the snake_case decoder output.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "swapMode".into(),
            op: "eq".into(),
            value: PredicateValue::Single("exact-in".into()),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::DisallowedValue { .. })
        ));
    }

    #[test]
    fn enum_multi_value_partial_violation_rejected() {
        // `in` operator: every element of the multi-list must be in the set.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "validity.source".into(),
            op: "in".into(),
            value: PredicateValue::Multi(vec![
                "tx-deadline".into(),
                "tx_deadline".into(), // underscore typo — should reject
            ]),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::DisallowedValue { .. })
        ));
    }

    #[test]
    fn pattern_rejects_short_address() {
        // The exact footgun the pattern check exists for: a 6-character
        // hex blob looks like an address but isn't. Without the regex
        // guard, this would compile to a syntactically valid Cedar policy
        // that never matches the well-formed `context.recipient` produced
        // by the decoder.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "recipient".into(),
            op: "eq".into(),
            value: PredicateValue::Single("0x52".into()),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::PatternMismatch { .. })
        ));
    }

    #[test]
    fn pattern_accepts_valid_address() {
        // A real 40-char EVM address (USDC mainnet, lowercased) must pass.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "recipient".into(),
            op: "eq".into(),
            value: PredicateValue::Single(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".into(),
            ),
        }]);
        assert!(validate(&rule, &schema).is_ok());
    }

    #[test]
    fn pattern_accepts_mixed_case_address() {
        // EIP-55 checksummed addresses use mixed case — regex permits
        // [0-9a-fA-F]{40}, so checksummed forms pass.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "inputToken.asset.address".into(),
            op: "eq".into(),
            value: PredicateValue::Single(
                "0xA0b86991c6218b36c1D19D4a2e9Eb0cE3606eB48".into(),
            ),
        }]);
        assert!(validate(&rule, &schema).is_ok());
    }

    #[test]
    fn pattern_rejects_partial_match() {
        // `is_match` allows the pattern to be anywhere in the string by
        // default, but our patterns are anchored with ^ and $. This test
        // guards that anchoring stays intact — a 40-hex prefix with junk
        // appended must be rejected.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "recipient".into(),
            op: "eq".into(),
            value: PredicateValue::Single(
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48extra".into(),
            ),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::PatternMismatch { .. })
        ));
    }

    #[test]
    fn pattern_check_applies_per_element_in_multi() {
        // `in [..]` on an address field: every element must match the
        // pattern, not just one.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "recipient".into(),
            op: "in".into(),
            value: PredicateValue::Multi(vec![
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".into(),
                "0xnope".into(), // ← second element invalid
            ]),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::PatternMismatch { .. })
        ));
    }

    #[test]
    fn pattern_rejects_non_digit_decimal_string() {
        // DecimalString fields (validity.expiresAt etc.) reject anything
        // other than digits.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "validity.expiresAt".into(),
            op: "eq".into(),
            value: PredicateValue::Single("2026-05-19T00:00:00Z".into()),
        }]);
        assert!(matches!(
            validate(&rule, &schema),
            Err(ValidationError::PatternMismatch { .. })
        ));
    }

    #[test]
    fn free_form_value_unchanged_by_enum_check() {
        // Non-enum, non-pattern fields stay free-form. `feeBps` has no
        // pattern (it's a Long), so any string that parses as i64 passes
        // through the validator — operator-level escape catches non-int.
        let schema = swap::schema();
        let rule = base_rule(vec![Predicate {
            field: "feeBps".into(),
            op: "eq".into(),
            value: PredicateValue::Single("30".into()),
        }]);
        assert!(validate(&rule, &schema).is_ok());
    }
}
