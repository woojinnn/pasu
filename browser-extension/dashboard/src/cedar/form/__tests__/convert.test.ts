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
    const ir = formToIr({ trigger: { kind: "any" }, groups: [], id: "p", severity: "warn", reason: "" });
    expect(ir.conditions).toEqual([]);
    expect(irToForm(ir)).toEqual({ trigger: { kind: "any" }, groups: [], id: "p", severity: "warn", reason: "" });
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

  it("rejects an unless clause", () => {
    const ir = base();
    expect(
      irToForm({ ...ir, conditions: [{ kind: "unless", body: ir.conditions[0].body }] }),
    ).toBeNull();
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

  it("rejects more than one clause", () => {
    const ir = base();
    expect(irToForm({ ...ir, conditions: [...ir.conditions, ...ir.conditions] })).toBeNull();
  });
});
