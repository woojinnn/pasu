import { beforeEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({ sendToExtension: vi.fn() }));
vi.mock("./extension-bridge", () => bridge);

import * as ps2 from "./policy-store";

beforeEach(() => bridge.sendToExtension.mockReset());

describe("policy-store client", () => {
  it("getOverview sends ps2:get-overview and returns data verbatim", async () => {
    const snap = {
      library: { schemaVersion: 1, defs: {}, packages: {} },
      wallets: { schemaVersion: 1, byAddress: {} },
      rev: 3,
    };
    bridge.sendToExtension.mockResolvedValueOnce(snap);
    await expect(ps2.getOverview()).resolves.toEqual(snap);
    expect(bridge.sendToExtension).toHaveBeenCalledWith({ type: "ps2:get-overview" });
  });

  it("write helpers pass the full message shape", async () => {
    bridge.sendToExtension.mockResolvedValue(null);
    await ps2.bindDef({ defId: "def::a", packageId: "pkg::x", addresses: ["0xA1"], params: { cap: 1 } });
    expect(bridge.sendToExtension).toHaveBeenCalledWith({
      type: "ps2:bind",
      defId: "def::a",
      packageId: "pkg::x",
      addresses: ["0xA1"],
      params: { cap: 1 },
    });
    await ps2.setPackageEnabled({ address: "0xa1", packageId: "pkg::x", enabled: false });
    expect(bridge.sendToExtension).toHaveBeenLastCalledWith({
      type: "ps2:set-package-enabled",
      address: "0xa1",
      packageId: "pkg::x",
      enabled: false,
    });
    await ps2.provisionWallets(["0xa1"]);
    expect(bridge.sendToExtension).toHaveBeenLastCalledWith({ type: "ps2:provision-wallets", addresses: ["0xa1"] });
  });

  it("duplicateDef returns the new def id", async () => {
    bridge.sendToExtension.mockResolvedValueOnce("def::new");
    await expect(ps2.duplicateDef("def::a")).resolves.toBe("def::new");
    expect(bridge.sendToExtension).toHaveBeenCalledWith({ type: "ps2:duplicate-def", defId: "def::a" });
  });

  it("write errors propagate (no fail-soft)", async () => {
    bridge.sendToExtension.mockRejectedValueOnce(new Error("ps2_failed"));
    await expect(ps2.deleteDef("def::a")).rejects.toThrow("ps2_failed");
  });
});
