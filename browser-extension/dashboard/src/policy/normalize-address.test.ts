import { describe, expect, it } from "vitest";
import {
  isEvmAddressField,
  normalizeAddressForDisplay,
} from "./normalize-address";

describe("isEvmAddressField", () => {
  it("matches only the canonical EVM address regex literal", () => {
    expect(isEvmAddressField("^0x[0-9a-fA-F]{40}$")).toBe(true);
    expect(isEvmAddressField("^[0-9]+$")).toBe(false);
    expect(isEvmAddressField(undefined)).toBe(false);
    expect(isEvmAddressField("")).toBe(false);
  });
});

describe("normalizeAddressForDisplay", () => {
  const valid = "0x1234567890abcdef1234567890ABCDEF12345678";
  const bare = "1234567890abcdef1234567890ABCDEF12345678";

  it("prepends `0x` to bare 40-hex input", () => {
    expect(normalizeAddressForDisplay(bare)).toBe(`0x${bare}`);
  });

  it("folds an uppercase `0X` prefix to lowercase", () => {
    expect(normalizeAddressForDisplay(`0X${bare}`)).toBe(`0x${bare}`);
  });

  it("trims whitespace before checking the shape", () => {
    expect(normalizeAddressForDisplay(`  ${bare}  `)).toBe(`0x${bare}`);
    expect(normalizeAddressForDisplay(`\n0X${bare}\t`)).toBe(`0x${bare}`);
  });

  it("preserves checksum case on the 40-hex body", () => {
    // Mixed case in the body is the EIP-55 checksum — we MUST NOT
    // touch it or wallets will reject the address.
    expect(normalizeAddressForDisplay(valid)).toBe(valid);
    expect(normalizeAddressForDisplay(`0X${bare}`)).toBe(`0x${bare}`);
  });

  it("leaves anything that doesn't structurally resemble an address alone", () => {
    expect(normalizeAddressForDisplay("")).toBe("");
    expect(normalizeAddressForDisplay("   ")).toBe("   ");
    expect(normalizeAddressForDisplay("0xabc")).toBe("0xabc"); // too short
    expect(normalizeAddressForDisplay("0xZZZZ".padEnd(42, "Z"))).toBe(
      "0xZZZZ".padEnd(42, "Z"),
    ); // non-hex
    expect(normalizeAddressForDisplay(`${bare}0`)).toBe(`${bare}0`); // 41-hex
  });
});
