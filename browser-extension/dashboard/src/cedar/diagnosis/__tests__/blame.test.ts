import { describe, it, expect } from "vitest";
import type { PolicyIR, Expr } from "../../blocks/ir";
import { blame, type TruthMap } from "../blame";

const attr = (name: string): Expr => ({ kind: "attr", of: { kind: "var", name: "context" }, attr: name });
const cmp = (a: string, op: any, n: number): Expr => ({ kind: "binary", op, left: attr(a), right: { kind: "lit", litType: "long", value: n } });

function forbidWhen(body: Expr): PolicyIR {
  return {
    kind: "policy", effect: "forbid", annotations: [],
    scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
    conditions: [{ kind: "when", body }],
  };
}

function forbidUnless(body: Expr): PolicyIR {
  return {
    kind: "policy", effect: "forbid", annotations: [],
    scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
    conditions: [{ kind: "unless", body }],
  };
}

describe("blame walker", () => {
  it("AND(true): both true conjuncts are responsible", () => {
    const body: Expr = { kind: "binary", op: "&&", left: cmp("a", ">", 1), right: cmp("b", ">", 1) };
    const policy = forbidWhen(body);
    const truth: TruthMap = { "c0.body": true, "c0.body.left": true, "c0.body.right": true };
    expect(blame(policy, truth).sort()).toEqual(["c0.body.left", "c0.body.right"]);
  });

  it("OR(true): only the true disjunct is responsible", () => {
    const body: Expr = { kind: "binary", op: "||", left: cmp("a", ">", 1), right: cmp("b", ">", 1) };
    const policy = forbidWhen(body);
    const truth: TruthMap = { "c0.body": true, "c0.body.left": true, "c0.body.right": false };
    expect(blame(policy, truth)).toEqual(["c0.body.left"]);
  });

  it("NOT(true): recurses into the operand as responsible-for-false", () => {
    // forbid when { !(a > 1) } fired ⇒ (a > 1) is FALSE and responsible.
    const inner = cmp("a", ">", 1);
    const body: Expr = { kind: "unary", op: "!", operand: inner };
    const policy = forbidWhen(body);
    const truth: TruthMap = { "c0.body": true, "c0.body.operand": false };
    expect(blame(policy, truth)).toEqual(["c0.body.operand"]);
  });

  // ── boolean-position attr / lit / if (the bug this fix targets) ──

  it("case 1 — OR with a boolean attr disjunct true: blames the attr leaf", () => {
    // forbid when { context.isFlagged || (context.amount > 100) }
    // isFlagged is a boolean `attr`; it's true, the comparison is false.
    const body: Expr = {
      kind: "binary", op: "||",
      left: attr("isFlagged"),          // boolean attr (left disjunct)
      right: cmp("amount", ">", 100),   // comparison (right disjunct)
    };
    const policy = forbidWhen(body);
    const truth: TruthMap = {
      "c0.body": true,
      "c0.body.left": true,   // isFlagged (boolean attr) — now probed
      "c0.body.right": false, // amount > 100
    };
    expect(blame(policy, truth)).toEqual(["c0.body.left"]);
  });

  it("case 2 — bare boolean attr body: blames the body itself", () => {
    // forbid when { context.reserveState.isFrozen }
    const body: Expr = {
      kind: "attr",
      of: { kind: "attr", of: { kind: "var", name: "context" }, attr: "reserveState" },
      attr: "isFrozen",
    };
    const policy = forbidWhen(body);
    const truth: TruthMap = { "c0.body": true };
    expect(blame(policy, truth)).toEqual(["c0.body"]);
  });

  it("case 3 — unless AND both-false: both boolean-attr conjuncts responsible", () => {
    // forbid unless { context.a && context.b }; both false ⇒ unless false ⇒
    // forbid fires; each false conjunct is responsible-for-false.
    const body: Expr = { kind: "binary", op: "&&", left: attr("a"), right: attr("b") };
    const policy = forbidUnless(body);
    const truth: TruthMap = {
      "c0.body": false,
      "c0.body.left": false,
      "c0.body.right": false,
    };
    expect(blame(policy, truth).sort()).toEqual(["c0.body.left", "c0.body.right"]);
  });

  it("case 4 — if in boolean position: blames cond + taken branch, excludes the other disjunct", () => {
    // forbid when {
    //   (if context.k == "x" then context.p else context.q) || (context.amount > 100)
    // }
    // the if (left disjunct) is true: cond true ⇒ then-branch (context.p) taken & true;
    // the amount comparison (right disjunct) is false.
    const ifNode: Expr = {
      kind: "if",
      cond: { kind: "binary", op: "==", left: attr("k"), right: { kind: "lit", litType: "string", value: "x" } },
      then: attr("p"),  // boolean attr (taken branch)
      else: attr("q"),  // boolean attr (untaken branch)
    };
    const body: Expr = { kind: "binary", op: "||", left: ifNode, right: cmp("amount", ">", 100) };
    const policy = forbidWhen(body);
    const truth: TruthMap = {
      "c0.body": true,
      "c0.body.left": true,        // the if node — true ⇒ || recurses into it
      "c0.body.left.cond": true,   // context.k == "x"
      "c0.body.left.then": true,   // context.p (taken)
      "c0.body.right": false,      // context.amount > 100 (excluded)
    };
    const culprits = blame(policy, truth);
    // the if's children paths are non-empty (cond + taken branch carry blame)
    expect(culprits).toContain("c0.body.left.cond");
    expect(culprits).toContain("c0.body.left.then");
    // the untaken branch and the false disjunct are excluded
    expect(culprits).not.toContain("c0.body.left.else");
    expect(culprits).not.toContain("c0.body.right");
  });
});
