import type { ManagedPolicy, PolicySeverity } from "../../server-api";

/**
 * Small parsers reused by every editor page — list cards need the same
 * `severity` and `name` derivation as the panel itself.
 */

const SEVERITY_RE = /@severity\("(deny|warn|info)"\)/;
const ID_ANNOTATION_RE = /@id\("([^"]+)"\)/;

/** Read the `@severity(…)` annotation; defaults to `"deny"` if absent. */
export function severityFromCedar(text: string): PolicySeverity {
  const m = text.match(SEVERITY_RE);
  return (m?.[1] as PolicySeverity | undefined) ?? "deny";
}

/** Prefer the user-set `displayName`; fall back to the `@id(…)` annotation,
 *  finally to "untitled". */
export function nameFromPolicy(p: ManagedPolicy): string {
  if (p.displayName?.trim()) return p.displayName.trim();
  const m = p.text.match(ID_ANNOTATION_RE);
  return m?.[1] ?? "untitled";
}
