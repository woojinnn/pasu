import { describe, expect, it } from "vitest";
import { normalizeDecimalForDisplay } from "./normalize-decimal";

describe("normalizeDecimalForDisplay", () => {
  it("appends `.0` to bare integers", () => {
    expect(normalizeDecimalForDisplay("1")).toBe("1.0");
    expect(normalizeDecimalForDisplay("100")).toBe("100.0");
    expect(normalizeDecimalForDisplay("-1")).toBe("-1.0");
    expect(normalizeDecimalForDisplay("+5")).toBe("5.0");
  });

  it("fills missing fractional digits", () => {
    expect(normalizeDecimalForDisplay("1.")).toBe("1.0");
    expect(normalizeDecimalForDisplay("-2.")).toBe("-2.0");
  });

  it("prepends `0` to leading-dot input", () => {
    expect(normalizeDecimalForDisplay(".5")).toBe("0.5");
    expect(normalizeDecimalForDisplay("-.25")).toBe("-0.25");
  });

  it("leaves canonical input untouched", () => {
    expect(normalizeDecimalForDisplay("1.5")).toBe("1.5");
    expect(normalizeDecimalForDisplay("1.50")).toBe("1.50");
    expect(normalizeDecimalForDisplay("-0.25")).toBe("-0.25");
  });

  it("trims whitespace before normalizing", () => {
    expect(normalizeDecimalForDisplay("  42 ")).toBe("42.0");
  });

  it("returns invalid input unchanged so the Rust validator can report it", () => {
    expect(normalizeDecimalForDisplay("abc")).toBe("abc");
    expect(normalizeDecimalForDisplay("1.2.3")).toBe("1.2.3");
    expect(normalizeDecimalForDisplay("1 5")).toBe("1 5");
    expect(normalizeDecimalForDisplay("1.5a")).toBe("1.5a");
    expect(normalizeDecimalForDisplay("-")).toBe("-");
  });

  it("leaves empty input as-is (don't surprise the user with `0.0`)", () => {
    expect(normalizeDecimalForDisplay("")).toBe("");
    expect(normalizeDecimalForDisplay("   ")).toBe("   ");
  });
});
