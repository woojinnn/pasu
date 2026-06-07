import { afterEach, describe, expect, it, vi } from "vitest";

import { sendToExtension } from "./extension-bridge";

describe("extension bridge", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses chrome.runtime.sendMessage when the dashboard runs inside the extension", async () => {
    const payload = { type: "dashboard:ping" };
    const sendMessage = vi
      .fn()
      .mockResolvedValue({ ok: true, data: { version: 1 } });
    vi.stubGlobal("chrome", { runtime: { sendMessage } });

    await expect(sendToExtension(payload, 10)).resolves.toEqual({ version: 1 });
    expect(sendMessage).toHaveBeenCalledWith(payload);
  });

  it("surfaces direct runtime error envelopes", async () => {
    const sendMessage = vi.fn().mockResolvedValue({
      ok: false,
      error: { kind: "parse_failed", message: "bad cedar" },
    });
    vi.stubGlobal("chrome", { runtime: { sendMessage } });

    await expect(
      sendToExtension({ type: "dashboard:put-raw" }, 10),
    ).rejects.toMatchObject({
      name: "ExtensionBridgeError",
      kind: "parse_failed",
      message: "bad cedar",
    });
  });
});
