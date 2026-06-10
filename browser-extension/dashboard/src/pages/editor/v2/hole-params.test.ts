import { describe, expect, it } from "vitest";
import { parseHoleInput, formatHoleValue } from "./hole-params";

describe("hole input (HoleSpec.type별)", () => {
  it("long/decimal parse + reject non-numeric", () => {
    expect(parseHoleInput("long", "42")).toEqual({ ok: true, value: 42 });
    expect(parseHoleInput("long", "42.9")).toEqual({ ok: true, value: 42 });
    expect(parseHoleInput("decimal", "1.5")).toEqual({ ok: true, value: 1.5 });
    expect(parseHoleInput("long", "abc").ok).toBe(false);
    expect(parseHoleInput("decimal", "").ok).toBe(false);
  });

  it("addressSet splits/trims/lowercases lines", () => {
    expect(parseHoleInput("addressSet", "0xAB\n 0xCD \n")).toEqual({ ok: true, value: ["0xab", "0xcd"] });
    expect(parseHoleInput("addressSet", "")).toEqual({ ok: true, value: [] });
  });

  it("bool/string/address pass through", () => {
    expect(parseHoleInput("bool", "true")).toEqual({ ok: true, value: true });
    expect(parseHoleInput("bool", "false")).toEqual({ ok: true, value: false });
    expect(parseHoleInput("address", "0xAB")).toEqual({ ok: true, value: "0xab" });
    expect(parseHoleInput("string", "hi")).toEqual({ ok: true, value: "hi" });
  });

  it("formatHoleValue is the display inverse", () => {
    expect(formatHoleValue(["0xab", "0xcd"])).toBe("0xab\n0xcd");
    expect(formatHoleValue(42)).toBe("42");
    expect(formatHoleValue(true)).toBe("true");
    expect(formatHoleValue(undefined)).toBe("");
  });
});
