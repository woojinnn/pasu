import { afterEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => {
  class ExtensionBridgeTimeout extends Error {
    constructor(message: string) {
      super(message);
      this.name = "ExtensionBridgeTimeout";
    }
  }
  return {
    sendToExtension: vi.fn(),
    ExtensionBridgeTimeout,
  };
});

vi.mock("./extension-bridge", () => bridge);

import {
  ExtensionBridgeTimeout,
  sendToExtension,
} from "./extension-bridge";
import {
  clearCurrentUser,
  deletePolicy,
  deletePolicySet,
  listManagedPolicies,
  putPolicy,
  putPolicySet,
  setCurrentUser,
  setEnabledPolicyIds,
} from "./extension-sync";

describe("extension sync", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("keeps read calls fail-soft when the extension bridge is unavailable", async () => {
    vi.mocked(sendToExtension).mockRejectedValue(
      new ExtensionBridgeTimeout("missing bridge"),
    );

    await expect(listManagedPolicies()).resolves.toEqual([]);
  });

  it("does not treat put timeouts as successful writes", async () => {
    vi.mocked(sendToExtension).mockRejectedValue(
      new ExtensionBridgeTimeout("missing bridge"),
    );

    await expect(
      putPolicy({
        id: "dashboard::draft-cedar-test",
        cedarText: "forbid (principal, action, resource);",
      }),
    ).rejects.toMatchObject({ name: "ExtensionBridgeTimeout" });
  });

  it.each([
    ["deletePolicy", () => deletePolicy("dashboard::draft-cedar-test")],
    ["setEnabledPolicyIds", () => setEnabledPolicyIds(["dashboard::draft-cedar-test"])],
    [
      "putPolicySet",
      () =>
        putPolicySet({
          id: "dashboard-set::market-pack",
          displayName: "Market pack",
          memberIds: ["dashboard::draft-cedar-test"],
        }),
    ],
    ["deletePolicySet", () => deletePolicySet("dashboard-set::market-pack")],
    ["setCurrentUser", () => setCurrentUser("user-1")],
    ["clearCurrentUser", () => clearCurrentUser()],
  ])("does not treat %s timeouts as successful writes", async (_name, act) => {
    vi.mocked(sendToExtension).mockRejectedValue(
      new ExtensionBridgeTimeout("missing bridge"),
    );

    await expect(act()).rejects.toMatchObject({ name: "ExtensionBridgeTimeout" });
  });
});
