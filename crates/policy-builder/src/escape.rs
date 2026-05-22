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

/// Coerce a UI integer input into the strict form `escape_long`
/// accepts. Tolerates a fractional part of all-zeros (`"1.0"`,
/// `"100.00"`, `"-1.0"`) since users naturally copy-paste those from
/// DEX UIs and explorers — strips the fractional zeros and emits the
/// integer. Rejects any non-zero fractional digit (`"1.5"`) so we
/// never silently round.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidLong`] when the input has a non-zero
/// fractional part, non-digit characters, multiple `.`, or any shape
/// that can't be coerced to an integer by trimming trailing `.0+`.
#[must_use]
pub fn normalize_long_input(value: &str) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    let (sign, rest) = match trimmed.as_bytes()[0] {
        b'-' => ("-", &trimmed[1..]),
        b'+' => ("", &trimmed[1..]),
        _ => ("", trimmed),
    };
    if rest.is_empty() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    let mut parts = rest.splitn(3, '.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    // The fractional part must be all-zeros — anything else would mean
    // silently rounding the user's input.
    if frac_part.bytes().any(|b| b != b'0') {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    Ok(format!("{sign}{int_part}"))
}

/// Inverse of [`scale_decimal_to_long`]. Given an integer literal and a
/// scale, reinsert the decimal point so a Long policy literal can be
/// pretty-printed back as the human-facing value the user originally typed.
/// Trailing zeros in the fractional part are trimmed (keeping at least one)
/// so `1000000000` at scale 9 round-trips to `1.0` rather than
/// `1.000000000`.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidLong`] when the input isn't a plain
/// integer literal.
pub fn unscale_long_to_decimal(value: &str, scale: u8) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    let (sign, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        ("-", rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        ("", rest)
    } else {
        ("", trimmed)
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }

    let scale_usize = scale as usize;
    if scale_usize == 0 {
        return Ok(format!("{sign}{digits}"));
    }

    // Left-pad with zeros so we always have at least `scale + 1` digits to
    // split (one for the integer part, `scale` for the fraction).
    let padded = if digits.len() <= scale_usize {
        format!("{:0>width$}", digits, width = scale_usize + 1)
    } else {
        digits.to_string()
    };
    let split_at = padded.len() - scale_usize;
    let int_part = &padded[..split_at];
    let frac_part = &padded[split_at..];
    let frac_trimmed = frac_part.trim_end_matches('0');
    let frac_display = if frac_trimmed.is_empty() {
        "0"
    } else {
        frac_trimmed
    };

    // -0.0 is meaningless; strip the sign when the value rounds to zero.
    let normalized_sign = if sign == "-" && int_part.trim_start_matches('0').is_empty()
        && frac_display == "0"
    {
        ""
    } else {
        sign
    };
    Ok(format!("{normalized_sign}{int_part}.{frac_display}"))
}

/// Convert a decimal-shaped user input (`"0.5"`, `"100"`, `"0.000000001"`)
/// into the matching i64 Long literal after a fixed `10^scale` rescale, then
/// emit it as a Cedar Long literal. Used by token-native amount fields so a
/// policy written as `> 0.5` lines up with the manifest-rescaled context
/// value regardless of the underlying token's decimals.
///
/// `scale = 9` is the typical Gwei-style choice — preserves 9 fractional
/// digits, max magnitude ~9.2 × 10⁹ before i64 overflow.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidLong`] when:
/// - the input isn't a valid decimal (`[-]?<digits>[.<digits>]`);
/// - the fractional part has more digits than `scale` (would lose precision);
/// - the rescaled value doesn't fit in i64.
pub fn scale_decimal_to_long(value: &str, scale: u8) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }

    let mut i = 0;
    let is_negative = match bytes.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };

    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_part = &trimmed[int_start..i];
    if int_part.is_empty() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }

    let frac_part = if bytes.get(i) == Some(&b'.') {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        &trimmed[frac_start..i]
    } else {
        ""
    };

    // Anything left after the optional fractional part means we never
    // accepted the full literal (e.g. `"1.5abc"`). Reject rather than
    // silently truncating.
    if i != bytes.len() {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }
    if frac_part.len() > scale as usize {
        return Err(EscapeError::InvalidLong(value.to_string()));
    }

    // Combine integer + fractional digits, right-pad with zeros so the
    // resulting string represents `value × 10^scale` as an integer.
    let pad = (scale as usize) - frac_part.len();
    let mut combined = String::with_capacity(int_part.len() + frac_part.len() + pad + 1);
    combined.push_str(int_part);
    combined.push_str(frac_part);
    for _ in 0..pad {
        combined.push('0');
    }

    // Strip leading zeros so i64::parse is happy with normalized form; keep
    // a single "0" when the magnitude rounds out to zero.
    let stripped = combined.trim_start_matches('0');
    let magnitude = if stripped.is_empty() { "0" } else { stripped };

    let signed = if is_negative && magnitude != "0" {
        format!("-{magnitude}")
    } else {
        magnitude.to_string()
    };

    signed
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

