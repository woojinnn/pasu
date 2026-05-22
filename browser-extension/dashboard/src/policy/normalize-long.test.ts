import { describe, expect, it } from "vitest";
import { normalizeLongForDisplay } from "./normalize-long";

describe("normalizeLongForDisplay", () => {
  it("strips fractional-zero suffixes", () => {
    expect(normalizeLongForDisplay("1.0")).toBe("1");
    expect(normalizeLongForDisplay("100.00")).toBe("100");
    expect(normalizeLongForDisplay("-1.0")).toBe("-1");
    expect(normalizeLongForDisplay("0.0")).toBe("0");
  });

  it("normalizes the sign without affecting magnitude", () => {
    expect(normalizeLongForDisplay("+5")).toBe("5");
    expect(normalizeLongForDisplay("-5")).toBe("-5");
    expect(normalizeLongForDisplay("5")).toBe("5");
  });

  it("trims surrounding whitespace", () => {
    expect(normalizeLongForDisplay("  42 ")).toBe("42");
    expect(normalizeLongForDisplay("\t100.0\n")).toBe("100");
  });

  it("leaves canonical integers untouched", () => {
    expect(normalizeLongForDisplay("1")).toBe("1");
    expect(normalizeLongForDisplay("12345")).toBe("12345");
    expect(normalizeLongForDisplay("-12345")).toBe("-12345");
  });

  it("leaves non-zero fractional input alone so the Rust validator surfaces a clear error", () => {
    expect(normalizeLongForDisplay("1.5")).toBe("1.5");
    expect(normalizeLongForDisplay("100.01")).toBe("100.01");
    expect(normalizeLongForDisplay("-0.5")).toBe("-0.5");
  });

  it("leaves garbage and empty input alone", () => {
    expect(normalizeLongForDisplay("")).toBe("");
    expect(normalizeLongForDisplay("   ")).toBe("   ");
    expect(normalizeLongForDisplay("abc")).toBe("abc");
    expect(normalizeLongForDisplay("1.0.0")).toBe("1.0.0");
    expect(normalizeLongForDisplay("1.5a")).toBe("1.5a");
    expect(normalizeLongForDisplay("-")).toBe("-");
    expect(normalizeLongForDisplay(".5")).toBe(".5");
  });
});
