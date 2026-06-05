import { describe, it, expect } from "vitest";
import { makeHole, replaceNode, extractParams, fillParams } from "../params";
import { blocksToEst } from "../blocksToEst";
import { estToBlocks } from "../estToBlocks";
import realPolicies from "./fixtures/real-policies-est.json";
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

describe("integration", () => {
  const collectLits = (e: any, acc: any[] = []): any[] => {
    if (e && typeof e === "object") {
      if (e.kind === "lit" && e.litType === "long") acc.push(e);
      for (const v of Object.values(e)) {
        if (Array.isArray(v)) v.forEach((x) => collectLits(x, acc));
        else if (v && typeof v === "object") collectLits(v, acc);
      }
    }
    return acc;
  };

  it("parameterizes a real shipped policy's first long literal and round-trips", () => {
    const withLong = (realPolicies as { est: any }[])
      .map((c) => estToBlocks(c.est, null))
      .find((ir) => collectLits(ir).length > 0);
    expect(withLong, "expected at least one shipped policy with a long literal").toBeTruthy();

    const target = collectLits(withLong)[0];
    const tmpl = replaceNode(withLong!, (e) => e === target, makeHole(target, { name: "threshold", optional: true }));
    expect(extractParams(tmpl).map((s) => s.name)).toEqual(["threshold"]);

    const filled = fillParams(tmpl, { threshold: target.value + 1 });
    expect(filled.ok).toBe(true);
    if (filled.ok) expect(() => blocksToEst(filled.policy)).not.toThrow(); // hole-free + serializable
  });

  it("safety net: blocksToEst throws on a template with an unfilled hole", () => {
    const tmpl = policyWith(makeHole(lit(1), { name: "p" }));
    expect(() => blocksToEst(tmpl)).toThrow(/hole/);
  });

  it("property: fill-with-defaults reproduces the original policy", () => {
    for (let i = 1; i <= 500; i++) {
      const original = lit(i);
      const ir = policyWith({ kind: "binary", op: ">", left: { kind: "var", name: "context" }, right: original });
      const tmpl = replaceNode(ir, (e) => e === original, makeHole(original, { name: "p", optional: true }));
      const r = fillParams(tmpl, {});
      expect(r.ok).toBe(true);
      if (r.ok) expect(r.policy).toEqual(ir); // default fill === original
    }
  });
});

describe("property: parameterize → fill across all value kinds (1000 cases)", () => {
  const lcg = (seed: number) => {
    let x = (seed >>> 0) || 1;
    return () => {
      x = (Math.imul(x, 1103515245) + 12345) >>> 0;
      return x / 0xffffffff;
    };
  };
  const ri = (rng: () => number, n: number) => Math.floor(rng() * n);

  const randValue = (rng: () => number): Expr => {
    switch (ri(rng, 5)) {
      case 0: return { kind: "lit", litType: "long", value: ri(rng, 100000) };
      case 1: return { kind: "lit", litType: "string", value: "s" + ri(rng, 50) };
      case 2: return { kind: "lit", litType: "bool", value: rng() < 0.5 };
      case 3: return { kind: "litEntity", entity: { type: "T" + ri(rng, 5), id: "e" + ri(rng, 9) } };
      default:
        return { kind: "set", elements: Array.from({ length: ri(rng, 4) }, () => ({ kind: "lit", litType: "string", value: "x" + ri(rng, 9) })) };
    }
  };
  // The raw fill value that reproduces a given value node.
  const defaultFill = (v: Expr): any => {
    if (v.kind === "lit") return v.value;
    if (v.kind === "litEntity") return { type: v.entity.type, id: v.entity.id };
    if (v.kind === "set") return v.elements.map((e) => (e.kind === "lit" ? e.value : null));
    return null;
  };

  it("extract + required/optional + fill(default)/fill(new) all hold", () => {
    for (let i = 1; i <= 1000; i++) {
      const rng = lcg(i);
      const value = randValue(rng);
      const ir = policyWith({ kind: "binary", op: "==", left: { kind: "var", name: "context" }, right: value });
      const optional = i % 2 === 0;
      const tmpl = replaceNode(ir, (e) => e === value, makeHole(value, { name: "p", optional }));

      expect(extractParams(tmpl).map((s) => s.name)).toEqual(["p"]);

      const empty = fillParams(tmpl, {});
      if (optional) {
        expect(empty.ok, `seed ${i} optional empty`).toBe(true);
        if (empty.ok) expect(empty.policy).toEqual(ir);
      } else {
        expect(empty, `seed ${i} required empty`).toMatchObject({ ok: false, errors: [{ reason: "missing" }] });
      }

      const filled = fillParams(tmpl, { p: defaultFill(value) });
      expect(filled.ok, `seed ${i} fill default: ${JSON.stringify(filled)}`).toBe(true);
      if (filled.ok) expect(filled.policy).toEqual(ir); // fill-own-default reproduces original
    }
  });
});
