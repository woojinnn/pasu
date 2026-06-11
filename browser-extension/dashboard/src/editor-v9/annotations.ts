/**
 * Cedar annotation stamper.
 *
 * The DB row is the source of truth for `name` (mapped to `@id`) and
 * `severity`. On save we re-stamp the cedar text so the annotations
 * always match the metadata columns — preventing drift between the
 * two representations.
 *
 * Strategy:
 *   1. Strip any existing top-level `@id(...)` and `@severity(...)`
 *      annotations from the head of the text.
 *   2. Prepend fresh `@id(...)` and `@severity(...)` derived from the
 *      passed `name` and `severity`.
 *
 * We intentionally only touch the head of the file — annotations
 * deeper inside (mid-policy, after `permit(...)`) aren't valid Cedar
 * anyway, and any inline `@key("value")` the user typed below is
 * untouched.
 */

const ANNOTATION_HEAD = /^(?:\s*@(?:id|severity|reason)\s*\([^)]*\)\s*)+/;

/** Sanitize policy name into a Cedar-friendly id (alphanum + dash + underscore).
 *  Cedar identifiers conventionally stick to ASCII; non-ASCII (Korean,
 *  emoji, etc.) is stripped. If sanitization leaves nothing meaningful
 *  (all-Korean name → only underscores remain), fall back to "policy"
 *  so the emitted annotation reads `@id("policy")` rather than
 *  `@id("___")`. */
export function policyIdFromName(name: string): string {
  const trimmed = (name || "").trim();
  if (!trimmed) return "policy";
  const sanitized = trimmed
    .replace(/\s+/g, "_")
    .replace(/[^A-Za-z0-9_\-:.]/g, "");
  if (!sanitized || /^[_\-.:]+$/.test(sanitized)) return "policy";
  return sanitized;
}

/**
 * Lowercase every EVM-address string literal in `cedarText`.
 *
 * WHY: the engine normalises all addresses it derives from a transaction to
 * lowercase (Rust hex-formats lower; the token registry lowercases too), so
 * `context.tokenIn.key.address` is always lowercase. Cedar string comparison
 * is case-SENSITIVE, so a checksum-cased literal a user typed (e.g. WETH
 * `0xC02aaA39…`) can never equal the lowercase context value — the policy
 * silently never fires. Normalising address literals to lowercase here makes
 * the stored/installed policy canonical and symmetric with the context.
 *
 * Matches exactly a quoted `0x` + 40 hex digits + quote, so a 32-byte hash
 * (64 hex) or a 4-byte selector (8 hex) is left untouched. Lowercasing only
 * drops the optional EIP-55 checksum casing — the address VALUE is unchanged.
 */
export function lowercaseAddressLiterals(cedarText: string): string {
  return cedarText.replace(/"0[xX][0-9a-fA-F]{40}"/g, (m) => m.toLowerCase());
}

/** Re-stamp `@id` + `@severity` + `@reason` annotations onto the head of
 *  `cedarText`. An existing head `@reason("…")` is preserved verbatim — it is
 *  the user-facing message and must survive a re-save; only when absent does
 *  the policy name stand in, so the popup never falls back to
 *  "(no reason annotation)". */
export function stampAnnotations(
  cedarText: string,
  name: string,
  severity: string,
): string {
  const head = ANNOTATION_HEAD.exec(cedarText)?.[0] ?? "";
  const body = cedarText.replace(ANNOTATION_HEAD, "");
  const id = policyIdFromName(name);
  const sev = severity.replace(/"/g, '\\"');
  // 기존 사유는 이미 이스케이프된 리터럴 그대로 재삽입한다(이중 이스케이프 방지).
  const existingReason = /@reason\s*\(\s*"([^)]*)"\s*\)/.exec(head)?.[1];
  const reason =
    existingReason && existingReason.trim()
      ? existingReason
      : (name || "policy").replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `@id("${id}")\n@severity("${sev}")\n@reason("${reason}")\n${body}`;
}
