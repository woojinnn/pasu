/**
 * Tree → Cedar text serializer.
 *
 * Ported from `front/scopeball-v3/editor-v7-data.js` (`v7ToCedar` +
 * `v7NodeCedar` + `v7PredCedar`). The output is a single `permit`
 * statement: `permit(principal, action == Action::"X", resource) when { … };`
 *
 * Two output shapes:
 *   - `serializeDoc(doc)` returns the flat text (what we feed to the
 *     wasm `testPolicyLocal` checker).
 *   - `serializeDocLines(doc)` returns the line-by-line breakdown used
 *     by the future code panel for syntax-highlight + guard hover-link.
 *
 * Notes on Cedar semantics:
 *   - `enrichment.*` predicates are wrapped `(enrichment has fieldName && expr)`
 *     so a missing enrichment doesn't trip an evaluator error — matches
 *     v7's `treatAsFalse` default.
 *   - `@meta.from` style refs lose the `@` and become bare paths.
 *   - Disabled guards (`enabled === false`) are skipped, with a comment
 *     line noting the count.
 *   - Floating drafts are not emitted; a trailing comment notes them.
 */

import type { Doc, NodeId, PredicateNode } from "./types";

type CedarLineKind = "cmt" | "kw" | "arg" | "punct" | "guard";
export interface CedarLine {
  n: number;
  text: string;
  kind: CedarLineKind;
  guardId?: string;
}

const BINARY_SYMBOLS: Record<string, string> = {
  eq: "==",
  neq: "!=",
  lt: "<",
  lte: "<=",
  gt: ">",
  gte: ">=",
};

function predicateToCedar(n: PredicateNode): string {
  let expr: string;
  if (n.op === "isTrue") {
    expr = `${n.param} == true`;
  } else if (n.op === "isFalse") {
    expr = `${n.param} == false`;
  } else {
    const sym = BINARY_SYMBOLS[n.op] ?? n.op;
    const v = n.value;
    let rhs: string;
    if (!v) {
      rhs = '""';
    } else if (v.kind === "ref" || (typeof v.text === "string" && v.text.startsWith("@"))) {
      rhs = v.text.replace(/^@/, "");
    } else if (v.kind === "num") {
      rhs = v.text;
    } else if (v.kind === "bool") {
      rhs = v.text;
    } else {
      rhs = `"${v.text}"`;
    }
    expr = `${n.param} ${sym} ${rhs}`;
  }

  // Wrap every `enrichment.*` access in `has`-guard so a missing host
  // enrichment doesn't trip Cedar's "field not found" error — matches
  // the evaluator's `absence: treatAsFalse` default.
  if (n.param.startsWith("enrichment.")) {
    const [head, ...rest] = n.param.split(".");
    return `(${head} has ${rest.join(".")} && ${expr})`;
  }
  return expr;
}

function nodeToCedar(doc: Doc, id: NodeId): string {
  const n = doc.nodes[id];
  if (!n) return "true";
  if (n.type === "predicate") return predicateToCedar(n);
  if (n.type === "hat") return n.childId ? nodeToCedar(doc, n.childId) : "true";

  const kids = n.childIds.filter((c) => {
    const k = doc.nodes[c];
    return k && k.type !== "hat" && k.enabled !== false;
  });

  if (n.op === "NOT") {
    return `!(${kids.length ? nodeToCedar(doc, kids[0]) : "false"})`;
  }
  if (kids.length === 0) return n.op === "OR" ? "false" : "true";

  const join = n.op === "AND" ? " && " : " || ";
  const parts = kids.map((c) => {
    const t = nodeToCedar(doc, c);
    const ck = doc.nodes[c];
    if (ck && ck.type === "logic" && ck.op !== "NOT" && ck.childIds.length > 1) {
      return `(${t})`;
    }
    return t;
  });
  return parts.join(join);
}

/**
 * Convert an action id like `"Amm::Swap"` into the Cedar entity ref the
 * extension's wasm cedar engine expects: `Amm::Action::"Swap"`. Single-
 * segment ids (no `::`) fall back to the bare `Action::"X"` form for
 * forward compatibility, but real schema action ids always carry their
 * namespace.
 */
function actionToCedarId(action: string): string {
  const parts = action.split("::");
  if (parts.length >= 2) {
    const id = parts.pop()!;
    const ns = parts.join("::");
    return `${ns}::Action::"${id}"`;
  }
  return `Action::"${action}"`;
}

export function serializeDocLines(doc: Doc): CedarLine[] {
  const lines: CedarLine[] = [];
  let ln = 0;
  const push = (text: string, kind: CedarLineKind, guardId?: string) => {
    ln += 1;
    lines.push(guardId ? { n: ln, text, kind, guardId } : { n: ln, text, kind });
  };

  const policyId = (doc.policyName || "policy").replace(/\s+/g, "_");
  const hat = doc.nodes[doc.hatId];
  const effect = hat?.type === "hat" ? hat.effect : "permit";
  const keyword = effect === "deny" ? "forbid" : "permit";

  push(`@id("${policyId}")`, "cmt");
  // Type narrowing (`principal is Wallet`, `resource is Protocol`) is
  // required by the extension's cedar wasm engine for strict schema
  // validation. Without it the policy fails to install — the engine
  // can't bind the action's `appliesTo` constraint.
  push(`${keyword} (`, "kw");
  push("  principal is Wallet,", "arg");
  push(`  action == ${actionToCedarId(doc.action || "Amm::Swap")},`, "arg");
  push("  resource is Protocol", "arg");
  push(")", "punct");
  push("when {", "kw");

  const root = doc.nodes[doc.rootId];
  if (root?.type === "logic") {
    const guards = root.childIds.filter((c) => {
      const k = doc.nodes[c];
      return k && k.type !== "hat" && k.enabled !== false;
    });

    if (guards.length === 0) {
      push("  true  // (no safety conditions)", "cmt");
    } else if (root.op === "NOT") {
      // NOT-root: render as `!(children-joined-by-AND)` on one logical
      // line. Per-guard line breakdown loses meaning under negation.
      const inner = guards.map((c) => nodeToCedar(doc, c)).join(" && ");
      push(`  !(${inner})`, "guard");
    } else {
      // AND or OR root — render guards as one Cedar expression per line,
      // joined by the root operator. `&&` for AND, `||` for OR.
      const sep = root.op === "OR" ? "|| " : "&& ";
      guards.forEach((cid, i) => {
        const t = nodeToCedar(doc, cid);
        const ck = doc.nodes[cid];
        if (!ck) return;
        const label = "label" in ck && ck.label ? `  // ${ck.label}` : "";
        const prefix = i > 0 ? sep : "";
        const guardId = "guardId" in ck ? ck.guardId : undefined;
        push(`  ${prefix}${t}${label}`, "guard", guardId ?? cid);
      });
    }

    const disabled = root.childIds.filter((c) => {
      const k = doc.nodes[c];
      return k && k.type !== "hat" && k.enabled === false;
    }).length;
    if (disabled > 0) {
      push(`// ${disabled}개 가드 비활성 — 컴파일 제외`, "cmt");
    }
  } else {
    push("  true", "cmt");
  }

  push("};", "kw");

  if (doc.drafts.length > 0) {
    push(`// 미연결 ${doc.drafts.length}개 — 컴파일 제외`, "cmt");
  }
  return lines;
}

export function serializeDoc(doc: Doc): string {
  return serializeDocLines(doc)
    .map((l) => l.text)
    .join("\n");
}
