import { describe, it, expect } from "vitest";
import { makeHole, replaceNode } from "../params";
import type { Expr, PolicyIR } from "../ir";

const lit = (value: number): Expr => ({ kind: "lit", litType: "long", value });
const policyWith = (body: Expr): PolicyIR => ({
  kind: "policy",
  effect: "permit",
  annotations: [],
  scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
  conditions: [{ kind: "when", body }],
});

describe("makeHole", () => {
  it("infers expected + captures default from a lit", () => {
    const h = makeHole(lit(10000), { name: "maxUsd", label: "Max swap (USD)", constraints: { min: 0 } });
    expect(h).toMatchObject({
      kind: "hole",
      name: "maxUsd",
      expected: "lit:long",
      default: lit(10000),
      label: "Max swap (USD)",
      constraints: { min: 0 },
    });
  });
  it("infers expected for litEntity and set", () => {
    expect(makeHole({ kind: "litEntity", entity: { type: "User", id: "a" } }, { name: "u" }).expected).toBe("litEntity");
    expect(makeHole({ kind: "set", elements: [] }, { name: "s" }).expected).toBe("set");
  });
  it("throws on a non-value node", () => {
    expect(() => makeHole({ kind: "var", name: "context" }, { name: "x" })).toThrow(/lit\/litEntity\/set/);
  });
});

describe("replaceNode", () => {
  it("replaces the first matching node, leaving others", () => {
    const ir = policyWith({ kind: "binary", op: ">", left: lit(1), right: lit(1) });
    const hole = makeHole(lit(1), { name: "p" });
    const out = replaceNode(ir, (e) => e.kind === "lit" && e.value === 1, hole);
    const body: any = out.conditions[0].body;
    expect(body.left).toEqual(hole); // first match replaced
    expect(body.right).toEqual(lit(1)); // second untouched
  });
});
