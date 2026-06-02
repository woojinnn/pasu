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

/** Re-stamp `@id` + `@severity` + `@reason` annotations onto the head of
 *  `cedarText`. The reason is the human-readable name so the extension
 *  popup can surface "this policy fired because of <name>" — without a
 *  `@reason`, the popup falls back to "(no reason annotation)". */
export function stampAnnotations(
  cedarText: string,
  name: string,
  severity: string,
): string {
  const body = cedarText.replace(ANNOTATION_HEAD, "");
  const id = policyIdFromName(name);
  const sev = severity.replace(/"/g, '\\"');
  const reason = (name || "policy").replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `@id("${id}")\n@severity("${sev}")\n@reason("${reason}")\n${body}`;
}
