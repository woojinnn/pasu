import { describe, expect, it } from "vitest";

import { extractHoles, redactCedar } from "../publish-redact";

const RECIPIENT = `@id("recipient-blocklist-deny")
forbid(principal, action, resource)
when { context.recipient == "0xA1c4000000000000000000000000000000007e29" };`;

const ALLOWLIST = `@id("gov-delegatee-allowlist-deny")
forbid(principal, action, resource)
when { ["0x91d2000000000000000000000000000000000001", "0x44ab000000000000000000000000000000000002"].contains(context.delegatee) };`;

const SLIPPAGE = `@id("swap-slippage-wide-warn")
forbid(principal, action, resource)
when { context.slippageBp >= 150 };`;

describe("extractHoles", () => {
  it("finds a literal address comparison and blanks it (forced)", () => {
    const holes = extractHoles(RECIPIENT);
    const addr = holes.find((h) => h.kind === "address");
    expect(addr).toBeTruthy();
    expect(addr!.path).toBe("context.recipient");
    expect(addr!.paramName).toBe("?recipient");
    const out = redactCedar(RECIPIENT, holes, new Set());
    expect(out).not.toContain("0xA1c4000000000000000000000000000000007e29");
  });

  it("finds an address set behind .contains()", () => {
    const holes = extractHoles(ALLOWLIST);
    const addr = holes.find((h) => h.kind === "address");
    expect(addr).toBeTruthy();
    expect(addr!.addrCount).toBe(2);
    expect(addr!.path).toBe("context.delegatee");
  });

  it("finds a numeric threshold with its value", () => {
    const holes = extractHoles(SLIPPAGE);
    const num = holes.find((h) => h.kind === "number");
    expect(num).toBeTruthy();
    expect(num!.display).toBe("150");
  });

  it("redacts addresses always; keeps a number only when chosen", () => {
    const holes = extractHoles(SLIPPAGE);
    const num = holes.find((h) => h.kind === "number")!;

    const kept = redactCedar(SLIPPAGE, holes, new Set([num.key]));
    expect(kept).toContain("150"); // author kept the recommended value

    const blanked = redactCedar(SLIPPAGE, holes, new Set());
    expect(blanked).not.toContain("150"); // blanked to 0
    expect(blanked).toContain("context.slippageBp >= 0");
  });

  it("blanks a real address to the zero address", () => {
    const holes = extractHoles(ALLOWLIST);
    const out = redactCedar(ALLOWLIST, holes, new Set());
    expect(out).not.toContain("0x91d2000000000000000000000000000000000001");
  });
});
