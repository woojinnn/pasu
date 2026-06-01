import { describe, expect, test } from "vitest";

import { policyIdFromName, stampAnnotations } from "./annotations";

describe("policyIdFromName", () => {
  test("spaces → underscores", () => {
    expect(policyIdFromName("Swap baseline")).toBe("Swap_baseline");
  });
  test("strips disallowed chars", () => {
    expect(policyIdFromName("slip>50 (bp)")).toBe("slip50_bp");
  });
  test("blank → 'policy'", () => {
    expect(policyIdFromName("   ")).toBe("policy");
  });

  test("Korean name → 'policy' (no '___' garbage)", () => {
    expect(policyIdFromName("특정 토큰 거래 차단")).toBe("policy");
  });

  test("mixed Korean + ASCII keeps the ASCII parts", () => {
    expect(policyIdFromName("USDC 차단")).toBe("USDC_");
  });
});

describe("stampAnnotations", () => {
  test("adds @id + @severity + @reason to plain cedar", () => {
    const out = stampAnnotations("permit(principal, action, resource);", "Slippage 50", "deny");
    expect(out).toBe(
      '@id("Slippage_50")\n@severity("deny")\n@reason("Slippage 50")\npermit(principal, action, resource);',
    );
  });

  test("replaces existing @id / @severity / @reason (no duplication)", () => {
    const input =
      '@id("old-name")\n@severity("warn")\n@reason("old reason")\npermit(principal, action, resource);';
    const out = stampAnnotations(input, "New Name", "deny");
    expect(out).toBe(
      '@id("New_Name")\n@severity("deny")\n@reason("New Name")\npermit(principal, action, resource);',
    );
  });

  test("leaves inline annotations deeper in the policy untouched", () => {
    const input = 'forbid(principal, action, resource) when {\n  @custom("x")\n  true\n};';
    const out = stampAnnotations(input, "x", "deny");
    expect(out).toContain('@custom("x")');
    expect(out.startsWith('@id("x")\n@severity("deny")\n@reason("x")\n')).toBe(true);
  });
});
