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
});
