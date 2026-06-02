import { describe, it, expect } from "vitest";
import { makeHole, replaceNode, extractParams } from "../params";
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

describe("extractParams", () => {
  it("collects holes in document order with their specs", () => {
    const ir = policyWith({
      kind: "binary",
      op: "&&",
      left: { kind: "binary", op: ">", left: lit(0), right: makeHole(lit(10000), { name: "maxUsd", label: "Max" }) },
      right: { kind: "binary", op: "<", left: lit(0), right: makeHole(lit(60), { name: "minPct", optional: true }) },
    });
    const specs = extractParams(ir);
    expect(specs.map((s) => s.name)).toEqual(["maxUsd", "minPct"]);
    expect(specs[0]).toMatchObject({ name: "maxUsd", expected: "lit:long", label: "Max", default: lit(10000) });
    expect(specs[1].optional).toBe(true);
  });
  it("throws on duplicate param names", () => {
    const ir = policyWith({
      kind: "binary",
      op: "&&",
      left: makeHole(lit(1), { name: "dup" }),
      right: makeHole(lit(2), { name: "dup" }),
    });
    expect(() => extractParams(ir)).toThrow(/duplicate/);
  });
});
