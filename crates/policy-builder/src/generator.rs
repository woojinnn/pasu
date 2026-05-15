//! `PolicyRule` -> Cedar text compiler.
//!
//! The emitted shape mirrors the convention enforced by
//! `policy-engine`'s installer: a single `forbid` with `@id`, `@severity`,
//! `@reason` annotations and an AND-of-predicates `when` clause.
//!
//! Compilation order:
//! 1. Validate the rule against its action schema.
//! 2. Walk predicates: emit Cedar fragments and collect the set of parent
//!    record names that need a `context has X` guard.
//! 3. Compose: annotations, head, then the `when` clause with all `has`
//!    guards inserted first (so type-narrowing happens before the comparisons
//!    that depend on it).

use crate::escape::escape_string;
use crate::operators::{self, EmitError};
use crate::types::{ActionSchema, PolicyRule};
use crate::validate::{validate, ValidationError};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Failure modes for [`compile`].
#[derive(Debug, Error)]
pub enum CompileError {
    /// Rule failed pre-emit validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// Operator emission failed (escape error or arity mismatch slipping past validation).
    #[error("predicate {index}: {source}")]
    Emit {
        /// Predicate position.
        index: usize,
        /// Underlying emit error.
        #[source]
        source: EmitError,
    },
}

/// Compile `rule` against `schema` into a Cedar policy string.
///
/// The result is a single `forbid` policy ready to be passed to
/// `install_policies_json` as one entry of the policy set.
///
/// # Errors
///
/// Returns [`CompileError::Validation`] when the rule references unknown
/// fields/operators or has shape mismatches, and [`CompileError::Emit`] when
/// an operand fails Cedar-literal validation downstream.
///
/// # Panics
///
/// Never in practice: post-`validate`, every predicate's field and operator
/// lookup is guaranteed to succeed.
pub fn compile(rule: &PolicyRule, schema: &ActionSchema) -> Result<String, CompileError> {
    validate(rule, schema)?;

    // BTreeSet sorts lexicographically; `context has X` precedes
    // `context.X has Y` because space (0x20) sorts before dot (0x2E), so
    // parent-record guards naturally land before their nested attribute
    // guards in the emitted `when` clause.
    let mut needed_has_guards: BTreeSet<String> = BTreeSet::new();
    let mut fragments: Vec<String> = Vec::with_capacity(rule.predicates.len());

    for (index, predicate) in rule.predicates.iter().enumerate() {
        let field_spec = schema
            .fields
            .get(&predicate.field)
            .expect("validated above");

        collect_has_guards(field_spec, &mut needed_has_guards);

        let op = operators::find(field_spec.cedar_type, &predicate.op).expect("validated above");
        let left = format!("context.{}", predicate.field);
        let fragment = op
            .emit(&left, &predicate.value)
            .map_err(|source| CompileError::Emit { index, source })?;
        fragments.push(fragment);
    }

    Ok(assemble(rule, schema, &needed_has_guards, &fragments))
}

/// Compile a rule using the bundled schema registry (looked up by `rule.action`).
///
/// # Errors
///
/// Returns [`ValidationError::UnknownAction`] wrapped in [`CompileError`] if
/// no schema matches `rule.action`; otherwise behaves like [`compile`].
pub fn compile_with_registry(
    rule: &PolicyRule,
    registry: &BTreeMap<String, ActionSchema>,
) -> Result<String, CompileError> {
    let schema = registry
        .get(&rule.action)
        .ok_or_else(|| ValidationError::UnknownAction(rule.action.clone()))?;
    compile(rule, schema)
}

fn collect_has_guards(field_spec: &crate::types::FieldSpec, out: &mut BTreeSet<String>) {
    // Cedar strict validation requires a `has` guard before any access to an
    // optional attribute. Three cases:
    //   1. top-level optional leaf      → `context has <leaf>`
    //   2. optional parent record       → `context has <parent>`
    //   3. optional leaf within parent  → `context.<parent> has <leaf_name>`
    // Cases 2 and 3 can both fire for the same field (optional leaf inside
    // an optional parent); both guards are needed and `BTreeSet` dedupes
    // shared parents across multiple predicates.
    match field_spec.parent_path.as_deref() {
        Some(parent) => {
            if field_spec.parent_optional {
                out.insert(format!("context has {parent}"));
            }
            if field_spec.optional {
                let leaf_name = field_spec
                    .path
                    .rsplit('.')
                    .next()
                    .expect("dotted path has at least one segment");
                out.insert(format!("context.{parent} has {leaf_name}"));
            }
        }
        None => {
            if field_spec.optional {
                out.insert(format!("context has {}", field_spec.path));
            }
        }
    }
}

