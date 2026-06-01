/**
 * Tree evaluator — the JS-side mirror of the wasm Cedar runtime.
 *
 * Ported from `front/scopeball-v3/editor-v7-data.js` (`v7Evaluate`,
 * `v7EvalNode`, `v7EvalPred`, `v7Apply`, `v7ReadPath`, `v7ResolveVal`).
 *
 * Why keep a JS evaluator when wasm is the source of truth?
 *   - Live in-editor verdict preview without round-tripping through
 *     wasm on every keystroke.
 *   - Per-node truth values for the "failed guards" panel — wasm only
 *     hands back the policy-level allow/deny.
 *   - Unit tests can pin the V7 golden expectations (3 fixtures) without
 *     wasm in the harness.
 *
 * Deny-by-default semantics:
 *   - Root AND of guards true → ALLOW.
 *   - Anything else (false, missing root, missing hat) → DENY.
 *
 * `absence` handling on `enrichment.*` predicates mirrors the v3
 * behavior: undefined LHS resolves to `false` (treatAsFalse) or `true`
 * (treatAsTrue / skip). Defaults to `treatAsFalse`.
 */

import type { Doc, NodeId, PredicateNode } from "./types";

export type Truth = Record<NodeId, boolean>;

export interface FailedGuard {
  id: NodeId;
  guardId: string;
  label: string;
}

export interface EvalResult {
  verdict: "ALLOW" | "DENY";
  permitMatch: boolean;
  truth: Truth;
  failed: FailedGuard[];
}

/** Tx shape kept loose — host enrichers / sim contexts pass arbitrary
 *  records. Path lookup walks `.`-separated keys. */
export type TxLike = Record<string, unknown>;

function readPath(tx: TxLike, path: string): unknown {
  if (!path) return undefined;
  const clean = path.replace(/^@/, "");
  const parts = clean.split(".");
  let cur: unknown = tx[parts[0]];
  for (let i = 1; i < parts.length; i += 1) {
    if (cur == null || typeof cur !== "object") return undefined;
    cur = (cur as Record<string, unknown>)[parts[i]];
  }
  return cur;
}

function resolveValue(tx: TxLike, n: PredicateNode): unknown {
  const v = n.value;
  if (!v) return undefined;
  if (v.kind === "ref" || (typeof v.text === "string" && v.text.startsWith("@"))) {
    return readPath(tx, v.text);
  }
  if (v.kind === "num") return Number(v.text);
  if (v.kind === "bool") return v.text === "true";
  return v.text;
}

function applyOp(op: string, l: unknown, r: unknown): boolean {
  switch (op) {
    case "eq":
      return l === r;
    case "neq":
      return l !== r;
    case "lt":
      return Number(l) < Number(r);
    case "lte":
      return Number(l) <= Number(r);
    case "gt":
      return Number(l) > Number(r);
    case "gte":
      return Number(l) >= Number(r);
    case "isTrue":
      return l === true;
    case "isFalse":
      return l === false;
    case "in":
      return Array.isArray(r) && r.indexOf(l) >= 0;
    case "notIn":
      return Array.isArray(r) && r.indexOf(l) < 0;
    case "startsWith":
      return typeof l === "string" && l.startsWith(String(r));
    case "contains":
      if (Array.isArray(l)) return l.includes(r);
      return typeof l === "string" && l.includes(String(r));
    case "isEmpty":
      if (Array.isArray(l)) return l.length === 0;
      return l == null || l === "";
    default:
      return false;
  }
}

function evalPredicate(n: PredicateNode, tx: TxLike): boolean {
  const lhs = readPath(tx, n.param);
  const optional = n.param.startsWith("enrichment.");
  if ((lhs === undefined || lhs === null) && optional) {
    const a = n.absence ?? "treatAsFalse";
    return a === "treatAsTrue";
  }
  const noRhs = n.op === "isTrue" || n.op === "isFalse" || n.op === "isEmpty";
  const rhs = noRhs ? undefined : resolveValue(tx, n);
  return applyOp(n.op, lhs, rhs);
}

function evalNode(doc: Doc, id: NodeId | null, tx: TxLike, truth: Truth): boolean {
  if (!id) return true;
  const n = doc.nodes[id];
  if (!n) return true;

  let res: boolean;
  if (n.type === "predicate") {
    res = evalPredicate(n, tx);
  } else if (n.type === "hat") {
    res = evalNode(doc, n.childId, tx, truth);
  } else {
    const kids = n.childIds.filter((c) => {
      const k = doc.nodes[c];
      return k && k.type !== "hat" && k.enabled !== false;
    });
    if (kids.length === 0) {
      res = n.op !== "OR";
    } else if (n.op === "NOT") {
      res = !evalNode(doc, kids[0], tx, truth);
    } else {
      // No short-circuit — fully populate `truth` so the UI can show
      // every guard's verdict, not just the first failure.
      const rs = kids.map((c) => evalNode(doc, c, tx, truth));
      res = n.op === "OR" ? rs.some(Boolean) : rs.every(Boolean);
    }
  }
  truth[id] = res;
  return res;
}

export function evalDoc(doc: Doc, tx: TxLike): EvalResult {
  const truth: Truth = {};
  const ok = evalNode(doc, doc.hatId, tx, truth);
  const failed: FailedGuard[] = [];
  const root = doc.nodes[doc.rootId];
  if (root && root.type === "logic") {
    for (const cid of root.childIds) {
      const c = doc.nodes[cid];
      if (!c || c.type === "hat" || c.enabled === false) continue;
      if (truth[cid] === false) {
        const guardId = "guardId" in c && c.guardId ? c.guardId : cid;
        const label =
          "label" in c && c.label
            ? c.label
            : c.type === "predicate"
              ? c.param
              : cid;
        failed.push({ id: cid, guardId, label });
      }
    }
  }
  return { verdict: ok ? "ALLOW" : "DENY", permitMatch: ok, truth, failed };
}
