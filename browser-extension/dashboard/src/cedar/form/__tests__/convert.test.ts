import { describe, expect, it } from "vitest";

import { blocksToEst } from "../../blocks/blocksToEst";
import type { Expr, PolicyIR } from "../../blocks/ir";
import { pathByNode } from "../../diagnosis/path";
import { policyDiagramPaths } from "../../diagram/PolicyDiagram";
import { formToIr, formToIrWithMap, irToForm } from "../convert";
import type { FormCondition, FormModel } from "../model";

const cond = (
  fieldPath: string,
  op: FormCondition["op"],
  value: FormCondition["value"],
  extra: Partial<FormCondition> = {},
): FormCondition => ({ fieldPath, op, value, joiner: "and", ...extra });

/** Exercises field-vs-field, decimal `>=` (ext form), `in`, a custom field
 *  (has-guards), a negated row, OR/AND joiners, and an unless clause. */
const richModel: FormModel = {
  trigger: { kind: "actionEq", entityType: "Amm::Action", id: "Swap" },
  when: [
    cond("context.recipient", "!=", { kind: "field", path: "principal.address" }),
    cond("context.custom.inputUsd", ">=", { kind: "decimal", value: "0.05" }, { joiner: "or" }),
    cond("context.spender", "in", { kind: "set", values: ["0xabc", "0xdef"] }, { joiner: "and" }),
  ],
  unless: [cond("context.flagged", "==", { kind: "bool", value: false })],
  id: "my-policy",
  severity: "deny",
  reason: "위험 동작",
};

