import { describe, it, expect } from "vitest";
import { makeHole, replaceNode, extractParams, fillParams } from "../params";
import { blocksToEst } from "../blocksToEst";
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

const tmpl = () =>
  policyWith({
    kind: "binary",
    op: ">",
    left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "amt" },
    right: makeHole(lit(10000), { name: "maxUsd", constraints: { min: 0, max: 1000000 } }),
  });

describe("fillParams", () => {
  it("fills a supplied value", () => {
    const r = fillParams(tmpl(), { maxUsd: 5000 });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.policy.conditions[0].body as any).right).toEqual(lit(5000));
      expect(() => blocksToEst(r.policy)).not.toThrow(); // hole-free
    }
  });
  it("errors on a missing required param", () => {
    const r = fillParams(tmpl(), {});
    expect(r).toMatchObject({ ok: false, errors: [{ name: "maxUsd", reason: "missing" }] });
  });
  it("falls back to default for an optional unsupplied param", () => {
    const ir = policyWith(makeHole(lit(42), { name: "p", optional: true }));
    const r = fillParams(ir, {});
    expect(r.ok).toBe(true);
    if (r.ok) expect(r.policy.conditions[0].body).toEqual(lit(42));
  });
  it("reports type, range, enum, and unknown errors together", () => {
    const ir = policyWith({
      kind: "binary",
      op: "&&",
      left: makeHole(lit(1), { name: "n", constraints: { max: 10 } }),
      right: makeHole({ kind: "lit", litType: "string", value: "x" }, { name: "s", constraints: { enum: ["a", "b"] } }),
    });
    const r = fillParams(ir, { n: 999, s: "z", bogus: 1 });
    expect(r.ok).toBe(false);
    if (!r.ok) {
      const reasons = Object.fromEntries(r.errors.map((e) => [e.name, e.reason]));
      expect(reasons).toEqual({ n: "range", s: "enum", bogus: "unknown" });
    }
  });
  it("builds a litEntity and a typed set", () => {
    const ir = policyWith({
      kind: "binary",
      op: "&&",
      left: makeHole({ kind: "litEntity", entity: { type: "User", id: "x" } }, { name: "u" }),
      right: makeHole({ kind: "set", elements: [{ kind: "lit", litType: "string", value: "a" }] }, { name: "allow" }),
    });
    const r = fillParams(ir, { u: { type: "User", id: "bob" }, allow: ["0xAAA", "0xBBB"] });
    expect(r.ok).toBe(true);
    if (r.ok) {
      const body: any = r.policy.conditions[0].body;
      expect(body.left).toEqual({ kind: "litEntity", entity: { type: "User", id: "bob" } });
      expect(body.right).toEqual({
        kind: "set",
        elements: [
          { kind: "lit", litType: "string", value: "0xAAA" },
          { kind: "lit", litType: "string", value: "0xBBB" },
        ],
      });
    }
  });
});