/// Coerce a user-friendly decimal-shaped string into the strict
/// `[-]?<digits>.<1..=4 digits>` form Cedar's `decimal()` parser
/// accepts. Lets callers pass `"1"` / `".5"` / `"1."` / `"-1"` — the
/// shapes a UI text input naturally produces — instead of forcing
/// every entry point to remember Cedar's literal grammar.
///
/// Returns the input unchanged when it already has a fractional part,
/// even with trailing zeros (`"1.50"` stays `"1.50"`) — the
/// `escape_decimal` validator decides whether the shape is acceptable.
///
/// # Errors
///
/// Returns [`EscapeError::InvalidDecimal`] for empty input, multiple
/// `.`, non-digit characters, or any other shape that can't be coerced
/// to Cedar's accepted form by appending/prepending a `0`.
#[must_use]
pub fn normalize_decimal_input(value: &str) -> Result<String, EscapeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    let (sign, rest) = match trimmed.as_bytes()[0] {
        b'-' => ("-", &trimmed[1..]),
        b'+' => ("", &trimmed[1..]),
        _ => ("", trimmed),
    };
    if rest.is_empty() || rest.contains(' ') {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    // Split on `.` once; more than one dot is invalid.
    let mut parts = rest.splitn(3, '.');
    let int_part_raw = parts.next().unwrap_or("");
    let frac_part_raw = parts.next();
    if parts.next().is_some() {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    let int_part_normalized = if int_part_raw.is_empty() {
        "0"
    } else {
        int_part_raw
    };
    let frac_part_normalized = match frac_part_raw {
        None => "0",
        Some("") => "0",
        Some(f) => f,
    };
    if !int_part_normalized.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    if !frac_part_normalized.bytes().all(|b| b.is_ascii_digit()) {
        return Err(EscapeError::InvalidDecimal(value.to_string()));
    }
    Ok(format!(
        "{sign}{int_part_normalized}.{frac_part_normalized}"
    ))
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

    #[test]
    fn normalize_decimal_input_coerces_loose_shapes() {
        // Integers get a `.0` so Cedar's parser accepts them.
        assert_eq!(normalize_decimal_input("1").unwrap(), "1.0");
        assert_eq!(normalize_decimal_input("100").unwrap(), "100.0");
        assert_eq!(normalize_decimal_input("-1").unwrap(), "-1.0");
        assert_eq!(normalize_decimal_input("+5").unwrap(), "5.0");
        // Already-decimal stays decimal (trailing zeros preserved — the
        // caller's strict validator decides if 1..=4 digits is OK).
        assert_eq!(normalize_decimal_input("1.5").unwrap(), "1.5");
        assert_eq!(normalize_decimal_input("1.50").unwrap(), "1.50");
        assert_eq!(normalize_decimal_input("-0.25").unwrap(), "-0.25");
        // Missing fractional digits ("1.") fills in with "0".
        assert_eq!(normalize_decimal_input("1.").unwrap(), "1.0");
        // Missing integer part (".5") prepends "0".
        assert_eq!(normalize_decimal_input(".5").unwrap(), "0.5");
        assert_eq!(normalize_decimal_input("-.5").unwrap(), "-0.5");
        // Whitespace is trimmed.
        assert_eq!(normalize_decimal_input("  42 ").unwrap(), "42.0");
    }

    #[test]
    fn normalize_decimal_input_rejects_broken_shapes() {
        assert!(normalize_decimal_input("").is_err());
        assert!(normalize_decimal_input("   ").is_err());
        assert!(normalize_decimal_input("-").is_err());
        assert!(normalize_decimal_input("abc").is_err());
        assert!(normalize_decimal_input("1.2.3").is_err()); // multi-dot
        assert!(normalize_decimal_input("1 5").is_err()); // internal space
        assert!(normalize_decimal_input("1.5a").is_err());
    }

    #[test]
    fn normalize_long_input_accepts_fractional_zero() {
        // Common copy-paste artifact: explorers / DEX UIs render
        // integers with a trailing `.0` (`100.0`, `-1.0`). The
        // normalizer strips them; the downstream `escape_long` then
        // succeeds. Non-zero fractional digits still fail — we don't
        // silently round.
        assert_eq!(normalize_long_input("1").unwrap(), "1");
        assert_eq!(normalize_long_input("1.0").unwrap(), "1");
        assert_eq!(normalize_long_input("100.00").unwrap(), "100");
        assert_eq!(normalize_long_input("-1").unwrap(), "-1");
        assert_eq!(normalize_long_input("-1.0").unwrap(), "-1");
        assert_eq!(normalize_long_input("+5").unwrap(), "5");
        assert_eq!(normalize_long_input("  42 ").unwrap(), "42");
        assert_eq!(normalize_long_input("0").unwrap(), "0");
        assert_eq!(normalize_long_input("0.0").unwrap(), "0");
    }

    #[test]
    fn normalize_long_input_rejects_non_zero_fraction_and_garbage() {
        // Silent rounding would be a footgun, so we reject any
        // non-zero fractional digit.
        assert!(normalize_long_input("1.5").is_err());
        assert!(normalize_long_input("100.01").is_err());
        assert!(normalize_long_input("-0.5").is_err());

        // Generic garbage stays rejected.
        assert!(normalize_long_input("").is_err());
        assert!(normalize_long_input("   ").is_err());
        assert!(normalize_long_input("abc").is_err());
        assert!(normalize_long_input("1.0.0").is_err());
        assert!(normalize_long_input("1.5a").is_err());
        assert!(normalize_long_input(".5").is_err()); // no int part
        assert!(normalize_long_input("-").is_err());
    }

    #[test]
    fn escape_decimal_after_normalize_round_trip() {
        // Confirms the post-Phase fix: `escape_decimal(normalize("1"))`
        // produces the same Cedar literal as `escape_decimal("1.0")`.
        let normalized = normalize_decimal_input("1").unwrap();
        assert_eq!(
            escape_decimal(&normalized).unwrap(),
            escape_decimal("1.0").unwrap(),
        );
    }

    #[test]
    fn scale_decimal_to_long_integer_input() {
        assert_eq!(scale_decimal_to_long("0", 9).unwrap(), "0");
        assert_eq!(scale_decimal_to_long("1", 9).unwrap(), "1000000000");
        assert_eq!(scale_decimal_to_long("100", 9).unwrap(), "100000000000");
    }

    #[test]
    fn scale_decimal_to_long_fractional_input() {
        // 0.5 → 500_000_000 (= 0.5 × 10⁹)
        assert_eq!(scale_decimal_to_long("0.5", 9).unwrap(), "500000000");
        // 0.00003 ETH style, the exact case the type system used to lose.
        assert_eq!(scale_decimal_to_long("0.00003", 9).unwrap(), "30000");
        // Min unit at scale=9.
        assert_eq!(scale_decimal_to_long("0.000000001", 9).unwrap(), "1");
        // Trailing zeros in fraction shouldn't change magnitude.
        assert_eq!(scale_decimal_to_long("1.0", 9).unwrap(), "1000000000");
        assert_eq!(scale_decimal_to_long("1.50", 9).unwrap(), "1500000000");
    }

    #[test]
    fn scale_decimal_to_long_negative() {
        assert_eq!(scale_decimal_to_long("-1", 9).unwrap(), "-1000000000");
        assert_eq!(scale_decimal_to_long("-0.0", 9).unwrap(), "0");
    }

    #[test]
    fn scale_decimal_to_long_rejects_overprecise() {
        // 10 fractional digits at scale=9 would lose the last digit silently;
        // we reject so users notice and round explicitly.
        assert!(scale_decimal_to_long("0.0000000001", 9).is_err());
    }

    #[test]
    fn scale_decimal_to_long_rejects_overflow() {
        // 10¹⁰ × 10⁹ = 10¹⁹ — overflows i64 (max ≈ 9.22 × 10¹⁸).
        assert!(scale_decimal_to_long("10000000000", 9).is_err());
    }

    #[test]
    fn scale_decimal_to_long_rejects_bad_shape() {
        assert!(scale_decimal_to_long("", 9).is_err());
        assert!(scale_decimal_to_long(".", 9).is_err());
        assert!(scale_decimal_to_long(".5", 9).is_err()); // no int part
        assert!(scale_decimal_to_long("abc", 9).is_err());
        assert!(scale_decimal_to_long("1.5abc", 9).is_err());
        assert!(scale_decimal_to_long("1.2.3", 9).is_err());
    }

    #[test]
    fn unscale_long_round_trips_with_scale() {
        for (decimal_input, scale, expected_long, expected_back) in [
            ("0.00003", 9u8, "30000", "0.00003"),
            ("1", 9, "1000000000", "1.0"),
            ("1.5", 9, "1500000000", "1.5"),
            ("100", 9, "100000000000", "100.0"),
            ("0", 9, "0", "0.0"),
            ("0.000000001", 9, "1", "0.000000001"),
        ] {
            let long = scale_decimal_to_long(decimal_input, scale).unwrap();
            assert_eq!(long, expected_long, "scale_decimal_to_long({decimal_input})");
            let back = unscale_long_to_decimal(&long, scale).unwrap();
            assert_eq!(back, expected_back, "unscale({long})");
        }
    }

    #[test]
    fn unscale_long_zero_padding_correct() {
        // "5" with scale=9 must produce "0.000000005", not "5.0" or "0.5".
        assert_eq!(unscale_long_to_decimal("5", 9).unwrap(), "0.000000005");
    }

    #[test]
    fn unscale_long_negative_round_trip() {
        let long = scale_decimal_to_long("-1.5", 9).unwrap();
        assert_eq!(long, "-1500000000");
        assert_eq!(unscale_long_to_decimal(&long, 9).unwrap(), "-1.5");
    }

    #[test]
    fn unscale_long_rejects_non_integer() {
        assert!(unscale_long_to_decimal("1.5", 9).is_err());
        assert!(unscale_long_to_decimal("abc", 9).is_err());
    }
}