describe("formToIr / irToForm", () => {
  it("round-trips a rich model losslessly", () => {
    expect(irToForm(formToIr(richModel))).toEqual(richModel);
  });

  it("round-trips a pure-AND list", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        cond("context.b", ">", { kind: "long", value: 2 }),
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("round-trips a mixed AND/OR list as an OR of AND-runs", () => {
    // A 그리고 B 또는 C  →  (A∧B) ∨ C
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        cond("context.b", "==", { kind: "long", value: 2 }, { joiner: "and" }),
        cond("context.c", "==", { kind: "long", value: 3 }, { joiner: "or" }),
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("round-trips a per-row NOT", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [cond("context.flagged", "==", { kind: "bool", value: true }, { not: true })],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const ir = formToIr(m);
    // body is `!(context.flagged == true)`
    expect(ir.conditions[0].body.kind).toBe("unary");
    expect(irToForm(ir)).toEqual(m);
  });

  it("emits forbid + action scope + @id/@severity/@reason", () => {
    const ir = formToIr(richModel);
    expect(ir.effect).toBe("forbid");
    expect(ir.scope.action).toEqual({ kind: "scopeEq", entity: { type: "Amm::Action", id: "Swap" } });
    expect(ir.annotations).toEqual([
      { name: "id", value: "my-policy" },
      { name: "severity", value: "deny" },
      { name: "reason", value: "위험 동작" },
    ]);
  });

  it("auto-inserts has-guards for a custom field inside its run", () => {
    const ir = formToIr({
      ...richModel,
      when: [cond("context.custom.inputUsd", ">=", { kind: "decimal", value: "1" })],
      unless: [],
    });
    const terms: Expr[] = [];
    const walk = (e: Expr) => {
      if (e.kind === "binary" && e.op === "&&") {
        walk(e.left);
        walk(e.right);
      } else terms.push(e);
    };
    walk(ir.conditions[0].body);
    const guards = terms.filter((t) => t.kind === "has");
    expect(guards).toHaveLength(2);
    expect(guards).toContainEqual({ kind: "has", of: { kind: "var", name: "context" }, attr: "custom" });
    expect(guards).toContainEqual({
      kind: "has",
      of: { kind: "attr", of: { kind: "var", name: "context" }, attr: "custom" },
      attr: "inputUsd",
    });
  });

  it("places has-guards inside the run that uses the optional field (no fail-open)", () => {
    // when { a == 1 || custom.x >= 1 } — 가드가 OR 전체가 아니라 두 번째 run 안에만.
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        cond("context.custom.x", ">=", { kind: "decimal", value: "1" }, { joiner: "or" }),
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const body = formToIr(m).conditions[0].body;
    expect(body.kind).toBe("binary");
    if (body.kind !== "binary") return;
    expect(body.op).toBe("||");
    // 왼쪽 run(a == 1)에는 has가 전혀 없어야 한다.
    const hasCount = (e: Expr): number => {
      if (e.kind === "has") return 1;
      if (e.kind === "binary") return hasCount(e.left) + hasCount(e.right);
      if (e.kind === "unary") return hasCount(e.operand);
      return 0;
    };
    expect(hasCount(body.left)).toBe(0);
    expect(hasCount(body.right)).toBe(2); // context has custom, context.custom has x
    // round-trip 유지
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("rejects a clause where a run is only guards", () => {
    // when { a == 1 || context has custom } — has만 남는 run은 폼 밖(블록 핸드오프).
    const a: Expr = {
      kind: "binary",
      op: "==",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "a" },
      right: { kind: "lit", litType: "long", value: 1 },
    };
    const guardOnly: Expr = { kind: "has", of: { kind: "var", name: "context" }, attr: "custom" };
    const ir: PolicyIR = {
      ...formToIr(richModel),
      conditions: [{ kind: "when", body: { kind: "binary", op: "||", left: a, right: guardOnly } }],
    };
    expect(irToForm(ir)).toBeNull();
  });

  it("rejects a group whose disjunct is an AND-run `X && (A && B || C)` (OR-only groups)", () => {
    const cmp2 = (n: string, v: number): Expr => ({
      kind: "binary",
      op: "==",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: n },
      right: { kind: "lit", litType: "long", value: v },
    });
    const ir: PolicyIR = {
      ...formToIr(richModel),
      conditions: [
        {
          kind: "when",
          body: {
            kind: "binary",
            op: "&&",
            left: cmp2("x", 0),
            right: {
              kind: "binary",
              op: "||",
              left: { kind: "binary", op: "&&", left: cmp2("a", 1), right: cmp2("b", 2) },
              right: cmp2("c", 3),
            },
          },
        },
      ],
    };
    expect(irToForm(ir)).toBeNull();
  });

  it("normalizes `[set].contains(attr)` to an `in` condition (allowlist policies)", () => {
    // forbid when { !(["0xaa","0xbb"].contains(context.target)) }
    const ir: PolicyIR = {
      ...formToIr(richModel),
      conditions: [
        {
          kind: "when",
          body: {
            kind: "unary",
            op: "!",
            operand: {
              kind: "binary",
              op: "contains",
              left: { kind: "set", elements: [
                { kind: "lit", litType: "string", value: "0xaa" },
                { kind: "lit", litType: "string", value: "0xbb" },
              ] },
              right: { kind: "attr", of: { kind: "var", name: "context" }, attr: "target" },
            },
          },
        },
      ],
    };
    expect(irToForm(ir)?.when).toEqual([
      cond("context.target", "in", { kind: "set", values: ["0xaa", "0xbb"] }, { not: true }),
    ]);
  });

  it("formToIrWithMap: every form node maps to an Expr whose path the diagram renders", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        cond("context.flagged", "==", { kind: "bool", value: true }, { not: true }),
        cond("context.spender", "in", { kind: "set", values: ["0xaa", "0xbb"] }, { joiner: "or" }),
        {
          kind: "group",
          joiner: "and",
          conds: [
            cond("context.b", "==", { kind: "long", value: 2 }),
            cond("context.c", "==", { kind: "long", value: 3 }, { joiner: "or" }),
          ],
        },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const { ir, exprsByNode, runRootByHead } = formToIrWithMap(m);
    const pathOf = pathByNode(ir);
    const shown = new Set(policyDiagramPaths(ir));
    // 모든 폼 노드(when의 leaf 3 + group 1 = 4, group 안 leaf 2 = 6)가 등록되고,
    expect(exprsByNode.size).toBe(6);
    for (const exprs of exprsByNode.values()) {
      const paths = exprs.map((e) => pathOf.get(e));
      // 각 Expr은 canonical path를 갖고, 그중 하나는 다이어그램이 실제로 그린다.
      expect(paths.every(Boolean)).toBe(true);
      expect(paths.some((p) => shown.has(p!))).toBe(true);
    }
    // run 머리(1번째/3번째 노드) → run 루트 게이트.
    expect(runRootByHead.size).toBe(2);
    expect(runRootByHead.has(m.when[0])).toBe(true);
    expect(runRootByHead.has(m.when[2])).toBe(true);
    for (const root of runRootByHead.values()) expect(pathOf.get(root)).toBeTruthy();
    // 동일 모델의 formToIr와 같은 IR.
    expect(ir).toEqual(formToIr(m));
  });

  it("produces an EST the local IR→EST converter accepts", () => {
    const est = blocksToEst(formToIr(richModel)) as { effect: string };
    expect(est.effect).toBe("forbid");
  });

  it("skips an empty group box (no crash on fold of empty)", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        { kind: "group", joiner: "and", conds: [] },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(() => formToIr(m)).not.toThrow();
    expect(formToIr(m).conditions).toHaveLength(1); // empty box dropped
    // a clause that is ONLY an empty box → no when clause
    expect(formToIr({ ...m, when: [{ kind: "group", joiner: "and", conds: [] }] }).conditions).toEqual([]);
  });

  it("an empty model is a forbid with no when clause", () => {
    const empty: FormModel = {
      trigger: { kind: "any" },
      when: [],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const ir = formToIr(empty);
    expect(ir.conditions).toEqual([]);
    expect(irToForm(ir)).toEqual(empty);
  });

  // ── irToForm returns null outside the representable subset ──

  const base = (): PolicyIR => formToIr(richModel);

  it("rejects permit", () => {
    expect(irToForm({ ...base(), effect: "permit" })).toBeNull();
  });

  it("rejects a scopeIn action trigger", () => {
    expect(
      irToForm({
        ...base(),
        scope: {
          principal: { kind: "scopeAll" },
          action: { kind: "scopeIn", entities: [{ type: "Action", id: "A" }] },
          resource: { kind: "scopeAll" },
        },
      }),
    ).toBeNull();
  });

  it("parses a standalone unless clause into the unless list", () => {
    const ir = base();
    const form = irToForm({ ...ir, conditions: [{ kind: "unless", body: ir.conditions[0].body }] });
    expect(form).not.toBeNull();
    expect(form?.when).toEqual([]);
    expect(form?.unless.length).toBeGreaterThan(0);
  });

  it("rejects two when clauses", () => {
    const ir = base();
    expect(irToForm({ ...ir, conditions: [...ir.conditions, ...ir.conditions] })).toBeNull();
  });

  it("round-trips an explicit group `A && (B || C)`", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        {
          kind: "group",
          joiner: "and",
          conds: [
            cond("context.b", "==", { kind: "long", value: 2 }),
            cond("context.c", "==", { kind: "long", value: 3 }, { joiner: "or" }),
          ],
        },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("round-trips a CNF of two groups `(A || B) && (C || D)`", () => {
    const g = (a: string, b: string) => ({
      kind: "group" as const,
      joiner: "and" as const,
      conds: [
        cond(`context.${a}`, "==", { kind: "long", value: 1 }),
        cond(`context.${b}`, "==", { kind: "long", value: 2 }, { joiner: "or" }),
      ],
    });
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [g("a", "b"), g("c", "d")],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("rejects a doubly-nested group (hands off to blocks)", () => {
    // forbid when { z==0 && (a==1 || (b==2 && (c==3 || d==4))) } — a group whose
    // own internals need another group → beyond the one-level form subset.
    const cmp = (n: string, v: number): Expr => ({
      kind: "binary",
      op: "==",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: n },
      right: { kind: "lit", litType: "long", value: v },
    });
    const or = (l: Expr, r: Expr): Expr => ({ kind: "binary", op: "||", left: l, right: r });
    const and = (l: Expr, r: Expr): Expr => ({ kind: "binary", op: "&&", left: l, right: r });
    const ir: PolicyIR = {
      ...base(),
      conditions: [
        {
          kind: "when",
          body: and(cmp("z", 0), or(cmp("a", 1), and(cmp("b", 2), or(cmp("c", 3), cmp("d", 4))))),
        },
      ],
    };
    expect(irToForm(ir)).toBeNull();
  });
});
