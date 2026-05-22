// EVM address input normalization for the rule builder.
//
// Schema declares address fields with `pattern = "^0x[0-9a-fA-F]{40}$"`.
// The Rust validator rejects anything that doesn't match — including
// the two near-misses users routinely produce:
//   - bare 40-hex without the `0x` prefix (`"abcd…"` instead of
//     `"0xabcd…"`), pasted from Etherscan's "copy address" buttons
//     or from Solidity console output
//   - uppercase prefix (`"0Xabcd…"`), from some legacy tools and
//     unicode-normalized clipboard sources
//
// We DON'T touch the 40 hex characters' case so EIP-55 checksum
// addresses round-trip exactly as typed. Only the `0x` prefix is
// case-folded, and only when missing it's prepended.
//
// Dashboard-only — the Rust pattern check stays strict so any SDK
// caller still hits the same validation gate.

const EVM_ADDRESS_PATTERN = "^0x[0-9a-fA-F]{40}$";
const BARE_40_HEX = /^[0-9a-fA-F]{40}$/;

/**
 * Whether the given field's declared regex is the EVM address shape.
 * We compare strings (not pre-compiled regexes) because the schema
 * stores patterns as their source form and the static address shape
 * is well-known. Other patterns (token id digits, decimal-string
 * amount, etc.) are left untouched.
 */
export function isEvmAddressField(pattern: string | undefined): boolean {
  return pattern === EVM_ADDRESS_PATTERN;
}

/**
 * Coerce common EVM address typos into the canonical `0x<40-hex>`
 * shape. Returns input unchanged when:
 *   - it's empty / whitespace-only (don't prepend on empty field)
 *   - it doesn't structurally resemble an address (length / charset
 *     mismatch) — let the Rust validator surface the pattern error
 *     with the original input
 */
export function normalizeAddressForDisplay(raw: string): string {
  const trimmed = raw.trim();
  if (trimmed === "") return raw;

  // Already canonical — leave the checksum case alone.
  if (/^0x[0-9a-fA-F]{40}$/.test(trimmed)) return trimmed;

  // Uppercase `0X` prefix — normalize to lowercase, preserve the rest.
  if (/^0X[0-9a-fA-F]{40}$/.test(trimmed)) {
    return `0x${trimmed.slice(2)}`;
  }

  // Bare 40-hex without prefix — prepend `0x`.
  if (BARE_40_HEX.test(trimmed)) {
    return `0x${trimmed}`;
  }

  // Doesn't match any known near-miss — leave alone so the Rust
  // pattern validator's error mentions the original input.
  return raw;
}
