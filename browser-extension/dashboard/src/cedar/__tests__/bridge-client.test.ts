import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the bridge BEFORE importing the module under test. `vi.hoisted` lets the
// mock fn exist above the hoisted vi.mock factory.
const { sendToExtension } = vi.hoisted(() => ({ sendToExtension: vi.fn() }));
vi.mock("../../server-api/extension-bridge", () => ({
  sendToExtension,
  ExtensionBridgeTimeout: class ExtensionBridgeTimeout extends Error {},
}));

import { textToBlocks, blocksToText } from "../index";
import type { PolicyIR } from "../blocks";
import realPolicies from "../blocks/__tests__/fixtures/real-policies-est.json";

const SWAP_PERMIT = `permit(principal, action == Action::"Swap", resource) when { context.slippageBp <= 50 };`;

beforeEach(() => sendToExtension.mockReset());

describe("textToBlocks", () => {
  it("parses the EST envelope and returns one PolicyIR per policy", async () => {
    // The SW resolves with the raw wasm JSON string (the bridge `data` field).
    // Use a real shipped EST so estToBlocks gets a faithful structure.
    const est = (realPolicies as { est: unknown }[])[0].est;
    sendToExtension.mockResolvedValue(JSON.stringify({ ok: true, policies: [{ id: "policy0", est }] }));

    const irs = await textToBlocks(SWAP_PERMIT);
    expect(sendToExtension).toHaveBeenCalledWith(
      expect.objectContaining({ type: "cedar-text-to-est", text: SWAP_PERMIT }),
      expect.any(Number),
    );
    expect(irs).toHaveLength(1);
    expect(irs[0].kind).toBe("policy");
  });

  it("throws on a wasm error envelope", async () => {
    sendToExtension.mockResolvedValue(JSON.stringify({ ok: false, error: "parse error" }));
    await expect(textToBlocks("permit(")).rejects.toThrow(/parse error/);
  });
});

describe("blocksToText", () => {
  it("serializes IR → EST → sends est_json → returns text", async () => {
    sendToExtension.mockResolvedValue(JSON.stringify({ ok: true, text: SWAP_PERMIT }));
    const ir: PolicyIR = {
      kind: "policy",
      effect: "permit",
      annotations: [],
      scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
      conditions: [{ kind: "when", body: { kind: "lit", litType: "bool", value: true } }],
    };
    const text = await blocksToText(ir);
    expect(sendToExtension).toHaveBeenCalledWith(
      expect.objectContaining({ type: "cedar-est-to-text" }),
      expect.any(Number),
    );
    expect(text).toBe(SWAP_PERMIT);
  });
});
