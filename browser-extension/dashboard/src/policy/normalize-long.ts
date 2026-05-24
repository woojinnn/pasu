// Long input normalization for the rule builder.
//
// Cedar's `Long` literal is a plain base-10 integer. The Rust emit
// path is strict: `escape_long("1.0")` fails with "invalid Long
// literal: 1.0". That's a footgun for users copy-pasting integers
// from explorers / DEX UIs that render trailing `.0` on whole
// numbers. The Rust side now coerces `"1.0"` → `"1"` defensively
// (see `policy_builder::escape::normalize_long_input`); this TS
// mirror runs on blur so the user SEES the canonical form before
// hitting Compile.
//
// Mirror rules with the Rust normalizer; change one, change the
// other. Non-zero fractional digits are still rejected so we never
// silently round.

/**
 * Coerce a UI integer input into the strict form Cedar's `Long`
 * literal expects. Returns input unchanged when:
 *   - it's empty / whitespace-only (don't surprise users with `0`)
 *   - it's already a plain integer
 *   - it has a non-zero fractional digit (would imply rounding — let
 *     the Rust validator surface the error verbatim)
 *   - it's garbage we can't interpret
 */
export function normalizeLongForDisplay(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed === "") return raw;

  const signMatch = /^([+-]?)(.*)$/.exec(trimmed);
  if (!signMatch) return raw;
  const sign = signMatch[1] === "-" ? "-" : "";
  const rest = signMatch[2];
  if (rest === "") return raw;

  if (!rest.includes(".")) {
    // Already integer-shaped — only normalize sign (`+5` → `5`,
    // already-trimmed whitespace).
    if (!/^[0-9]+$/.test(rest)) return raw;
    return `${sign}${rest}`;
  }

  const parts = rest.split(".");
  if (parts.length > 2) return raw;
  const [intPart, fracPart] = parts;
  if (intPart === "" || !/^[0-9]+$/.test(intPart)) return raw;
  if (fracPart !== "" && !/^[0-9]+$/.test(fracPart)) return raw;
  // All-zero fractional → safe to drop. Anything else → leave alone
  // so the user sees the "invalid Long literal" error from Rust and
  // realizes they typed a non-integer.
  if (/^0*$/.test(fracPart)) return `${sign}${intPart}`;
  return raw;
}
