import type { PolicySeverity } from "../../server-api";

/** Cedar 텍스트의 `@severity(…)` annotation 파서 — 에디터/마켓 공용. */
const SEVERITY_RE = /@severity\("(deny|warn|info)"\)/;

/** Read the `@severity(…)` annotation; defaults to `"deny"` if absent. */
export function severityFromCedar(text: string): PolicySeverity {
  const m = text.match(SEVERITY_RE);
  return (m?.[1] as PolicySeverity | undefined) ?? "deny";
}
