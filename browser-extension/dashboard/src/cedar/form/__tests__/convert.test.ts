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

  it("canonicalizes a hand-written `!(a == b)` into the complement operator", () => {
    const ir = formToIr({
      trigger: { kind: "any" },
      when: [cond("context.flagged", "==", { kind: "bool", value: true })],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    });
    const negated: PolicyIR = {
      ...ir,
      conditions: [{ kind: "when", body: { kind: "unary", op: "!", operand: ir.conditions[0].body } }],
    };
    expect(irToForm(negated)?.when).toEqual([
      cond("context.flagged", "!=", { kind: "bool", value: true }),
    ]);
  });

  it("round-trips the negative memberships (notIn / notContains)", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.target", "notIn", { kind: "set", values: ["0xaa", "0xbb"] }),
        cond("context.tags", "notContains", { kind: "string", value: "risky" }),
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const ir = formToIr(m);
    // both emit `!(…contains…)`
    expect(ir.conditions[0].body.kind).toBe("binary"); // && of the two
    expect(irToForm(ir)).toEqual(m);
  });

  it("De-Morgans `!(A || B)` in an AND context into complemented rows", () => {
    // when { z == 0 && !(a == 1 || b == 2) }  →  [z==0, a!=1, b!=2]
    const cmp3 = (n: string, v: number): Expr => ({
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
            left: cmp3("z", 0),
            right: {
              kind: "unary",
              op: "!",
              operand: { kind: "binary", op: "||", left: cmp3("a", 1), right: cmp3("b", 2) },
            },
          },
        },
      ],
    };
    const form = irToForm(ir);
    expect(form?.when).toEqual([
      cond("context.z", "==", { kind: "long", value: 0 }),
      cond("context.a", "!=", { kind: "long", value: 1 }),
      cond("context.b", "!=", { kind: "long", value: 2 }),
    ]);
    // and the canonical form round-trips stably
    const m: FormModel = { ...richModel, when: form!.when, unless: [] };
    expect(irToForm(formToIr(m))?.when).toEqual(form?.when);
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

  it("opens `X && (A && B || C)` as an OR-group with an AND-subgroup alternative", () => {
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
    const form = irToForm(ir);
    expect(form).not.toBeNull();
    expect(form?.when).toEqual([
      cond("context.x", "==", { kind: "long", value: 0 }),
      {
        kind: "group",
        joiner: "and",
        conds: [
          {
            kind: "group",
            joiner: "and",
            conds: [
              cond("context.a", "==", { kind: "long", value: 1 }),
              cond("context.b", "==", { kind: "long", value: 2 }, { joiner: "or" }),
            ],
          },
          cond("context.c", "==", { kind: "long", value: 3 }, { joiner: "or" }),
        ],
      },
    ]);
    // and round-trips losslessly from the form side
    const m: FormModel = { ...richModel, when: form!.when, unless: [] };
    expect(irToForm(formToIr(m))?.when).toEqual(form?.when);
  });

  it("round-trips the deep nesting (A && (B||C)) && D && (E || (F && (G||H)))", () => {
    const c1 = (n: string, j: "and" | "or" = "and") =>
      cond(`context.${n}`, "==", { kind: "long", value: 1 }, j === "or" ? { joiner: "or" } : {});
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        c1("a"),
        { kind: "group", joiner: "and", conds: [c1("b"), c1("c", "or")] },
        c1("d"),
        {
          kind: "group",
          joiner: "and",
          conds: [
            c1("e"),
            {
              kind: "group",
              joiner: "or",
              conds: [
                c1("f"),
                { kind: "group", joiner: "or", conds: [c1("g"), c1("h", "or")] },
              ],
            },
          ],
        },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const round = irToForm(formToIr(m));
    expect(round).not.toBeNull();
    // joiners inside groups are display-dead and normalized (head and, rest or)
    expect(round?.when).toHaveLength(4);
    expect(irToForm(formToIr(round!))).toEqual(round);
  });

  it("guards an optional field inside its nearest AND context only (deep branch)", () => {
    // when { a == 1 && (b == 2 || custom.x >= 1) } — 가드는 custom.x 선택지에만.
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        {
          kind: "group",
          joiner: "and",
          conds: [
            cond("context.b", "==", { kind: "long", value: 2 }),
            cond("context.custom.x", ">=", { kind: "decimal", value: "1" }, { joiner: "or" }),
          ],
        },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const body = formToIr(m).conditions[0].body;
    const hasCount = (e: Expr): number => {
      if (e.kind === "has") return 1;
      if (e.kind === "binary") return hasCount(e.left) + hasCount(e.right);
      if (e.kind === "unary") return hasCount(e.operand);
      return 0;
    };
    // body = a==1 && (b==2 || (has·has && x>=1))
    expect(body.kind).toBe("binary");
    if (body.kind !== "binary") return;
    const orNode = body.right;
    expect(orNode.kind === "binary" && orNode.op === "||").toBe(true);
    if (orNode.kind !== "binary") return;
    expect(hasCount(body.left)).toBe(0); // a==1 쪽엔 가드 없음
    expect(hasCount(orNode.left)).toBe(0); // b==2 선택지엔 없음
    expect(hasCount(orNode.right)).toBe(2); // custom.x 선택지에만
    expect(irToForm(formToIr(m))).toEqual(m);
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
      cond("context.target", "notIn", { kind: "set", values: ["0xaa", "0xbb"] }),
    ]);
  });

  it("formToIrWithMap: every form node maps to an Expr whose path the diagram renders", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      when: [
        cond("context.a", "==", { kind: "long", value: 1 }),
        cond("context.flagged", "notIn", { kind: "set", values: ["a", "b"] }),
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

  it("opens a doubly-nested group recursively (parity alternation)", () => {
    // forbid when { z==0 && (a==1 || (b==2 && (c==3 || d==4))) } — depth 4,
    // alternating OR/AND containers.
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
    const form = irToForm(ir);
    expect(form).not.toBeNull();
    const m: FormModel = { ...richModel, when: form!.when, unless: [] };
    expect(irToForm(formToIr(m))?.when).toEqual(form?.when);
  });
});

