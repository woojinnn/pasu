import { describe, it, expect } from "vitest";
import { blocksToEst } from "../blocksToEst";
import { estToBlocks } from "../estToBlocks";
import type { ActionScope, EntityRef, Expr, LikePattern, PolicyIR, Scope } from "../ir";

// Deterministic PRNG so any failure reproduces from its seed.
function lcg(seed: number): () => number {
  let x = seed >>> 0;
  return () => {
    x = (Math.imul(x, 1103515245) + 12345) >>> 0;
    return x / 0xffffffff;
  };
}
const ri = (rng: () => number, n: number) => Math.floor(rng() * n);
const pick = <T>(rng: () => number, arr: readonly T[]): T => arr[ri(rng, arr.length)];

const BINARY_OPS = [
  "==", "!=", "<", "<=", ">", ">=", "&&", "||", "+", "-", "*",
  "in", "contains", "containsAll", "containsAny", "getTag", "hasTag",
] as const;
const UNARY_OPS = ["!", "neg", "isEmpty"] as const;
const VARS = ["principal", "action", "resource", "context"] as const;
// Extension fn names that do NOT collide with structural keys or binary/unary ops.
const EXT_FNS = ["ip", "decimal", "lessThan", "greaterThan", "isInRange"] as const;

function randEntity(rng: () => number): EntityRef {
  return { type: "T" + ri(rng, 5), id: "e" + ri(rng, 9) };
}

function randPattern(rng: () => number): LikePattern {
  return Array.from({ length: ri(rng, 5) }, () =>
    rng() < 0.4 ? "Wildcard" : { Literal: String.fromCharCode(97 + ri(rng, 26)) },
  );
}

function randLeaf(rng: () => number): Expr {
  switch (ri(rng, 5)) {
    case 0: return { kind: "lit", litType: "long", value: ri(rng, 100000) };
    case 1: return { kind: "lit", litType: "string", value: "s" + ri(rng, 50) };
    case 2: return { kind: "lit", litType: "bool", value: rng() < 0.5 };
    case 3: return { kind: "var", name: pick(rng, VARS) };
    default: return { kind: "litEntity", entity: randEntity(rng) };
  }
}

function randExpr(depth: number, rng: () => number): Expr {
  if (depth <= 0) return randLeaf(rng);
  const d = depth - 1;
  switch (ri(rng, 12)) {
    case 0: return { kind: "binary", op: pick(rng, BINARY_OPS), left: randExpr(d, rng), right: randExpr(d, rng) };
    case 1: return { kind: "unary", op: pick(rng, UNARY_OPS), operand: randExpr(d, rng) };
    case 2: return { kind: "attr", of: randExpr(d, rng), attr: "a" + ri(rng, 6) };
    case 3: return { kind: "has", of: randExpr(d, rng), attr: "h" + ri(rng, 6) };
    case 4: return { kind: "like", of: randExpr(d, rng), pattern: randPattern(rng) };
    case 5:
      return rng() < 0.5
        ? { kind: "is", of: randExpr(d, rng), entityType: "T" + ri(rng, 5), in: randExpr(d, rng) }
        : { kind: "is", of: randExpr(d, rng), entityType: "T" + ri(rng, 5) };
    case 6: return { kind: "if", cond: randExpr(d, rng), then: randExpr(d, rng), else: randExpr(d, rng) };
    case 7: return { kind: "set", elements: Array.from({ length: ri(rng, 3) }, () => randExpr(d, rng)) };
    case 8:
      return {
        kind: "record",
        pairs: Array.from({ length: ri(rng, 3) }, (_, i) => ({ key: "k" + i, value: randExpr(d, rng) })),
      };
    case 9:
      return { kind: "ext", fn: pick(rng, EXT_FNS), args: Array.from({ length: 1 + ri(rng, 2) }, () => randExpr(d, rng)) };
    default: return randLeaf(rng);
  }
}

function randScope(rng: () => number, slot: "?principal" | "?resource"): Scope {
  switch (ri(rng, 5)) {
    case 0: return { kind: "scopeAll" };
    case 1: return { kind: "scopeEq", entity: randEntity(rng) };
    case 2: return { kind: "scopeIn", entity: randEntity(rng) };
    case 3:
      return rng() < 0.5
        ? { kind: "scopeIs", entityType: "T" + ri(rng, 5), in: randEntity(rng) }
        : { kind: "scopeIs", entityType: "T" + ri(rng, 5) };
    default: return { kind: "slot", slot };
  }
}

function randActionScope(rng: () => number): ActionScope {
  switch (ri(rng, 3)) {
    case 0: return { kind: "scopeAll" };
    case 1: return { kind: "scopeEq", entity: randEntity(rng) };
    default: return { kind: "scopeIn", entities: Array.from({ length: 1 + ri(rng, 3) }, () => randEntity(rng)) };
  }
}

function randPolicy(rng: () => number): PolicyIR {
  return {
    kind: "policy",
    effect: rng() < 0.5 ? "permit" : "forbid",
    annotations: Array.from({ length: ri(rng, 3) }, (_, i) => ({ name: "ann" + i, value: "v" + ri(rng, 9) })),
    scope: {
      principal: randScope(rng, "?principal"),
      action: randActionScope(rng),
      resource: randScope(rng, "?resource"),
    },
    conditions: Array.from({ length: 1 + ri(rng, 2) }, () => ({
      kind: rng() < 0.5 ? ("when" as const) : ("unless" as const),
      body: randExpr(4, rng),
    })),
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

describe("property: synthesized full policies round-trip byte-exact with zero raw (#2 & #4)", () => {
  it(
    "10000 seeded scenarios (effect, all scope heads + slots, multi when/unless, annotations, full expr grammar)",
    () => {
      for (let i = 1; i <= 10000; i++) {
        const est = blocksToEst(randPolicy(lcg(i)));
        const ir2 = estToBlocks(est, null);
        expect(blocksToEst(ir2), `seed ${i}: not byte-exact`).toEqual(est);
        expect(rawCount(ir2), `seed ${i}: produced raw`).toBe(0);
      }
    },
    60000,
  );
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
    const ir: PolicyIR = {
      kind: "policy",
      effect: "permit",
      annotations: [],
      scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
      conditions: [{ kind: "when", body: { kind: "hole", expected: "expr", name: "maxUsd" } }],
    };
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
