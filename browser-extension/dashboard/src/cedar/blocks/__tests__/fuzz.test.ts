import { describe, it, expect } from "vitest";
import { blocksToEst } from "../blocksToEst";
import { estToBlocks } from "../estToBlocks";
import type { Expr, PolicyIR } from "../ir";

// Deterministic PRNG so any failure reproduces from its seed.
function lcg(seed: number): () => number {
  let x = seed >>> 0;
  return () => {
    x = (Math.imul(x, 1103515245) + 12345) >>> 0;
    return x / 0xffffffff;
  };
}

function randExpr(depth: number, rng: () => number): Expr {
  if (depth <= 0) {
    const leaf = Math.floor(rng() * 3);
    if (leaf === 0) return { kind: "lit", litType: "long", value: Math.floor(rng() * 1000) };
    if (leaf === 1) return { kind: "attr", of: { kind: "var", name: "context" }, attr: "f" + Math.floor(rng() * 5) };
    return { kind: "var", name: "context" };
  }
  switch (Math.floor(rng() * 6)) {
    case 0:
      return { kind: "binary", op: "&&", left: randExpr(depth - 1, rng), right: randExpr(depth - 1, rng) };
    case 1:
      return { kind: "binary", op: ">", left: randExpr(depth - 1, rng), right: randExpr(depth - 1, rng) };
    case 2:
      return { kind: "unary", op: "!", operand: randExpr(depth - 1, rng) };
    case 3:
      return { kind: "set", elements: [randExpr(depth - 1, rng), randExpr(depth - 1, rng)] };
    case 4:
      return { kind: "if", cond: randExpr(depth - 1, rng), then: randExpr(depth - 1, rng), else: randExpr(depth - 1, rng) };
    default:
      return { kind: "has", of: { kind: "var", name: "context" }, attr: "g" + Math.floor(rng() * 3) };
  }
}

function policyOf(body: Expr): PolicyIR {
  return {
    kind: "policy",
    effect: "permit",
    annotations: [],
    scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
    conditions: [{ kind: "when", body }],
  };
}

function rawCount(node: any): number {
  let n = 0;
  const walk = (x: any) => {
    if (x && typeof x === "object") {
      if (x.kind === "raw") n++;
      for (const v of Object.values(x)) {
        if (Array.isArray(v)) v.forEach(walk);
        else if (v && typeof v === "object") walk(v);
      }
    }
  };
  walk(node);
  return n;
}

describe("property: random policies round-trip byte-exact with zero raw (#2 & #4)", () => {
  it("200 random policies (seeded)", () => {
    for (let i = 1; i <= 200; i++) {
      const est = blocksToEst(policyOf(randExpr(5, lcg(i))));
      const ir2 = estToBlocks(est, null);
      expect(blocksToEst(ir2), `seed ${i}: not byte-exact`).toEqual(est);
      expect(rawCount(ir2), `seed ${i}: produced raw`).toBe(0);
    }
  });
});

describe("robustness / failure modes", () => {
  it("an unknown EST node falls back to raw and still round-trips", () => {
    const est: any = {
      effect: "permit",
      principal: { op: "All" },
      action: { op: "All" },
      resource: { op: "All" },
      conditions: [{ kind: "when", body: { mysteryOp: { left: { Var: "context" } } } }],
    };
    const ir: any = estToBlocks(est, null);
    expect(ir.conditions[0].body.kind).toBe("raw");
    expect(blocksToEst(ir)).toEqual(est);
  });

  it("blocksToEst throws a typed error on an unfilled hole", () => {
    const ir = policyOf({ kind: "hole", expected: "expr", name: "maxUsd" });
    expect(() => blocksToEst(ir)).toThrow(/hole/);
  });

  it("estToBlocks throws cleanly (no stack overflow) on pathologically deep nesting", () => {
    let body: any = { Value: true };
    for (let i = 0; i < 500; i++) body = { "!": { arg: body } };
    const est: any = {
      effect: "permit",
      principal: { op: "All" },
      action: { op: "All" },
      resource: { op: "All" },
      conditions: [{ kind: "when", body }],
    };
    expect(() => estToBlocks(est, null)).toThrow(/MAX_DEPTH/);
  });
});
