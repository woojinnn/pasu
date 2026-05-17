//! Cedar text → [`PolicyRule`] reverse parser (Phase 2, narrow subset).
//!
//! Recognizes the exact shapes produced by [`crate::generator::compile`]:
//!
//! ```text
//! @id("…")
//! @severity("deny"|"warn")
//! @reason("…")
//! forbid (
//!   principal is <Type>,
//!   action == Action::"<name>",
//!   resource is <Type>
//! )
//! when {
//!   <has-guards>
//!   && context.<path> <op> <literal>
//!   …
//! };
//! ```
//!
//! Anything outside that grammar — `permit`, multi-`forbid` files, OR/NOT,
//! hand-rolled clauses with arbitrary boolean expressions — is rejected with
//! [`ParseError::UnsupportedShape`]. Callers handle the error by keeping the
//! user in Code mode (see `web/dashboard` plan §5).

use crate::escape::{self, EscapeError};
use crate::types::{PolicyRule, Predicate, PredicateValue, Severity};
use regex::Regex;
use std::sync::OnceLock;
use thiserror::Error;

/// Failure modes for [`parse_cedar`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Required annotation is missing (`@id`, `@severity`, `@reason`).
    #[error("missing annotation @{0}")]
    MissingAnnotation(&'static str),
    /// `@severity(...)` is not `"deny"` or `"warn"`.
    #[error("invalid severity: {0}")]
    InvalidSeverity(String),
    /// `forbid (...)` head missing or malformed.
    #[error("missing or malformed forbid head")]
    MalformedHead,
    /// `action == Action::\"<name>\"` not found.
    #[error("missing action binding")]
    MissingAction,
    /// `when { ... }` block missing closing brace or empty.
    #[error("malformed when clause")]
    MalformedWhen,
    /// A predicate fragment didn't match any supported emit shape.
    #[error("unsupported predicate fragment: {0}")]
    UnsupportedShape(String),
    /// String escape decode failure on an emitted literal.
    #[error(transparent)]
    Escape(#[from] EscapeError),
}

/// Parse a single Cedar `forbid` policy text into a [`PolicyRule`].
///
/// # Errors
///
/// Returns [`ParseError`] when the input deviates from the
/// generator-produced subset described in this module.
#[allow(clippy::missing_panics_doc)]
pub fn parse_cedar(text: &str) -> Result<PolicyRule, ParseError> {
    let cleaned = strip_comments(text);

    let id = capture_annotation(&cleaned, "id")?.unwrap_or_default();
    if id.is_empty() {
        return Err(ParseError::MissingAnnotation("id"));
    }
    let severity_raw = capture_annotation(&cleaned, "severity")?
        .ok_or(ParseError::MissingAnnotation("severity"))?;
    let severity = match severity_raw.as_str() {
        "deny" => Severity::Deny,
        "warn" => Severity::Warn,
        other => return Err(ParseError::InvalidSeverity(other.to_string())),
    };
    let reason = capture_annotation(&cleaned, "reason")?
        .ok_or(ParseError::MissingAnnotation("reason"))?;

    // `forbid (…)` head — verify presence first so `permit` and free-form
    // policies are rejected with the more specific MalformedHead error
    // rather than a downstream MissingAction.
    if !head_regex().is_match(&cleaned) {
        return Err(ParseError::MalformedHead);
    }

    let action = capture_action(&cleaned).ok_or(ParseError::MissingAction)?;

    let predicates = match capture_when_body(&cleaned) {
        Some(body) => parse_predicates(&body)?,
        None => Vec::new(),
    };

    Ok(PolicyRule {
        id,
        action,
        severity,
        reason,
        predicates,
    })
}

// ── tokenization helpers ───────────────────────────────────────────────────

fn strip_comments(text: &str) -> String {
    // Block first (may contain `//`), then line.
    let block = block_comment_regex().replace_all(text, "");
    line_comment_regex().replace_all(&block, "").into_owned()
}

fn capture_annotation(text: &str, name: &str) -> Result<Option<String>, EscapeError> {
    let re = annotation_regex();
    for caps in re.captures_iter(text) {
        if &caps[1] == name {
            let raw = &caps[2];
            return Ok(Some(escape::unescape_string(raw)?));
        }
    }
    Ok(None)
}

fn capture_action(text: &str) -> Option<String> {
    let caps = action_regex().captures(text)?;
    escape::unescape_string(&caps[1]).ok()
}

fn capture_when_body(text: &str) -> Option<String> {
    // Find `when {` then walk forward to the matching `}`. Hand-rolled
    // depth tracking is needed because nested array literals contain `{`-
    // free content but the regex would still over-match on the first `}`.
    let start_marker = text.find("when")?;
    let after_when = &text[start_marker + 4..];
    let open_idx = after_when.find('{')?;
    let body_start = start_marker + 4 + open_idx + 1;
    let chars = text[body_start..].char_indices();
    let mut depth: i32 = 1;
    for (i, ch) in chars {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[body_start..body_start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

// ── predicate fragment parsing ─────────────────────────────────────────────

fn parse_predicates(body: &str) -> Result<Vec<Predicate>, ParseError> {
    let fragments = split_on_top_level_amp_amp(body);
    let mut out = Vec::new();
    for frag in fragments {
        let trimmed = frag.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Skip `has` guards — they're auto-emitted by the generator and
        // recovered structurally from the field schema, not from the text.
        if is_has_guard(trimmed) {
            continue;
        }
        out.push(parse_predicate(trimmed)?);
    }
    Ok(out)
}

fn is_has_guard(frag: &str) -> bool {
    // Matches `context has <ident>` and `context.<path> has <ident>`.
    has_guard_regex().is_match(frag)
}

/// Split on top-level `&&` while respecting `(...)` and `[...]` nesting so
/// `containsAny(["a", "b"])` and `decimal("…")` literals stay intact.
fn split_on_top_level_amp_amp(body: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut paren: i32 = 0;
    let mut bracket: i32 = 0;
    let mut in_string = false;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == '\\' {
                if let Some(&n) = chars.peek() {
                    current.push(n);
                    chars.next();
                }
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                paren += 1;
                current.push(ch);
            }
            ')' => {
                paren -= 1;
                current.push(ch);
            }
            '[' => {
                bracket += 1;
                current.push(ch);
            }
            ']' => {
                bracket -= 1;
                current.push(ch);
            }
            '&' if paren == 0 && bracket == 0 => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    parts.push(std::mem::take(&mut current));
                } else {
                    current.push(ch);
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

fn parse_predicate(frag: &str) -> Result<Predicate, ParseError> {
    // Try patterns in order — bool, in-array, method calls, infix cmps.
    if let Some(p) = try_bool_isfalse(frag)? {
        return Ok(p);
    }
    if let Some(p) = try_string_in(frag)? {
        return Ok(p);
    }
    if let Some(p) = try_method_call(frag)? {
        return Ok(p);
    }
    if let Some(p) = try_infix_cmp(frag)? {
        return Ok(p);
    }
    if let Some(p) = try_bool_istrue(frag) {
        return Ok(p);
    }
    Err(ParseError::UnsupportedShape(frag.to_string()))
}

fn try_bool_isfalse(frag: &str) -> Result<Option<Predicate>, ParseError> {
    if let Some(caps) = bool_isfalse_regex().captures(frag) {
        return Ok(Some(Predicate {
            field: caps[1].to_string(),
            op: "isFalse".into(),
            value: PredicateValue::None,
        }));
    }
    Ok(None)
}

fn try_bool_istrue(frag: &str) -> Option<Predicate> {
    bool_istrue_regex().captures(frag).map(|caps| Predicate {
        field: caps[1].to_string(),
        op: "isTrue".into(),
        value: PredicateValue::None,
    })
}

fn try_string_in(frag: &str) -> Result<Option<Predicate>, ParseError> {
    // Emit shape: `[ "a", "b" ].contains(context.field)`
    if let Some(caps) = string_in_regex().captures(frag) {
        let list_body = &caps[1];
        let field = caps[2].to_string();
        let values = parse_string_list(list_body)?;
        return Ok(Some(Predicate {
            field,
            op: "in".into(),
            value: PredicateValue::Multi(values),
        }));
    }
    Ok(None)
}

fn try_method_call(frag: &str) -> Result<Option<Predicate>, ParseError> {
    let Some(caps) = method_call_regex().captures(frag) else {
        return Ok(None);
    };
    let field = caps[1].to_string();
    let method = &caps[2];
    let args = caps[3].trim();

    let (op_id, value) = match method {
        // Decimal comparisons → `decimal("…")` single arg
        "greaterThan" => ("gt", parse_decimal_arg(args)?),
        "greaterThanOrEqual" => ("gte", parse_decimal_arg(args)?),
        "lessThan" => ("lt", parse_decimal_arg(args)?),
        "lessThanOrEqual" => ("lte", parse_decimal_arg(args)?),
        // Set operations
        "contains" => ("contains", parse_contains_arg(args)?),
        "containsAny" => ("containsAny", parse_list_arg(args)?),
        "containsAll" => ("containsAll", parse_list_arg(args)?),
        _ => {
            return Err(ParseError::UnsupportedShape(frag.to_string()));
        }
    };

    Ok(Some(Predicate {
        field,
        op: op_id.into(),
        value,
    }))
}

fn try_infix_cmp(frag: &str) -> Result<Option<Predicate>, ParseError> {
    let Some(caps) = infix_cmp_regex().captures(frag) else {
        return Ok(None);
    };
    let field = caps[1].to_string();
    let symbol = &caps[2];
    let rhs = caps[3].trim();

    let op_id = match symbol {
        ">" => "gt",
        ">=" => "gte",
        "<" => "lt",
        "<=" => "lte",
        "==" => "eq",
        "!=" => "ne",
        _ => return Err(ParseError::UnsupportedShape(frag.to_string())),
    };

    let value = if let Some(s) = strip_double_quotes(rhs) {
        PredicateValue::Single(escape::unescape_string(s)?)
    } else {
        // Long literal (numeric).
        PredicateValue::Single(rhs.to_string())
    };

    Ok(Some(Predicate {
        field,
        op: op_id.into(),
        value,
    }))
}

fn parse_decimal_arg(args: &str) -> Result<PredicateValue, ParseError> {
    // Shape: `decimal("100.00")`
    let caps = decimal_arg_regex()
        .captures(args)
        .ok_or_else(|| ParseError::UnsupportedShape(args.to_string()))?;
    let raw = &caps[1];
    Ok(PredicateValue::Single(escape::unescape_string(raw)?))
}

fn parse_contains_arg(args: &str) -> Result<PredicateValue, ParseError> {
    let trimmed = args.trim();
    if let Some(inner) = strip_double_quotes(trimmed) {
        return Ok(PredicateValue::Single(escape::unescape_string(inner)?));
    }
    // Numeric (SetOfLong)
    Ok(PredicateValue::Single(trimmed.to_string()))
}

fn parse_list_arg(args: &str) -> Result<PredicateValue, ParseError> {
    // Shape: `[ "a", "b", 1, 2 ]`
    let stripped = args
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| ParseError::UnsupportedShape(args.to_string()))?;
    let items = split_top_level_commas(stripped);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let t = item.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(inner) = strip_double_quotes(t) {
            out.push(escape::unescape_string(inner)?);
        } else {
            out.push(t.to_string());
        }
    }
    Ok(PredicateValue::Multi(out))
}

fn parse_string_list(list_body: &str) -> Result<Vec<String>, ParseError> {
    let items = split_top_level_commas(list_body);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let t = item.trim();
        let inner = strip_double_quotes(t)
            .ok_or_else(|| ParseError::UnsupportedShape(list_body.to_string()))?;
        out.push(escape::unescape_string(inner)?);
    }
    Ok(out)
}

fn split_top_level_commas(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut paren: i32 = 0;
    let mut bracket: i32 = 0;
    let mut in_string = false;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == '\\' {
                if let Some(&n) = chars.peek() {
                    current.push(n);
                    chars.next();
                }
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                paren += 1;
                current.push(ch);
            }
            ')' => {
                paren -= 1;
                current.push(ch);
            }
            '[' => {
                bracket += 1;
                current.push(ch);
            }
            ']' => {
                bracket -= 1;
                current.push(ch);
            }
            ',' if paren == 0 && bracket == 0 => {
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn strip_double_quotes(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    let stripped = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    Some(stripped)
}

// ── regex caches (compiled once) ───────────────────────────────────────────

macro_rules! cached_regex {
    ($name:ident, $pattern:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pattern).expect("static regex"))
        }
    };
}

cached_regex!(line_comment_regex, r"//[^\n]*");
cached_regex!(block_comment_regex, r"(?s)/\*.*?\*/");
cached_regex!(annotation_regex, r#"@(id|severity|reason)\("((?:\\.|[^"\\])*)"\)"#);
cached_regex!(action_regex, r#"action\s*==\s*Action::"((?:\\.|[^"\\])*)""#);
cached_regex!(head_regex, r"(?s)forbid\s*\([^)]*\)");
cached_regex!(has_guard_regex, r"^\s*context(?:\.[A-Za-z_][A-Za-z0-9_]*)*\s+has\s+[A-Za-z_][A-Za-z0-9_]*\s*$");
cached_regex!(bool_isfalse_regex, r"^\s*!\s*context\.([A-Za-z_][A-Za-z0-9_.]*)\s*$");
cached_regex!(bool_istrue_regex, r"^\s*context\.([A-Za-z_][A-Za-z0-9_.]*)\s*$");
cached_regex!(string_in_regex, r#"^\s*\[(.*)\]\.contains\(context\.([A-Za-z_][A-Za-z0-9_.]*)\)\s*$"#);
cached_regex!(method_call_regex, r"^\s*context\.([A-Za-z_][A-Za-z0-9_.]*)\.([A-Za-z]+)\((.*)\)\s*$");
cached_regex!(infix_cmp_regex, r"^\s*context\.([A-Za-z_][A-Za-z0-9_.]*)\s*(==|!=|>=|<=|>|<)\s*(.+)$");
cached_regex!(decimal_arg_regex, r#"^\s*decimal\("((?:\\.|[^"\\])*)"\)\s*$"#);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::generator::compile;
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

    fn roundtrip(rule: &PolicyRule) -> PolicyRule {
        let text = compile(rule, &swap::schema()).unwrap();
        parse_cedar(&text).unwrap_or_else(|e| panic!("parse failed for:\n{text}\n— err: {e}"))
    }

    #[test]
    fn unconditional_forbid_roundtrips() {
        let rule = rule_with(vec![]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.id, rule.id);
        assert_eq!(parsed.action, rule.action);
        assert_eq!(parsed.severity, rule.severity);
        assert_eq!(parsed.reason, rule.reason);
        assert!(parsed.predicates.is_empty());
    }

    #[test]
    fn long_gt_roundtrips() {
        let rule = rule_with(vec![Predicate {
            field: "feeBps".into(),
            op: "gt".into(),
            value: PredicateValue::Single("100".into()),
        }]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.predicates.len(), 1);
        assert_eq!(parsed.predicates[0].field, "feeBps");
        assert_eq!(parsed.predicates[0].op, "gt");
        match &parsed.predicates[0].value {
            PredicateValue::Single(s) => assert_eq!(s, "100"),
            other => panic!("expected Single, got {other:?}"),
        }
    }

    #[test]
    fn decimal_method_roundtrips() {
        let rule = rule_with(vec![Predicate {
            field: "totalInputUsd.value".into(),
            op: "gt".into(),
            value: PredicateValue::Single("100.00".into()),
        }]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.predicates.len(), 1);
        assert_eq!(parsed.predicates[0].field, "totalInputUsd.value");
        assert_eq!(parsed.predicates[0].op, "gt");
        match &parsed.predicates[0].value {
            PredicateValue::Single(s) => assert_eq!(s, "100.00"),
            other => panic!("expected Single, got {other:?}"),
        }
    }

    #[test]
    fn set_string_contains_any_roundtrips() {
        let rule = rule_with(vec![Predicate {
            field: "totalInputUsd.sources".into(),
            op: "containsAny".into(),
            value: PredicateValue::Multi(vec!["chainlink".into(), "pyth".into()]),
        }]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.predicates.len(), 1);
        assert_eq!(parsed.predicates[0].op, "containsAny");
        match &parsed.predicates[0].value {
            PredicateValue::Multi(vs) => {
                assert_eq!(vs, &vec!["chainlink".to_string(), "pyth".to_string()]);
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn string_in_roundtrips() {
        let rule = rule_with(vec![Predicate {
            field: "swapMode".into(),
            op: "in".into(),
            value: PredicateValue::Multi(vec!["exact_in".into(), "market".into()]),
        }]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.predicates.len(), 1);
        assert_eq!(parsed.predicates[0].field, "swapMode");
        assert_eq!(parsed.predicates[0].op, "in");
    }

    #[test]
    fn bool_is_true_and_is_false_roundtrip() {
        let rule_true = rule_with(vec![Predicate {
            field: "recipientIsContract".into(),
            op: "isTrue".into(),
            value: PredicateValue::None,
        }]);
        let parsed_true = roundtrip(&rule_true);
        assert_eq!(parsed_true.predicates[0].op, "isTrue");

        let rule_false = rule_with(vec![Predicate {
            field: "recipientIsContract".into(),
            op: "isFalse".into(),
            value: PredicateValue::None,
        }]);
        let parsed_false = roundtrip(&rule_false);
        assert_eq!(parsed_false.predicates[0].op, "isFalse");
    }

    #[test]
    fn warn_severity_roundtrips() {
        let mut rule = rule_with(vec![]);
        rule.severity = Severity::Warn;
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.severity, Severity::Warn);
    }

    #[test]
    fn id_with_quote_roundtrips() {
        let mut rule = rule_with(vec![]);
        rule.id = r#"user/quote"id"#.into();
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.id, r#"user/quote"id"#);
    }

    #[test]
    fn comments_are_stripped() {
        let text = r#"
        // a leading comment
        @id("x")
        @severity("deny")
        @reason("r") /* mid comment */
        forbid ( principal is Wallet, action == Action::"swap", resource is Protocol );
        "#;
        let parsed = parse_cedar(text).unwrap();
        assert_eq!(parsed.id, "x");
        assert_eq!(parsed.action, "swap");
        assert_eq!(parsed.severity, Severity::Deny);
    }

    #[test]
    fn permit_is_rejected() {
        let text = r#"
        @id("x")
        @severity("deny")
        @reason("r")
        permit ( principal, action, resource );
        "#;
        let err = parse_cedar(text).unwrap_err();
        assert!(matches!(err, ParseError::MalformedHead));
    }

    #[test]
    fn missing_severity_is_rejected() {
        let text = r#"
        @id("x")
        @reason("r")
        forbid ( principal is Wallet, action == Action::"swap", resource is Protocol );
        "#;
        let err = parse_cedar(text).unwrap_err();
        assert!(matches!(err, ParseError::MissingAnnotation("severity")));
    }

    #[test]
    fn multiple_predicates_roundtrip_in_order() {
        let rule = rule_with(vec![
            Predicate {
                field: "feeBps".into(),
                op: "gt".into(),
                value: PredicateValue::Single("100".into()),
            },
            Predicate {
                field: "swapMode".into(),
                op: "eq".into(),
                value: PredicateValue::Single("exact_in".into()),
            },
        ]);
        let parsed = roundtrip(&rule);
        assert_eq!(parsed.predicates.len(), 2);
        // generator may interleave `has`-guards but predicate fragments stay
        // in their input order — verify by tuple shape.
        let shapes: Vec<_> = parsed
            .predicates
            .iter()
            .map(|p| (p.field.as_str(), p.op.as_str()))
            .collect();
        assert_eq!(shapes, vec![("feeBps", "gt"), ("swapMode", "eq")]);
    }
}
