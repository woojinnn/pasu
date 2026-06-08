import { describe, expect, it } from "vitest";

import { blocksToEst } from "../../blocks/blocksToEst";
import type { Expr, PolicyIR } from "../../blocks/ir";
import { formToIr, irToForm } from "../convert";
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

  it("auto-inserts has-guards for a custom field at the top-level AND", () => {
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

  it("produces an EST the local IR→EST converter accepts", () => {
    const est = blocksToEst(formToIr(richModel)) as { effect: string };
    expect(est.effect).toBe("forbid");
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

  it("rejects a CNF OR-group `A && (B || C)` (hands off to blocks)", () => {
    const cmp = (n: string): Expr => ({
      kind: "binary",
      op: "==",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: n },
      right: { kind: "lit", litType: "bool", value: true },
    });
    const ir: PolicyIR = {
      ...base(),
      conditions: [
        {
          kind: "when",
          body: {
            kind: "binary",
            op: "&&",
            left: cmp("a"),
            right: { kind: "binary", op: "||", left: cmp("b"), right: cmp("c") },
          },
        },
      ],
    };
    expect(irToForm(ir)).toBeNull();
  });
});
