import { describe, it, expect } from "vitest";
import type { PolicyIR, Expr } from "../../blocks/ir";
import { buildProbes, diagnoseFromResult } from "../index";

const body: Expr = {
  kind: "binary", op: "&&",
  left: { kind: "binary", op: ">", left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "slippageBp" }, right: { kind: "lit", litType: "long", value: 100 } },
  right: { kind: "has", of: { kind: "var", name: "context" }, attr: "recipient" },
};
const policy: PolicyIR = {
  kind: "policy", effect: "forbid", annotations: [{ name: "id", value: "p" }],
  scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
  conditions: [{ kind: "when", body }],
};

describe("diagnosis end-to-end (pure)", () => {
  it("maps a true AND to both leaf culprits", () => {
    const { probes } = buildProbes(policy);
    const ids = probes.map((p) => p.id);
    // simulate WASM: the && body, the > leaf, and the has leaf are all true
    const result = { true_ids: ids, error_ids: [] };
    const d = diagnoseFromResult(policy, ids, result);
    expect(d.culprits.sort()).toEqual(["c0.body.left", "c0.body.right"]);
  });
});