fn assemble(
    rule: &PolicyRule,
    schema: &ActionSchema,
    has_guards: &BTreeSet<String>,
    fragments: &[String],
) -> String {
    let head = format!(
        "@id({})\n@severity({})\n@reason({})\nforbid (\n  principal is {principal},\n  action == Action::{action},\n  resource is {resource}\n)",
        escape_string(&rule.id),
        escape_string(rule.severity.as_str()),
        escape_string(&rule.reason),
        principal = schema.principal_type,
        action = escape_string(&schema.action),
        resource = schema.resource_type,
    );

    if has_guards.is_empty() && fragments.is_empty() {
        return format!("{head};\n");
    }

    let mut lines: Vec<String> = Vec::with_capacity(has_guards.len() + fragments.len());
    lines.extend(has_guards.iter().cloned());
    lines.extend(fragments.iter().cloned());

    let body = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("  {line}")
            } else {
                format!("  && {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("{head}\nwhen {{\n{body}\n}};\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::schemas::swap;
    use crate::types::{Predicate, PredicateValue, Severity};

    fn rule_with(predicates: Vec<Predicate>) -> PolicyRule {
        PolicyRule {
            id: "user/test".into(),
            action: "swap".into(),
            severity: Severity::Deny,
            reason: "test reason".into(),
            predicates,
        }
    }

    #[test]
    fn unconditional_forbid_emits_no_when() {
        let out = compile(&rule_with(vec![]), &swap::schema()).unwrap();
        assert!(out.contains("forbid"));
        assert!(!out.contains("when"));
        assert!(out.ends_with(";\n"));
    }

    #[test]
    fn simple_long_gt_emits_inline_comparison() {
        let rule = rule_with(vec![Predicate {
            field: "feeBps".into(),
            op: "gt".into(),
            value: PredicateValue::Single("100".into()),
        }]);
        let out = compile(&rule, &swap::schema()).unwrap();
        assert!(out.contains("context.feeBps > 100"), "got:\n{out}");
        assert!(out.contains(r#"@id("user/test")"#));
        assert!(out.contains(r#"@severity("deny")"#));
        assert!(out.contains(r#"@reason("test reason")"#));
        assert!(out.contains(r#"action == Action::"swap""#));
    }

    #[test]
    fn optional_parent_emits_single_has_guard() {
        let rule = rule_with(vec![
            Predicate {
                field: "totalInputUsd.value".into(),
                op: "gt".into(),
                value: PredicateValue::Single("100.00".into()),
            },
            Predicate {
                field: "totalInputUsd.staleSec".into(),
                op: "lte".into(),
                value: PredicateValue::Single("60".into()),
            },
        ]);
        let out = compile(&rule, &swap::schema()).unwrap();
        assert_eq!(
            out.matches("context has totalInputUsd").count(),
            1,
            "guard should appear once, got:\n{out}"
        );
        assert!(out.contains(r#"context.totalInputUsd.value.greaterThan(decimal("100.00"))"#));
        assert!(out.contains("context.totalInputUsd.staleSec <= 60"));
    }

    #[test]
    fn multiple_optional_parents_each_guarded() {
        let rule = rule_with(vec![
            Predicate {
                field: "totalInputUsd.value".into(),
                op: "gt".into(),
                value: PredicateValue::Single("100.00".into()),
            },
            Predicate {
                field: "totalMinOutputUsd.value".into(),
                op: "gt".into(),
                value: PredicateValue::Single("50.00".into()),
            },
        ]);
        let out = compile(&rule, &swap::schema()).unwrap();
        assert!(out.contains("context has totalInputUsd"));
        assert!(out.contains("context has totalMinOutputUsd"));
    }

    #[test]
    fn set_of_string_contains_any() {
        let rule = rule_with(vec![Predicate {
            field: "totalInputUsd.sources".into(),
            op: "containsAny".into(),
            value: PredicateValue::Multi(vec!["chainlink".into(), "pyth".into()]),
        }]);
        let out = compile(&rule, &swap::schema()).unwrap();
        assert!(out.contains(r#"context.totalInputUsd.sources.containsAny(["chainlink", "pyth"])"#));
    }

    #[test]
    fn warn_severity_propagates() {
        let mut rule = rule_with(vec![]);
        rule.severity = Severity::Warn;
        let out = compile(&rule, &swap::schema()).unwrap();
        assert!(out.contains(r#"@severity("warn")"#));
    }

    #[test]
    fn validation_error_short_circuits() {
        let rule = rule_with(vec![Predicate {
            field: "nonexistent".into(),
            op: "gt".into(),
            value: PredicateValue::Single("1".into()),
        }]);
        assert!(matches!(
            compile(&rule, &swap::schema()),
            Err(CompileError::Validation(
                ValidationError::UnknownField { .. }
            ))
        ));
    }

    #[test]
    fn id_with_quote_is_escaped() {
        let mut rule = rule_with(vec![]);
        rule.id = r#"user/quote"id"#.into();
        let out = compile(&rule, &swap::schema()).unwrap();
        assert!(out.contains(r#"@id("user/quote\"id")"#), "got:\n{out}");
    }
}
