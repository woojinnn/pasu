import { describe, it, expect } from "vitest";
import { blocksToEst } from "../blocksToEst";
import { estToBlocks } from "../estToBlocks";
import type { ActionScope, EntityRef, Expr, LikePattern, PolicyIR, Scope } from "../ir";

// Deterministic PRNG so any failure reproduces from its seed.
function lcg(seed: number): () => number {
  let x = (seed >>> 0) || 1;
  return () => {
    x = (Math.imul(x, 1103515245) + 12345) >>> 0;
    return x / 0xffffffff;
  };
}
const ri = (rng: () => number, n: number) => Math.floor(rng() * n);
const pick = <T>(rng: () => number, arr: readonly T[]): T => arr[ri(rng, arr.length)];

// Canonical (key-sorted) JSON for order-insensitive structural comparison.
function canon(x: any): any {
  if (Array.isArray(x)) return x.map(canon);
  if (x && typeof x === "object") {
    const o: Record<string, any> = {};
    for (const k of Object.keys(x).sort()) o[k] = canon(x[k]);
    return o;
  }
  return x;
}
const eq = (a: any, b: any) => JSON.stringify(canon(a)) === JSON.stringify(canon(b));

const BINARY_OPS = [
  "==", "!=", "<", "<=", ">", ">=", "&&", "||", "+", "-", "*",
  "in", "contains", "containsAll", "containsAny", "getTag", "hasTag",
] as const;
const UNARY_OPS = ["!", "neg", "isEmpty"] as const;
const VARS = ["principal", "action", "resource", "context"] as const;
const EXT_FNS = ["ip", "decimal", "lessThan", "greaterThan", "isInRange"] as const;
const STRINGS = ["", '"', "\\", 'a"b\\c', "日本語", "emoji😀", "a\nb", "x\ty", "0xDEADBEEF"] as const;
const LONGS = [0, 1, -1, 42, 2147483647, -2147483648, 9007199254740991, -9007199254740991] as const;

function randEntity(rng: () => number): EntityRef {
  return { type: "T" + ri(rng, 6), id: pick(rng, STRINGS) || "e" + ri(rng, 9) };
}
function randPattern(rng: () => number): LikePattern {
  return Array.from({ length: ri(rng, 6) }, () =>
    rng() < 0.4 ? "Wildcard" : { Literal: String.fromCharCode(97 + ri(rng, 26)) },
  );
}
function randLeaf(rng: () => number): Expr {
  switch (ri(rng, 5)) {
    case 0: return { kind: "lit", litType: "long", value: pick(rng, LONGS) };
    case 1: return { kind: "lit", litType: "string", value: pick(rng, STRINGS) };
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
        ? { kind: "is", of: randExpr(d, rng), entityType: "T" + ri(rng, 6), in: randExpr(d, rng) }
        : { kind: "is", of: randExpr(d, rng), entityType: "T" + ri(rng, 6) };
    case 6: return { kind: "if", cond: randExpr(d, rng), then: randExpr(d, rng), else: randExpr(d, rng) };
    case 7: return { kind: "set", elements: Array.from({ length: ri(rng, 4) }, () => randExpr(d, rng)) };
    case 8:
      return { kind: "record", pairs: Array.from({ length: ri(rng, 4) }, (_, i) => ({ key: "k" + i, value: randExpr(d, rng) })) };
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
        ? { kind: "scopeIs", entityType: "T" + ri(rng, 6), in: randEntity(rng) }
        : { kind: "scopeIs", entityType: "T" + ri(rng, 6) };
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
function randPolicy(rng: () => number, depth: number): PolicyIR {
  return {
    kind: "policy",
    effect: rng() < 0.5 ? "permit" : "forbid",
    annotations: Array.from({ length: ri(rng, 3) }, (_, i) => ({ name: "ann" + i, value: pick(rng, STRINGS) })),
    scope: {
      principal: randScope(rng, "?principal"),
      action: randActionScope(rng),
      resource: randScope(rng, "?resource"),
    },
    conditions: Array.from({ length: 1 + ri(rng, 2) }, () => ({
      kind: rng() < 0.5 ? ("when" as const) : ("unless" as const),
      body: randExpr(depth, rng),
    })),
  };
}

// A LINEAR chain of `n` nested unary `!` — bounded width, depth `n`. Stresses
// recursion depth without exponential tree blow-up.
function deepPolicy(n: number): PolicyIR {
  let body: Expr = { kind: "lit", litType: "bool", value: true };
  for (let i = 0; i < n; i++) body = { kind: "unary", op: "!", operand: body };
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

// Round-trip one generated policy. Returns "" on success or a failure description.
function roundTrip(label: string, genIr: PolicyIR): string {
  const est = blocksToEst(genIr);
  const ir1 = estToBlocks(est, null);
  const est1 = blocksToEst(ir1);
  if (!eq(est1, est)) return `${label}: EST not preserved`;
  if (!eq(ir1, genIr)) return `${label}: IR not stable (IR→EST→IR drift)`;
  if (rawCount(ir1) !== 0) return `${label}: produced raw`;
  return "";
}

describe("property: hardened scenario-synthesis fuzz", () => {
  it(
    "100000 seeded full policies: EST preserved + IR→EST→IR stable + zero raw",
    () => {
      let firstFail = "";
      let failures = 0;
      for (let i = 1; i <= 100000; i++) {
        const f = roundTrip(`seed ${i}`, randPolicy(lcg(i), 3 + (i % 3)));
        if (f) {
          failures++;
          if (!firstFail) firstFail = f;
        }
      }
      expect(failures, firstFail).toBe(0);
    },
    120000,
  );

  it(
    "2000 linear deep chains (depth 50–349, well under MAX_DEPTH) round-trip stably",
    () => {
      let firstFail = "";
      for (let i = 1; i <= 2000; i++) {
        const f = roundTrip(`deep ${i}`, deepPolicy(50 + (i % 300)));
        if (f) {
          firstFail = f;
          break;
        }
      }
      expect(firstFail, firstFail).toBe("");
    },
    120000,
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
      conditions: [
        {
          kind: "when",
          body: {
            kind: "hole",
            name: "maxUsd",
            expected: "lit:long",
            default: { kind: "lit", litType: "long", value: 0 },
          },
        },
      ],
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
