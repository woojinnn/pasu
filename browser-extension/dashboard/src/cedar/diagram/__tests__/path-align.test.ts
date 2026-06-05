import { describe, expect, it } from "vitest";

import type { Expr, PolicyIR } from "../../blocks/ir";
import { enumeratePaths } from "../../diagnosis/path";
import { policyDiagramPaths } from "../PolicyDiagram";

/** `context.custom.<name> == <n>` — a simple boolean leaf. */
const cmp = (name: string, n: number): Expr => ({
  kind: "binary",
  op: "==",
  left: {
    kind: "attr",
    of: { kind: "attr", of: { kind: "var", name: "context" }, attr: "custom" },
    attr: name,
  },
  right: { kind: "lit", litType: "long", value: n },
});

/** forbid Swap when { A && (B || C) && !D } — exercises a flattened AND chain,
 *  a nested OR, and a NOT, the cases where a hand-built path scheme would drift. */
const policy: PolicyIR = {
  kind: "policy",
  effect: "forbid",
  annotations: [{ name: "id", value: "t" }],
  scope: {
    principal: { kind: "scopeAll" },
    action: { kind: "scopeEq", entity: { type: "Action", id: "Swap" } },
    resource: { kind: "scopeAll" },
  },
  conditions: [
    {
      kind: "when",
      body: {
        kind: "binary",
        op: "&&",
        left: {
          kind: "binary",
          op: "&&",
          left: cmp("a", 1),
          right: { kind: "binary", op: "||", left: cmp("b", 2), right: cmp("c", 3) },
        },
        right: { kind: "unary", op: "!", operand: cmp("d", 4) },
      },
    },
  ],
};

describe("PolicyDiagram path alignment", () => {
  it("every diagram node path is a canonical diagnosis path (no drift)", () => {
    const canonical = new Set(enumeratePaths(policy).map((p) => p.path));
    const diagramPaths = policyDiagramPaths(policy);

    expect(diagramPaths.length).toBeGreaterThan(0);
    for (const p of diagramPaths) {
      expect(canonical.has(p)).toBe(true);
    }
  });

  it("flattened AND operands keep their true nested paths", () => {
    const paths = new Set(policyDiagramPaths(policy));
    // The leaf `a` sits two ANDs deep in Cedar's nested encoding.
    expect(paths.has("c0.body.left.left")).toBe(true);
    // `b`/`c` are under the nested OR.
    expect(paths.has("c0.body.left.right.left")).toBe(true);
    expect(paths.has("c0.body.left.right.right")).toBe(true);
    // `!D`'s operand.
    expect(paths.has("c0.body.right.operand")).toBe(true);
  });
});
