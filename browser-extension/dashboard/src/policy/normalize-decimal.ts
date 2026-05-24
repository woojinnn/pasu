// Decimal input normalization for the rule builder.
//
// Cedar's `decimal()` parser requires the strict form
// `[-]?<digits>.<1..=4 digits>` — an integer like `1` or a leading-dot
// `.5` are both rejected with `invalid decimal literal: <input>`. The
// Rust emit path (`policy_builder::escape::normalize_decimal_input`)
// already coerces these shapes before validation as a defensive
// backstop, but we also normalize here on the dashboard side so the
// USER sees the same canonical form their value will compile to
// instead of staring at the literal they typed.
//
// Display normalization fires on blur — typing stays unconstrained so
// `.` doesn't get eaten mid-entry. On blur:
//   - `"1"`     → `"1.0"`
//   - `"1."`    → `"1.0"`
//   - `".5"`    → `"0.5"`
//   - `"-.25"`  → `"-0.25"`
//   - `"1.5"`   → `"1.5"`     (already canonical)
//   - `"abc"`   → `"abc"`     (invalid — leave alone, Rust will report)
//   - `""`      → `""`        (don't fill empty inputs with `"0.0"`)

/**
 * Coerce a UI decimal input into the strict form Cedar's `decimal()`
 * literal expects. Mirrors the Rust `normalize_decimal_input` so the
 * two layers stay in lockstep; if you change one, change the other.
 *
 * Returns the input unchanged when:
 *   - it's empty or whitespace-only (don't surprise users by filling)
 *   - it's already in canonical `<int>.<frac>` form
 *   - it can't be interpreted as a decimal at all — better to surface
 *     the Rust validator's error than to coerce a typo into a
 *     plausible-looking literal
 */
export function normalizeDecimalForDisplay(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed === "") return raw;

  const signMatch = /^([+-]?)(.*)$/.exec(trimmed);
  if (!signMatch) return raw;
  const sign = signMatch[1] === "-" ? "-" : "";
  const rest = signMatch[2];
  if (rest === "") return raw;

  const parts = rest.split(".");
  if (parts.length > 2) return raw;
  const [intPartRaw, fracPartRaw = ""] = parts;

  // Reject anything containing non-digit characters in either segment.
  // (Rust's normalizer does the same check; mirroring keeps drift bait
  // out of the two-layer story.)
  if (intPartRaw !== "" && !/^[0-9]+$/.test(intPartRaw)) return raw;
  if (fracPartRaw !== "" && !/^[0-9]+$/.test(fracPartRaw)) return raw;

  const intPart = intPartRaw === "" ? "0" : intPartRaw;
  const fracPart = fracPartRaw === "" ? "0" : fracPartRaw;
  return `${sign}${intPart}.${fracPart}`;
}
