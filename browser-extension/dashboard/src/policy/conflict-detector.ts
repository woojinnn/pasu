import type { ManagedPolicy } from "@scopeball/sdk";

// Quick static "potential conflict" hints — no install/evaluate cycle.
//
// We extract a few shallow signals from each policy's Cedar text:
//   - `action == Action::"<name>"` head action
//   - `@severity("deny"|"warn")` annotation
//   - A normalized form of the `when { ... }` body (whitespace squashed,
//     `\s*&&\s*` split into a sorted set of conjunct strings)
//
// Two policies are flagged as a "potential conflict" if they target the
// same action AND share at least one conjunct AND differ in severity, OR
// have identical conjunct sets (redundant duplicate).
//
// This is intentionally narrow — we don't try to be the engine. Anything
// the engine accepts but this misses is *not* a false negative for the
// dashboard's purposes: the live evaluator is the source of truth, this
// is a hint to encourage user review.

export interface PolicyFacts {
  id: string;
  action?: string;
  severity?: "deny" | "warn";
  conjuncts: string[];
}

export interface ConflictHint {
  kind: "severity-mismatch" | "duplicate" | "subset";
  otherId: string;
  /** Conjuncts shared between this policy and `otherId`. */
  sharedConjuncts: string[];
}

const ACTION_RE = /action\s*==\s*Action::"([^"]+)"/;
const SEVERITY_RE = /@severity\("(deny|warn)"\)/;
const WHEN_RE = /when\s*\{([\s\S]*?)\}\s*;?\s*$/;

export function extractFacts(policy: ManagedPolicy): PolicyFacts {
  const text = policy.text;
  const actionMatch = ACTION_RE.exec(text);
  const severityMatch = SEVERITY_RE.exec(text);
  const whenMatch = WHEN_RE.exec(text);
  return {
    id: policy.id,
    action: actionMatch?.[1],
    severity: (severityMatch?.[1] as "deny" | "warn" | undefined),
    conjuncts: whenMatch ? normalizeConjuncts(whenMatch[1]) : [],
  };
}

function normalizeConjuncts(body: string): string[] {
  return body
    .split(/\s*&&\s*/)
    .map((c) => c.trim().replace(/\s+/g, " "))
    .filter((c) => c.length > 0)
    // Sort so set comparison is order-independent — the engine emits
    // `context has X` guards in alphabetical order already, but raw Code
    // edits won't.
    .sort();
}

export function detectConflicts(
  policies: readonly ManagedPolicy[],
): Map<string, ConflictHint[]> {
  const facts = policies.map(extractFacts);
  const out = new Map<string, ConflictHint[]>();
  const push = (id: string, hint: ConflictHint) => {
    const list = out.get(id) ?? [];
    list.push(hint);
    out.set(id, list);
  };

  for (let i = 0; i < facts.length; i++) {
    for (let j = i + 1; j < facts.length; j++) {
      const a = facts[i];
      const b = facts[j];
      // Different actions = independent rule space. Skip.
      if (!a.action || !b.action || a.action !== b.action) continue;

      const setA = new Set(a.conjuncts);
      const shared: string[] = [];
      for (const c of b.conjuncts) if (setA.has(c)) shared.push(c);
      if (shared.length === 0) continue;

      const sameSize =
        a.conjuncts.length === b.conjuncts.length &&
        a.conjuncts.length === shared.length;

      if (sameSize) {
        // Exact same predicate set on the same action.
        const kind: ConflictHint["kind"] =
          a.severity !== b.severity ? "severity-mismatch" : "duplicate";
        push(a.id, { kind, otherId: b.id, sharedConjuncts: shared });
        push(b.id, { kind, otherId: a.id, sharedConjuncts: shared });
      } else if (a.severity && b.severity && a.severity !== b.severity) {
        // Partial overlap with opposing severities — worth surfacing.
        push(a.id, {
          kind: "severity-mismatch",
          otherId: b.id,
          sharedConjuncts: shared,
        });
        push(b.id, {
          kind: "severity-mismatch",
          otherId: a.id,
          sharedConjuncts: shared,
        });
      } else {
        // Same severity, overlapping predicates — one's a subset of the
        // other or they share common ground. Flag as subset so the user
        // sees the relationship.
        push(a.id, {
          kind: "subset",
          otherId: b.id,
          sharedConjuncts: shared,
        });
        push(b.id, {
          kind: "subset",
          otherId: a.id,
          sharedConjuncts: shared,
        });
      }
    }
  }
  return out;
}

export function describeKind(kind: ConflictHint["kind"]): string {
  switch (kind) {
    case "severity-mismatch":
      return "severity 충돌";
    case "duplicate":
      return "중복 정책";
    case "subset":
      return "조건 부분 중복";
  }
}
