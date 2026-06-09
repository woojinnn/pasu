import { describe, expect, it } from "vitest";

import { formToIr, irToForm } from "../convert";
import { fieldsForTrigger } from "../field-catalog";
import type { Expr } from "../../blocks/ir";
import type { FormModel, FormTrigger, FormValue } from "../model";

const SWAP: FormTrigger = { kind: "actionEq", entityType: "Amm::Action", id: "Swap" };
const TRANSFER: FormTrigger = { kind: "actionEq", entityType: "Token::Action", id: "Erc20Transfer" };

function pathOf(e: Expr): string {
  if (e.kind === "var") return e.name;
  if (e.kind === "attr") return `${pathOf(e.of)}.${e.attr}`;
  return "?";
}
/** Collect every `<of> has <attr>` presence guard in an Expr tree. */
function hasGuards(e: unknown, out: string[] = []): string[] {
  if (!e || typeof e !== "object") return out;
  const node = e as Expr;
  if (node.kind === "has") out.push(`${pathOf(node.of)} has ${node.attr}`);
  for (const v of Object.values(e as Record<string, unknown>)) {
    if (Array.isArray(v)) v.forEach((x) => hasGuards(x, out));
    else if (v && typeof v === "object") hasGuards(v, out);
  }
  return out;
}
function model(trigger: FormTrigger, fieldPath: string, op: string, value: FormValue): FormModel {
  return {
    trigger,
    when: [{ fieldPath, op: op as never, value, joiner: "and" }],
    unless: [],
    id: "t",
    severity: "warn",
    reason: "",
  };
}

describe("schema-scoped field catalog", () => {
  it("offers only the chosen action's fields", () => {
    const swap = fieldsForTrigger(SWAP).map((f) => f.path);
    expect(swap).toContain("context.recipient");
    expect(swap).toContain("context.slippageBp");
    // swap amounts live UNDER context.direction (not flat context.*)
    expect(swap).toContain("context.direction.amountInNano");
    expect(swap).not.toContain("context.amountInNano");
    // perp / governance fields must NOT appear on a swap policy
    expect(swap).not.toContain("context.maxLeverage");
    expect(swap).not.toContain("context.delegatee");

    const transfer = fieldsForTrigger(TRANSFER).map((f) => f.path);
    expect(transfer).toContain("context.recipient");
    expect(transfer).toContain("context.amount");
    expect(transfer).not.toContain("context.slippageBp"); // swap-only
  });

  it("marks optional schema fields", () => {
    const opt = fieldsForTrigger(SWAP).find((f) => f.path === "context.tokenOut.key.address");
    expect(opt?.optional).toBe(true);
    const req = fieldsForTrigger(SWAP).find((f) => f.path === "context.recipient");
    expect(req?.optional).toBe(false);
  });
});

describe("automatic has-guards from the schema", () => {
  it("guards a nested optional amount (context.direction.amountInNano)", () => {
    // `direction` is REQUIRED in SwapContext, only `amountInNano` is optional —
    // so exactly one guard, on the inner attr (not on direction).
    const ir = formToIr(model(SWAP, "context.direction.amountInNano", ">=", { kind: "long", value: 5 }));
    const g = hasGuards(ir.conditions[0].body);
    expect(g).toContain("context.direction has amountInNano");
    expect(g).not.toContain("context has direction");
  });

  it("guards a 2-step optional chain (context.tokenOut.key.address)", () => {
    const ir = formToIr(
      model(SWAP, "context.tokenOut.key.address", "==", { kind: "string", value: "0xabc" }),
    );
    const g = hasGuards(ir.conditions[0].body);
    expect(g).toContain("context has tokenOut");
    expect(g).toContain("context.tokenOut.key has address");
  });

  it("adds NO guard for a required field (context.recipient on swap)", () => {
    const ir = formToIr(model(SWAP, "context.recipient", "==", { kind: "string", value: "0xabc" }));
    expect(hasGuards(ir.conditions[0].body)).toEqual([]);
  });

  it("under any-action, even common fields are guarded (nothing guaranteed)", () => {
    const ir = formToIr(model({ kind: "any" }, "context.recipient", "==", { kind: "string", value: "0xabc" }));
    expect(hasGuards(ir.conditions[0].body)).toContain("context has recipient");
  });

  it("round-trips: guards are stripped back to the same leaf", () => {
    const m = model(SWAP, "context.direction.amountInNano", ">=", { kind: "long", value: 5 });
    const back = irToForm(formToIr(m));
    expect(back?.when[0]).toMatchObject({
      fieldPath: "context.direction.amountInNano",
      op: ">=",
    });
  });
});

describe("set membership (in) generates valid Cedar", () => {
  /** Collect every binary operator used in an Expr tree. */
  function ops(e: unknown, out: string[] = []): string[] {
    if (!e || typeof e !== "object") return out;
    const n = e as Expr;
    if (n.kind === "binary") out.push(n.op);
    for (const v of Object.values(e as Record<string, unknown>)) {
      if (Array.isArray(v)) v.forEach((x) => ops(x, out));
      else if (v && typeof v === "object") ops(v, out);
    }
    return out;
  }

  it("emits [set].contains(attr), not Cedar entity-`in` (which rejects String sets)", () => {
    const ir = formToIr(
      model(SWAP, "context.recipient", "in", { kind: "set", values: ["0xaaa", "0xbbb"] }),
    );
    const used = ops(ir.conditions[0].body);
    expect(used).toContain("contains");
    expect(used).not.toContain("in");
  });

  it("round-trips an `in` leaf back to op `in`", () => {
    const m = model(SWAP, "context.recipient", "in", { kind: "set", values: ["0xaaa"] });
    const back = irToForm(formToIr(m));
    expect(back?.when[0]).toMatchObject({ fieldPath: "context.recipient", op: "in" });
  });
});
