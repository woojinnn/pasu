//! Cedar literal escaping.
//!
//! Two literal flavors matter for the generator:
//! - **String literals** appear inside `"…"`; backslash and double-quote must
//!   be backslash-escaped. Cedar follows the same convention as JSON for the
//!   subset we emit, so [`escape_string`] uses `serde_json::to_string` for
//!   safety against control bytes and Unicode edge cases.
//! - **Integer literals** must be plain decimal digits (optionally signed).
//!   [`escape_long`] validates the input parses to `i64` — any non-numeric
//!   input would otherwise inject syntax into the emitted policy.
//! - **Decimal literals** are emitted as `decimal("…")` — the inside is a
//!   string literal, but we additionally enforce the `<digits>.<digits>` shape
//!   Cedar's parser accepts.

use thiserror::Error;

/// Escape errors produced during operand emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EscapeError {
    /// `escape_long` received an operand that isn't a valid `i64`.
    #[error("invalid Long literal: {0}")]
    InvalidLong(String),
    /// `escape_decimal` received an operand whose shape Cedar would reject.
    ///
    /// Cedar's `decimal` parser accepts a sign, integer digits, exactly one
    /// `.`, and 1–4 fractional digits.
    #[error("invalid decimal literal: {0}")]
    InvalidDecimal(String),
    /// `unescape_string` received a literal whose escape sequences don't parse.
    #[error("invalid string literal escape: {0}")]
    InvalidEscape(String),
}

/// Emit a Cedar string literal: `"…"` with `\\` and `"` escaped.
///
/// # Panics
///
/// Never in practice: `serde_json::to_string` for a `&str` only fails when
/// the underlying writer fails, and we're writing into a fresh `String`.
#[must_use]
pub fn escape_string(value: &str) -> String {
    // serde_json::to_string handles \\, \", and any control bytes safely.
    // The grammar overlap with Cedar string literals is exact for the
    // characters we ever emit (ASCII + escapes).
    serde_json::to_string(value).expect("string serialization is infallible")
}

/// Decode the inner content of a Cedar/JSON string literal.
///
/// `inner` is the run of characters between the surrounding `"…"`; this
/// function reapplies the quotes and parses via `serde_json` so escapes
/// like `\"`, `\\`, `\n`, `\uXXXX` all round-trip exactly the way
/// [`escape_string`] writes them.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidEscape`] when the literal contains an
/// escape sequence that JSON's grammar doesn't accept.
pub fn unescape_string(inner: &str) -> Result<String, EscapeError> {
    let wrapped = format!("\"{inner}\"");
    serde_json::from_str::<String>(&wrapped)
        .map_err(|_| EscapeError::InvalidEscape(inner.to_string()))
}

/// Emit an unquoted Cedar `Long` literal after validating range.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidLong`] if `value` isn't a base-10 `i64`.
pub fn escape_long(value: &str) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    trimmed
        .parse::<i64>()
        .map(|n| n.to_string())
        .map_err(|_| EscapeError::InvalidLong(value.to_string()))
}

/// Emit a Cedar `decimal("…")` literal after validating the inner shape.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidDecimal`] when the operand doesn't match
/// Cedar's accepted `[-]?<digits>.<1..=4 digits>` form.
pub fn escape_decimal(value: &str) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    if !is_valid_decimal_lex(trimmed) {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    Ok(format!("decimal({})", escape_string(trimmed)))
}

fn is_valid_decimal_lex(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == int_start {
        return false;
    }
    if bytes.get(i) != Some(&b'.') {
        return false;
    }
    i += 1;
    let frac_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let frac_len = i - frac_start;
    if !(1..=4).contains(&frac_len) {
        return false;
    }
    i == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_string_quotes_and_escapes() {
        assert_eq!(escape_string("hello"), r#""hello""#);
        assert_eq!(escape_string(r#"a"b"#), r#""a\"b""#);
        assert_eq!(escape_string("a\\b"), r#""a\\b""#);
    }

    #[test]
    fn escape_long_accepts_valid_i64() {
        assert_eq!(escape_long("100").unwrap(), "100");
        assert_eq!(escape_long("-1").unwrap(), "-1");
        assert_eq!(escape_long("  42 ").unwrap(), "42");
    }

    #[test]
    fn escape_long_rejects_non_numeric() {
        assert!(escape_long("abc").is_err());
        assert!(escape_long("1.5").is_err());
        assert!(escape_long("99999999999999999999").is_err());
    }

    #[test]
    fn escape_decimal_accepts_shape() {
        assert_eq!(escape_decimal("100.00").unwrap(), r#"decimal("100.00")"#);
        assert_eq!(escape_decimal("-1.5").unwrap(), r#"decimal("-1.5")"#);
        assert_eq!(escape_decimal("0.1234").unwrap(), r#"decimal("0.1234")"#);
    }

    #[test]
    fn escape_decimal_rejects_bad_shape() {
        assert!(escape_decimal("100").is_err()); // no dot
        assert!(escape_decimal(".5").is_err()); // no int part
        assert!(escape_decimal("1.").is_err()); // no frac digits
        assert!(escape_decimal("1.12345").is_err()); // too many frac digits
        assert!(escape_decimal("1.5abc").is_err()); // trailing junk
    }
}
