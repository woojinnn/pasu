import type { ManagedPolicy, PolicySet } from "../../../server-api";

/** There is no draft lifecycle anymore — a policy only exists once saved, and
 *  saving always makes it live. Kept as a stub so legacy `life:"draft"` rows
 *  (created before the change) render as ordinary, toggleable policies. */
export function isDraft(_p: ManagedPolicy): boolean {
  return false;
}

/** Whether the row should render as "on" — simply its enabled bit. */
export function rowOn(_p: ManagedPolicy, enabled: boolean): boolean {
  return enabled;
}

export function isMarketSource(p: ManagedPolicy | PolicySet): boolean {
  return p.source === "market";
}

/** Stable, deterministic mtime label keyed off `updatedAtMs`. Used by
 *  the table's last-edited column when the real timestamp is too noisy
 *  to render as an absolute date. */
export function mtimeLabel(updatedAtMs: number, draft: boolean): string {
  const ms = Date.now() - updatedAtMs;
  if (draft && ms < 60 * 60_000) {
    const m = Math.max(1, Math.floor(ms / 60_000));
    return `${m}분 전`;
  }
  if (ms < 60 * 60_000) {
    const m = Math.max(1, Math.floor(ms / 60_000));
    return `${m}분 전`;
  }
  if (ms < 24 * 60 * 60_000) {
    const h = Math.floor(ms / (60 * 60_000));
    return `${h}시간 전`;
  }
  if (ms < 7 * 24 * 60 * 60_000) {
    const d = Math.floor(ms / (24 * 60 * 60_000));
    return `${d}일 전`;
  }
  const w = Math.floor(ms / (7 * 24 * 60 * 60_000));
  return `${w}주 전`;
}

/** Bucket the package list by "scope" — all / loose / per-package. */
export type ListScope =
  | { type: "all" }
  | { type: "loose" }
  | { type: "pkg"; id: string };

/** Filter the input policies by selection scope. `policiesBySet` is a
 *  map from setId → policy ids that belong to that set. */
export function filterByScope(
  policies: ManagedPolicy[],
  setMembership: Map<string, Set<string>>,
  scope: ListScope,
): ManagedPolicy[] {
  if (scope.type === "all") return policies;
  if (scope.type === "loose") {
    const claimed = new Set<string>();
    for (const ids of setMembership.values()) {
      for (const id of ids) claimed.add(id);
    }
    return policies.filter((p) => !claimed.has(p.id));
  }
  const ids = setMembership.get(scope.id) ?? new Set<string>();
  return policies.filter((p) => ids.has(p.id));
}

/** Build a (setId → memberIdSet) map for O(1) scope filtering. */
export function buildSetMembership(sets: PolicySet[]): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  for (const s of sets) {
    out.set(s.id, new Set(s.memberIds));
  }
  return out;
}
