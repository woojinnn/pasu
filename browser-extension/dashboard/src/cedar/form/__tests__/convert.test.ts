import { describe, expect, it } from "vitest";

import { blocksToEst } from "../../blocks/blocksToEst";
import type { Expr, PolicyIR } from "../../blocks/ir";
import { formToIr, irToForm } from "../convert";
import type { FormModel } from "../model";

/** A model exercising every supported shape: actionEq trigger, a bool `==`,
 *  a decimal `>=` (ext-method form), an `in` over a literal set, multiple AND
 *  groups, an OR within a group, and a custom field (triggers has-guards). */
const richModel: FormModel = {
  trigger: { kind: "actionEq", entityType: "Amm::Action", id: "Swap" },
  groups: [
    { leaves: [{ fieldPath: "context.flagged", op: "==", value: { kind: "bool", value: true } }] },
    {
      leaves: [
        { fieldPath: "context.custom.inputUsd", op: ">=", value: { kind: "decimal", value: "0.05" } },
        { fieldPath: "context.spender", op: "in", value: { kind: "set", values: ["0xabc", "0xdef"] } },
      ],
    },
  ],
  groupOp: "and",
  unlessGroups: [],
  unlessOp: "and",
  id: "my-policy",
  severity: "deny",
  reason: "위험 동작",
};

describe("formToIr / irToForm", () => {
  it("round-trips a rich model losslessly (guards are re-derived, not stored)", () => {
    expect(irToForm(formToIr(richModel))).toEqual(richModel);
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

  it("round-trips field-vs-field, group NOT, and an unless clause", () => {
    const m: FormModel = {
      trigger: { kind: "actionEq", entityType: "Token::Action", id: "Erc20Transfer" },
      groups: [
        // recipient != principal.address  (field vs field)
        { leaves: [{ fieldPath: "context.recipient", op: "!=", value: { kind: "field", path: "principal.address" } }] },
        // NOT (target in [allowlist])
        {
          negated: true,
          leaves: [{ fieldPath: "context.target", op: "in", value: { kind: "set", values: ["0xaa", "0xbb"] } }],
        },
      ],
      groupOp: "and",
      unlessGroups: [
        { leaves: [{ fieldPath: "context.flagged", op: "==", value: { kind: "bool", value: false } }] },
      ],
      unlessOp: "and",
      id: "p",
      severity: "deny",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("round-trips a DNF policy (groupOp 'or' — groups OR-ed, leaves AND-ed)", () => {
    const m: FormModel = {
      trigger: { kind: "any" },
      groups: [
        {
          leaves: [
            { fieldPath: "context.a", op: "==", value: { kind: "long", value: 1 } },
            { fieldPath: "context.b", op: "==", value: { kind: "long", value: 2 } },
          ],
        },
        { leaves: [{ fieldPath: "context.c", op: "==", value: { kind: "long", value: 3 } }] },
      ],
      groupOp: "or",
      unlessGroups: [],
      unlessOp: "and",
      id: "p",
      severity: "warn",
      reason: "",
    };
    expect(irToForm(formToIr(m))).toEqual(m);
  });

  it("normalizes `[set].contains(attr)` to an `in` leaf (allowlist policies)", () => {
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
    const form = irToForm(ir);
    expect(form?.groups).toEqual([
      { negated: true, leaves: [{ fieldPath: "context.target", op: "in", value: { kind: "set", values: ["0xaa", "0xbb"] } }] },
    ]);
  });

  it("auto-inserts has-guards for a custom field at the top-level AND", () => {
    const ir = formToIr({
      ...richModel,
      groups: [
        { leaves: [{ fieldPath: "context.custom.inputUsd", op: ">=", value: { kind: "decimal", value: "1" } }] },
      ],
    });
    const body = ir.conditions[0].body;
    // body = (context has custom) && (context.custom has inputUsd) && (inputUsd >= 1)
    const terms: Expr[] = [];
    const walk = (e: Expr) => {
      if (e.kind === "binary" && e.op === "&&") {
        walk(e.left);
        walk(e.right);
      } else terms.push(e);
    };
    walk(body);
    const guards = terms.filter((t) => t.kind === "has");
    expect(guards).toHaveLength(2);
    expect(guards).toContainEqual({ kind: "has", of: { kind: "var", name: "context" }, attr: "custom" });
    expect(guards).toContainEqual({
      kind: "has",
      of: { kind: "attr", of: { kind: "var", name: "context" }, attr: "custom" },
      attr: "inputUsd",
    });
  });

  it("produces an EST the local IR→EST converter accepts (valid Cedar shape)", () => {
    const est = blocksToEst(formToIr(richModel)) as { effect: string };
    expect(est.effect).toBe("forbid");
  });

  it("an empty model is a forbid with no when clause", () => {
    const empty: FormModel = {
      trigger: { kind: "any" },
      groups: [],
      groupOp: "and",
      unlessGroups: [],
      unlessOp: "and",
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

  it("parses a standalone unless clause into unlessGroups", () => {
    const ir = base();
    const form = irToForm({ ...ir, conditions: [{ kind: "unless", body: ir.conditions[0].body }] });
    expect(form).not.toBeNull();
    expect(form?.groups).toEqual([]);
    expect(form?.unlessGroups.length).toBeGreaterThan(0);
  });

  it("rejects a NOT in the body", () => {
    const ir: PolicyIR = {
      ...base(),
      conditions: [
        {
          kind: "when",
          body: {
            kind: "unary",
            op: "!",
            operand: { kind: "attr", of: { kind: "var", name: "context" }, attr: "flagged" },
          },
        },
      ],
    };
    expect(irToForm(ir)).toBeNull();
  });

  it("rejects two when clauses", () => {
    const ir = base();
    expect(irToForm({ ...ir, conditions: [...ir.conditions, ...ir.conditions] })).toBeNull();
  });
});
