import { describe, expect, it } from "vitest";
import { assertSlotPositions, renderAndVerify } from "../template-renderer";

describe("assertSlotPositions", () => {
  it("accepts a slot in a comparison RHS", () => {
    expect(() =>
      assertSlotPositions(
        "permit(principal, action, resource) when { context.totalInputUsd <= {{cap}} };",
      ),
    ).not.toThrow();
  });

  it("accepts a slot inside a set literal", () => {
    expect(() =>
      assertSlotPositions(
        "permit(principal, action, resource) when { context.spender in [{{a}}, {{b}}] };",
      ),
    ).not.toThrow();
  });

  it("rejects predicate-as-slot (the documented Codex finding)", () => {
    expect(() =>
      assertSlotPositions(
        "permit(principal, action, resource) when { {{cap}} };",
      ),
    ).toThrow(/non-whitelisted position/);
  });

  it("strips comments before scanning", () => {
    expect(() =>
      assertSlotPositions(
        "// {{ignored}} in a comment\npermit(principal, action, resource) when { context.x <= {{cap}} };",
      ),
    ).not.toThrow();
  });
});

describe("renderAndVerify", () => {
  it("renders integer template with substituted value", () => {
    const out = renderAndVerify({
      policyId: "demo::cap",
      templateText:
        "permit(principal, action, resource) when { context.totalInputUsd <= {{cap}} };",
      paramsSchema: { cap: { type: "integer", min: 1, max: 1000 } },
      paramValues: { cap: 250 },
    });
    expect(out).toContain("250");
  });

  it("escapes string values via JSON.stringify", () => {
    const out = renderAndVerify({
      policyId: "demo::label",
      templateText:
        "permit(principal, action, resource) when { context.label == {{label}} };",
      paramsSchema: {
        label: { type: "string", maxLen: 32, allowedChars: "A-Za-z" },
      },
      paramValues: { label: "demo" },
    });
    expect(out).toContain('"demo"');
  });

  it("renders address as a quoted string (no EthereumAddress entity)", () => {
    const out = renderAndVerify({
      policyId: "demo::spender",
      templateText:
        "permit(principal, action, resource) when { context.spender == {{addr}} };",
      paramsSchema: { addr: { type: "address" } },
      paramValues: { addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2" },
    });
    expect(out).toContain('"0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"');
  });

  it("rejects predicate-as-slot at renderAndVerify entry", () => {
    expect(() =>
      renderAndVerify({
        policyId: "demo::evil",
        templateText: "permit(principal, action, resource) when { {{cap}} };",
        paramsSchema: { cap: { type: "integer", min: 0, max: 1 } },
        paramValues: { cap: 0 },
      }),
    ).toThrow(/non-whitelisted/);
  });
});