describe("baked day1 shapes (builtin defs must open in the form)", () => {
  /** unlimited-approval-deny 본문 그대로: has 가드 2단 + bare bool + NOT-contains. */
  it("has-guarded bare bool + !(set.contains(attr)) round-trips", () => {
    const attr = (of: Expr, name: string): Expr => ({ kind: "attr", of, attr: name });
    const ctx: Expr = { kind: "var", name: "context" };
    const custom = attr(ctx, "custom");
    const body: Expr = {
      kind: "binary",
      op: "&&",
      left: {
        kind: "binary",
        op: "&&",
        left: {
          kind: "binary",
          op: "&&",
          left: { kind: "has", of: ctx, attr: "custom" },
          right: { kind: "has", of: custom, attr: "approvalIsUnlimited" },
        },
        right: attr(custom, "approvalIsUnlimited"),
      },
      right: {
        kind: "unary",
        op: "!",
        operand: {
          kind: "binary",
          op: "contains",
          left: { kind: "set", elements: [{ kind: "lit", litType: "string", value: "0xperm2" }] },
          right: attr(ctx, "spender"),
        },
      },
    };
    const ir: PolicyIR = {
      kind: "policy",
      effect: "forbid",
      annotations: [
        { name: "id", value: "unlimited-approval-deny" },
        { name: "severity", value: "deny" },
      ],
      scope: {
        principal: { kind: "scopeAll" },
        action: { kind: "scopeEq", entity: { type: "Token::Action", id: "Erc20Approve" } },
        resource: { kind: "scopeAll" },
      },
      conditions: [{ kind: "when", body }],
    };

    const model = irToForm(ir);
    expect(model).not.toBeNull();
    expect(model!.when).toEqual([
      expect.objectContaining({
        fieldPath: "context.custom.approvalIsUnlimited",
        op: "==",
        value: { kind: "bool", value: true },
      }),
      expect.objectContaining({ fieldPath: "context.spender", op: "notIn" }),
    ]);

    // 폼 → IR 재방출도 폼 호환 (저장 후 다시 열기 안정성)
    const reopened = irToForm(formToIr(model!));
    expect(reopened).not.toBeNull();
    expect(reopened!.when.length).toBe(2);
  });
});
