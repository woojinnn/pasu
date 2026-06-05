import { describe, it, expect } from "vitest";
import type { PolicyIR, Expr } from "../../blocks/ir";
import { enumeratePaths } from "../path";
import { buildProbes } from "../probes";
import { blame, type TruthMap } from "../blame";

const v = (n: any): Expr => ({ kind: "var", name: n });
const attr = (of: Expr, a: string): Expr => ({ kind: "attr", of, attr: a });
const long = (n: number): Expr => ({ kind: "lit", litType: "long", value: n });
const str = (s: string): Expr => ({ kind: "lit", litType: "string", value: s });

// Covers: &&, ||, !, comparison, has, like, is, ext, set, record, if — and the
// riskiest label-drift surfaces: an INDEXED path step into a `record` carrying a
// boolean-valued pair (so buildProbes emits a `…pairs[0]` probe), plus a `like`
// node. Both are spliced into the boolean tree without removing existing nodes.
const body: Expr = {
  kind: "binary", op: "&&",
  left: { kind: "binary", op: "||",
    left: { kind: "binary", op: ">", left: attr(v("context"), "slippageBp"), right: long(100) },
    right: { kind: "unary", op: "!", operand: { kind: "has", of: v("context"), attr: "recipient" } } },
  right: { kind: "if",
    cond: { kind: "is", of: v("resource"), entityType: "Protocol" },
    then: { kind: "ext", fn: "greaterThan", args: [attr(v("context"), "amount"), long(1)] },
    else: { kind: "binary", op: "||",
      left: { kind: "binary", op: "||",
        // a `like` node in boolean position
        left: { kind: "like", of: attr(v("context"), "recipient"),
          pattern: [{ Literal: "0x" }, "Wildcard"] },
        // a comparison reaching INTO a record's boolean-valued pair value —
        // the pair `{ flag: context.a > context.b }` is itself a comparison
        // (isBooleanNode), so buildProbes emits a `…else.left.right.pairs[0]` probe.
        right: { kind: "binary", op: "contains",
          left: { kind: "record", pairs: [
            { key: "flag", value: { kind: "binary", op: ">", left: attr(v("context"), "a"), right: attr(v("context"), "b") } },
          ] },
          right: str("flag") } },
      right: { kind: "binary", op: "contains",
        left: { kind: "set", elements: [str("a"), str("b")] }, right: str("a") } } },
};
const policy: PolicyIR = {
  kind: "policy", effect: "forbid", annotations: [],
  scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
  conditions: [{ kind: "when", body }],
};

// Guards that the three path producers — enumeratePaths (editor map), buildProbes
// (probe ids) and blame (culprit paths) — agree on every label, including the
// INDEXED steps (`set` elements / `record` pairs / `ext` args) that are the
// likeliest to drift. Covered node kinds: &&, ||, !, comparison, has, like, is,
// ext, set, record (with a boolean-valued pair → `…pairs[0]` probe), and if.
describe("path consistency across producers", () => {
  const allPaths = new Set(enumeratePaths(policy).map((e) => e.path));

  it("every probe id is a real enumeratePaths path", () => {
    for (const p of buildProbes(policy).probes) expect(allPaths.has(p.id)).toBe(true);
  });

  it("emits an INDEXED (pairs[i]) probe id — the riskiest label-drift surface", () => {
    const ids = buildProbes(policy).probes.map((p) => p.id);
    // the boolean-valued record pair `{ flag: context.a > context.b }` is probed
    expect(ids.some((id) => /\.pairs\[0\]$/.test(id))).toBe(true);
  });

  it("every blame path is a real enumeratePaths path", () => {
    // mark all boolean probes true (worst case for blame surface)
    const truth: TruthMap = {};
    for (const p of buildProbes(policy).probes) truth[p.id] = true;
    for (const path of blame(policy, truth)) expect(allPaths.has(path)).toBe(true);
  });
});
